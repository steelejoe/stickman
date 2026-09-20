//! Dual-core Wi-Fi website + USB `wifi.json` (core 1).
//!
//! Radio, USB, and the second core stay off until [`try_start`] (config button).

pub mod creds;
pub mod fat;
pub mod http;
pub mod usb;
pub mod wifi;

use crate::config::{ConfigCmd, NetMode, NetStatus, WifiCreds};
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use esp_hal::interrupt::software::SoftwareInterrupt;
use esp_hal::peripherals::{CPU_CTRL, FLASH, GPIO19, GPIO20, USB0, WIFI};
use esp_hal::system::Stack;
use esp_println::println;
use esp_radio::wifi::{Interfaces, WifiController};
use esp_rtos::embassy::Executor;
use static_cell::StaticCell;

static STARTED: AtomicBool = AtomicBool::new(false);
static HOLD: BlockingMutex<CriticalSectionRawMutex, RefCell<Option<WifiHold>>> =
    BlockingMutex::new(RefCell::new(None));

pub static CONFIG_CH: Channel<CriticalSectionRawMutex, ConfigCmd, 4> = Channel::new();
pub static CREDS_CH: Channel<CriticalSectionRawMutex, WifiCreds, 2> = Channel::new();
pub static NET_STATUS: Mutex<CriticalSectionRawMutex, NetStatus> = Mutex::new(NetStatus {
    mode: NetMode::Off,
    ssid: heapless::String::new(),
    ip: heapless::String::new(),
    last_error: heapless::String::new(),
});
/// Wakes the Wi-Fi task after USB writes new credentials.
pub static CREDS_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

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
    let radio = match esp_radio::init() {
        Ok(c) => c,
        Err(e) => {
            println!("WiFi: radio init failed: {e:?}");
            STARTED.store(false, Ordering::Release);
            HOLD.lock(|slot| *slot.borrow_mut() = Some(hold));
            return false;
        }
    };
    static RADIO: StaticCell<esp_radio::Controller<'static>> = StaticCell::new();
    let radio = RADIO.init(radio);

    let (controller, interfaces) = match esp_radio::wifi::new(radio, hold.wifi, Default::default()) {
        Ok(v) => v,
        Err(e) => {
            println!("WiFi: controller failed: {e:?}");
            STARTED.store(false, Ordering::Release);
            return false;
        }
    };

    let args = Core1Args {
        controller,
        interfaces,
        usb0: hold.usb0,
        usb_dp: hold.usb_dp,
        usb_dm: hold.usb_dm,
        flash: hold.flash,
        seed: hold.seed,
    };

    const CORE1_STACK: usize = 32 * 1024;
    static STACK: StaticCell<Stack<CORE1_STACK>> = StaticCell::new();
    println!("Starting core 1 (USB + HTTP + embassy-net)");
    esp_rtos::start_second_core(
        hold.cpu_ctrl,
        hold.sw0,
        hold.sw1,
        STACK.init(Stack::new()),
        move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            EXECUTOR.init(Executor::new()).run(move |spawner| {
                spawner.spawn(core1_main(args)).ok();
            });
        },
    );
    true
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

    let (storage, creds) = creds::load(flash);
    static FLASH: StaticCell<esp_storage::FlashStorage<'static>> = StaticCell::new();
    let storage = FLASH.init(storage);

    let spawner = unsafe { embassy_executor::Spawner::for_current_executor().await };
    spawner
        .spawn(wifi::run(controller, interfaces, seed, creds))
        .ok();
    usb::run(usb0, usb_dp, usb_dm, storage).await;
}

/// Non-blocking take of a website command (core 0 game loop).
pub fn try_recv_config() -> Option<ConfigCmd> {
    CONFIG_CH.try_receive().ok()
}

/// Publish a status snapshot for `/api/status`.
pub async fn set_status(update: impl FnOnce(&mut NetStatus)) {
    let mut g = NET_STATUS.lock().await;
    update(&mut g);
}

pub fn submit_creds(creds: WifiCreds) {
    let _ = CREDS_CH.try_send(creds);
    CREDS_SIGNAL.signal(());
}

pub fn submit_config(cmd: ConfigCmd) {
    let _ = CONFIG_CH.try_send(cmd);
}
