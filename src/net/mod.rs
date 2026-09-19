//! Dual-core Wi-Fi website + USB `wifi.json` (core 1).
//!
//! Radio init happens on core 0; this module's embassy tasks run on core 1.

pub mod creds;
pub mod fat;
pub mod http;
pub mod usb;
pub mod wifi;

use crate::config::{ConfigCmd, NetMode, NetStatus, WifiCreds};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use esp_hal::peripherals::{FLASH, GPIO19, GPIO20, USB0};
use esp_radio::wifi::{Interfaces, WifiController};

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
