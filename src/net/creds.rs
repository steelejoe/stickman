//! Persist `wifi.json` credentials in the last flash sector.

use crate::config::{parse_wifi_json, WifiCreds};
use embedded_storage::{ReadStorage, Storage};
use esp_hal::peripherals::FLASH;
use esp_println::println;
use esp_storage::FlashStorage;

const MAGIC: [u8; 4] = *b"STWF";
const SECTOR: u32 = 4096;

/// Load credentials from the last 4 KiB of flash, if present.
pub fn load(flash: FLASH<'static>) -> (FlashStorage<'static>, Option<WifiCreds>) {
    let mut storage = FlashStorage::new(flash).multicore_auto_park();
    let offset = match last_sector(&storage) {
        Some(o) => o,
        None => return (storage, None),
    };
    let mut buf = [0u8; 256];
    if storage.read(offset, &mut buf).is_err() {
        println!("wifi.json: flash read failed");
        return (storage, None);
    }
    if buf[0..4] != MAGIC {
        return (storage, None);
    }
    let n = buf[4] as usize;
    if n == 0 || 5 + n > buf.len() {
        return (storage, None);
    }
    match parse_wifi_json(&buf[5..5 + n]) {
        Ok(creds) => {
            println!("wifi.json: loaded SSID from flash");
            (storage, Some(creds))
        }
        Err(_) => (storage, None),
    }
}

/// Write the raw JSON (or a reconstructed object) into the last flash sector.
pub fn save(storage: &mut FlashStorage<'static>, creds: &WifiCreds) {
    let Some(offset) = last_sector(storage) else {
        println!("wifi.json: flash too small to persist");
        return;
    };
    let mut json = heapless::String::<160>::new();
    if core::fmt::Write::write_fmt(
        &mut json,
        format_args!(
            "{{\"ssid\":\"{}\",\"password\":\"{}\"}}",
            creds.ssid, creds.password
        ),
    )
    .is_err()
    {
        return;
    }
    let mut sector = [0xFFu8; SECTOR as usize];
    sector[0..4].copy_from_slice(&MAGIC);
    sector[4] = json.len() as u8;
    sector[5..5 + json.len()].copy_from_slice(json.as_bytes());
    match storage.write(offset, &sector) {
        Ok(()) => println!("wifi.json: persisted SSID to flash"),
        Err(e) => println!("wifi.json: flash write failed: {:?}", e),
    }
}

fn last_sector(storage: &FlashStorage<'static>) -> Option<u32> {
    let cap = storage.capacity() as u32;
    if cap < SECTOR {
        return None;
    }
    Some(cap - SECTOR)
}
