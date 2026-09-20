//! Dual-core Wi-Fi website + USB `wifi.json` (core 1).
//!
//! Radio, USB, and the second core stay off until [`try_start`] (config button).

pub mod creds;
pub mod fat;
pub mod http;
pub mod usb;
pub mod wifi;

use crate::config::{ConfigCmd, NetMode, NetStatus, WifiCreds};
use crate::room::{RoomsConfig, RoomsUpdate};
use crate::speech::SpeechConfig;
use core::cell::RefCell;
use core::fmt::Write as _;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use esp_hal::interrupt::software::SoftwareInterrupt;
use esp_hal::peripherals::{CPU_CTRL, FLASH, GPIO19, GPIO20, USB0, WIFI};
use esp_hal::system::Stack;
use esp_println::println;
use esp_radio::wifi::{Interfaces, WifiController};
use esp_rtos::embassy::Executor;
use static_cell::StaticCell;

/// Names/count plus optional SM65 blobs for each configured room.
#[derive(Clone)]
pub struct RoomStore {
    pub cfg: RoomsConfig,
    pub images: [Option<&'static [u8]>; crate::room::MAX_ROOMS],
}

impl Default for RoomStore {
    fn default() -> Self {
        Self {
            cfg: RoomsConfig::default(),
            images: [None; crate::room::MAX_ROOMS],
        }
    }
}

static STARTED: AtomicBool = AtomicBool::new(false);
static RADIO_WANTED: AtomicBool = AtomicBool::new(false);
static HOLD: BlockingMutex<CriticalSectionRawMutex, RefCell<Option<WifiHold>>> =
    BlockingMutex::new(RefCell::new(None));

pub static CONFIG_CH: Channel<CriticalSectionRawMutex, ConfigCmd, 4> = Channel::new();
pub static SPEECH_CH: Channel<CriticalSectionRawMutex, SpeechConfig, 1> = Channel::new();
pub static STORED_SPEECH: Mutex<CriticalSectionRawMutex, Option<SpeechConfig>> = Mutex::new(None);
pub static ROOMS_CH: Channel<CriticalSectionRawMutex, RoomsUpdate, 1> = Channel::new();
pub static STORED_ROOMS: Mutex<CriticalSectionRawMutex, RoomStore> = Mutex::new(RoomStore {
    cfg: RoomsConfig {
        count: 1,
        names: [
            heapless::String::new(),
            heapless::String::new(),
            heapless::String::new(),
        ],
    },
    images: [None; 3],
});
/// Flash writes run on core 0 so auto-park stops Wi-Fi, not the game loop.
#[derive(Clone)]
pub enum PersistJob {
    Wifi(WifiCreds),
    WifiClear,
    Speech(SpeechConfig),
    SpeechClear,
    Rooms,
    RoomImage(usize),
}

pub static PERSIST_CH: Channel<CriticalSectionRawMutex, PersistJob, 2> = Channel::new();
pub static PERSIST_DONE: Channel<CriticalSectionRawMutex, (), 2> = Channel::new();
pub static CREDS_CH: Channel<CriticalSectionRawMutex, Option<WifiCreds>, 2> = Channel::new();
pub static STORED_CREDS: Mutex<CriticalSectionRawMutex, Option<WifiCreds>> = Mutex::new(None);
pub static NET_STATUS: Mutex<CriticalSectionRawMutex, NetStatus> = Mutex::new(NetStatus {
    mode: NetMode::Off,
    ssid: heapless::String::new(),
    ip: heapless::String::new(),
    last_error: heapless::String::new(),
    configured: false,
});
static STATUS_SNAP: BlockingMutex<CriticalSectionRawMutex, RefCell<NetStatus>> =
    BlockingMutex::new(RefCell::new(NetStatus {
        mode: NetMode::Off,
        ssid: heapless::String::new(),
        ip: heapless::String::new(),
        last_error: heapless::String::new(),
        configured: false,
    }));
/// Wakes the Wi-Fi task after USB writes new credentials.
pub static CREDS_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// Wakes Wi-Fi and HTTP tasks when config mode wants the radio on or off.
pub static RADIO_WATCH: Watch<CriticalSectionRawMutex, (), 8> = Watch::new();
/// Core-1 bring-up failures for the game loop to print.
pub static CORE1_ERR: Channel<CriticalSectionRawMutex, heapless::String<96>, 1> = Channel::new();

/// Peripherals parked until a tap enables the radio.
pub struct WifiHold {
    pub wifi: WIFI<'static>,
    pub usb0: USB0<'static>,
    pub usb_dp: GPIO20<'static>,
    pub usb_dm: GPIO19<'static>,
    pub flash: FLASH<'static>,
    pub cpu_ctrl: CPU_CTRL<'static>,
    pub sw0: SoftwareInterrupt<'static, 0>,
    pub sw1: SoftwareInterrupt<'static, 1>,
    pub seed: u64,
}

/// Peripherals and radio objects handed to core 1.
pub struct Core1Args {
    pub controller: WifiController<'static>,
    pub interfaces: Interfaces<'static>,
    pub usb0: USB0<'static>,
    pub usb_dp: GPIO20<'static>,
    pub usb_dm: GPIO19<'static>,
    pub flash: FLASH<'static>,
    pub seed: u64,
}

/// Stash radio/USB parts so the game can boot without them.
pub fn park(hold: WifiHold) {
    HOLD.lock(|slot| {
        *slot.borrow_mut() = Some(hold);
    });
    println!("WiFi: parked (tap CFG to enable)");
}

pub fn is_started() -> bool {
    STARTED.load(Ordering::Acquire)
}

pub fn radio_wanted() -> bool {
    RADIO_WANTED.load(Ordering::Acquire)
}

/// Sleep until CFG asks for the radio. Safe for several tasks at once.
pub async fn wait_radio_wanted() {
    let Some(mut wake) = RADIO_WATCH.receiver() else {
        core::future::pending::<()>().await;
        return;
    };
    while !radio_wanted() {
        wake.changed().await;
    }
}

/// Connect (or stay connected) while config mode is on; stop the radio when off.
pub fn set_wanted(on: bool) {
    RADIO_WANTED.store(on, Ordering::Release);
    RADIO_WATCH.sender().send(());
    if on {
        println!("WiFi: config mode — radio requested");
    } else {
        println!("WiFi: leaving config mode — radio off");
    }
}

/// Non-blocking copy of the last published [`NetStatus`] (core 0 overlay).
pub fn try_status() -> NetStatus {
    STATUS_SNAP.lock(|s| s.borrow().clone())
}

/// Bring up radio + core 1 once. Returns true if this call started it.
pub fn try_start() -> bool {
    if STARTED.swap(true, Ordering::AcqRel) {
        return false;
    }
    let Some(hold) = HOLD.lock(|slot| slot.borrow_mut().take()) else {
        STARTED.store(false, Ordering::Release);
        return false;
    };

    println!("WiFi: enabling from CFG...");
    const CORE1_STACK: usize = 32 * 1024;
    static STACK: StaticCell<Stack<CORE1_STACK>> = StaticCell::new();
    println!("Starting core 1 (radio + HTTP + embassy-net)");
    let WifiHold {
        wifi,
        usb0,
        usb_dp,
        usb_dm,
        flash,
        cpu_ctrl,
        sw0,
        sw1,
        seed,
    } = hold;
    esp_rtos::start_second_core(
        cpu_ctrl,
        sw0,
        sw1,
        STACK.init(Stack::new()),
        move || core1_entry(wifi, usb0, usb_dp, usb_dm, flash, seed),
    );
    true
}

/// Radio init must run here so Wi-Fi tasks pin to core 1, not the game loop.
fn core1_entry(
    wifi: WIFI<'static>,
    usb0: USB0<'static>,
    usb_dp: GPIO20<'static>,
    usb_dm: GPIO19<'static>,
    flash: FLASH<'static>,
    seed: u64,
) {
    let radio = match esp_radio::init() {
        Ok(c) => c,
        Err(e) => {
            report_core1_error(format_args!("radio init failed: {e:?}"));
            loop {}
        }
    };
    println!("WiFi: radio on core 1");
    static RADIO: StaticCell<esp_radio::Controller<'static>> = StaticCell::new();
    let radio = RADIO.init(radio);
    let (controller, interfaces) = match esp_radio::wifi::new(radio, wifi, Default::default()) {
        Ok(v) => v,
        Err(e) => {
            report_core1_error(format_args!("controller failed: {e:?}"));
            loop {}
        }
    };
    let args = Core1Args {
        controller,
        interfaces,
        usb0,
        usb_dp,
        usb_dm,
        flash,
        seed,
    };
    static EXECUTOR: StaticCell<Executor> = StaticCell::new();
    EXECUTOR.init(Executor::new()).run(move |spawner| {
        if spawner.spawn(core1_main(args)).is_err() {
            report_core1_error(format_args!("core1_main spawn failed"));
        }
    });
}

fn report_core1_error(msg: impl core::fmt::Display) {
    let mut s = heapless::String::<96>::new();
    let _ = write!(&mut s, "{msg}");
    println!("WiFi: {s}");
    STATUS_SNAP.lock(|slot| {
        let mut st = slot.borrow_mut();
        st.mode = NetMode::Off;
        st.last_error.clear();
        for c in s.chars() {
            if st.last_error.push(c).is_err() {
                break;
            }
        }
    });
    let _ = CORE1_ERR.try_send(s);
}

/// Non-blocking take of a core-1 bring-up error (core 0 prints / overlay).
pub fn try_recv_core1_error() -> Option<heapless::String<96>> {
    CORE1_ERR.try_receive().ok()
}

#[embassy_executor::task]
async fn core1_main(args: Core1Args) {
    let Core1Args {
        controller,
        interfaces,
        usb0,
        usb_dp,
        usb_dm,
        flash,
        seed,
    } = args;

    let (mut storage, creds) = creds::load(flash);
    let speech = creds::read_speech(&mut storage);
    let (rooms_cfg, room_images) = creds::read_rooms(&mut storage);
    creds::install(storage).await;
    set_stored_speech(speech.clone()).await;
    let _ = SPEECH_CH.try_send(speech);
    set_stored_rooms(RoomStore {
        cfg: rooms_cfg,
        images: room_images,
    })
    .await;
    submit_rooms_now().await;
    // Do not start the USB MSC/CDC gadget. This board's USB-C is the same
    // USB0/GPIO19/20 pair that USB-Serial-JTAG (espflash --monitor) uses;
    // claiming it as OTG disconnects the host (`Broken pipe`).
    let _ = (usb0, usb_dp, usb_dm);

    let spawner = unsafe { embassy_executor::Spawner::for_current_executor().await };
    spawner
        .spawn(wifi::run(controller, interfaces, seed, creds))
        .ok();
    println!("WiFi: core 1 up (radio follows config mode); USB serial left for the monitor");
    core::future::pending::<()>().await;
}

/// Non-blocking take of a website command (core 0 game loop).
pub fn try_recv_config() -> Option<ConfigCmd> {
    CONFIG_CH.try_receive().ok()
}

pub fn try_recv_persist() -> Option<PersistJob> {
    PERSIST_CH.try_receive().ok()
}

async fn queue_persist(job: PersistJob) {
    PERSIST_CH.send(job).await;
    PERSIST_DONE.receive().await;
}

/// Run a flash persist job on core 0 (parks core 1 for the write).
pub async fn run_persist(job: PersistJob) {
    match job {
        PersistJob::Wifi(creds) => creds::persist(&creds).await,
        PersistJob::WifiClear => creds::persist_clear().await,
        PersistJob::Speech(cfg) => creds::persist_speech(&cfg).await,
        PersistJob::SpeechClear => creds::persist_speech_clear().await,
        PersistJob::Rooms => {
            let g = STORED_ROOMS.lock().await.clone();
            creds::persist_rooms(&g.cfg, &g.images).await;
        }
        PersistJob::RoomImage(i) => {
            let g = STORED_ROOMS.lock().await.clone();
            if let Some(bytes) = g.images.get(i).copied().flatten() {
                creds::persist_room_image(i, bytes).await;
            }
            creds::persist_rooms(&g.cfg, &g.images).await;
        }
    }
    let _ = PERSIST_DONE.try_send(());
}

/// Publish a status snapshot for `/api/status` and the config-mode overlay.
pub async fn set_status(update: impl FnOnce(&mut NetStatus)) {
    let mut g = NET_STATUS.lock().await;
    update(&mut g);
    STATUS_SNAP.lock(|s| *s.borrow_mut() = g.clone());
}

pub async fn save_wifi(creds: WifiCreds) {
    set_stored_creds(Some(creds.clone())).await;
    queue_persist(PersistJob::Wifi(creds)).await;
    println!("WiFi: credentials saved (apply on next Config)");
}

pub async fn reset_wifi() {
    set_stored_creds(None).await;
    queue_persist(PersistJob::WifiClear).await;
    println!("WiFi: credentials cleared (apply on next Config)");
}

pub fn submit_creds(creds: WifiCreds) {
    let _ = CREDS_CH.try_send(Some(creds));
    CREDS_SIGNAL.signal(());
}

pub async fn set_stored_creds(creds: Option<WifiCreds>) {
    *STORED_CREDS.lock().await = creds;
}

pub fn submit_config(cmd: ConfigCmd) {
    let _ = CONFIG_CH.try_send(cmd);
}

pub fn try_recv_speech() -> Option<SpeechConfig> {
    SPEECH_CH.try_receive().ok()
}

pub async fn set_stored_speech(cfg: SpeechConfig) {
    *STORED_SPEECH.lock().await = Some(cfg);
}

pub async fn stored_speech() -> SpeechConfig {
    STORED_SPEECH.lock().await.clone().unwrap_or_default()
}

pub async fn save_speech(cfg: SpeechConfig) {
    set_stored_speech(cfg.clone()).await;
    let _ = SPEECH_CH.try_receive();
    let _ = SPEECH_CH.try_send(cfg.clone());
    queue_persist(PersistJob::Speech(cfg)).await;
    println!("speech: saved");
}

pub async fn reset_speech() {
    let cfg = SpeechConfig::default();
    set_stored_speech(cfg.clone()).await;
    let _ = SPEECH_CH.try_receive();
    let _ = SPEECH_CH.try_send(cfg);
    queue_persist(PersistJob::SpeechClear).await;
    println!("speech: reset to defaults");
}

pub fn try_recv_rooms() -> Option<RoomsUpdate> {
    ROOMS_CH.try_receive().ok()
}

pub async fn set_stored_rooms(store: RoomStore) {
    *STORED_ROOMS.lock().await = store;
}

pub async fn stored_rooms() -> RoomStore {
    let g = STORED_ROOMS.lock().await.clone();
    if g.cfg.names[0].is_empty() {
        RoomStore::default()
    } else {
        g
    }
}

fn rooms_update_from(store: &RoomStore) -> RoomsUpdate {
    let mut images = [None; crate::room::MAX_ROOMS];
    images[0] = store.images[0]
        .or_else(crate::assets::firmware_background_sm65)
        .and_then(crate::assets::Rgb565Image::from_sm65);
    for i in 1..crate::room::MAX_ROOMS {
        images[i] = store.images[i].and_then(crate::assets::Rgb565Image::from_sm65);
    }
    RoomsUpdate {
        count: store.cfg.clamped_count(),
        names: store.cfg.names.clone(),
        images,
    }
}

async fn submit_rooms_now() {
    let update = rooms_update_from(&STORED_ROOMS.lock().await.clone());
    let _ = ROOMS_CH.try_receive();
    let _ = ROOMS_CH.try_send(update);
}

pub async fn save_rooms(cfg: RoomsConfig) {
    {
        let mut g = STORED_ROOMS.lock().await;
        g.cfg = cfg;
    }
    submit_rooms_now().await;
    queue_persist(PersistJob::Rooms).await;
    println!("rooms: saved");
}

pub async fn reset_rooms() {
    set_stored_rooms(RoomStore::default()).await;
    submit_rooms_now().await;
    queue_persist(PersistJob::Rooms).await;
    println!("rooms: reset to Home");
}

pub async fn save_room_image(i: usize, sm65: &[u8]) -> Result<(), &'static str> {
    let leaked = creds::install_room_sm65(i, sm65)?;
    {
        let mut g = STORED_ROOMS.lock().await;
        g.images[i] = Some(leaked);
    }
    submit_rooms_now().await;
    queue_persist(PersistJob::RoomImage(i)).await;
    Ok(())
}

pub async fn room_image_bytes(i: usize) -> Option<&'static [u8]> {
    if i >= crate::room::MAX_ROOMS {
        return None;
    }
    let g = STORED_ROOMS.lock().await;
    g.images[i]
        .or(g.images[0])
        .or_else(crate::assets::firmware_background_sm65)
}
