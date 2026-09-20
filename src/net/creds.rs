//! Persist `wifi.json` credentials in the last flash sector.

use crate::config::{parse_wifi_json, WifiCreds};
use crate::speech::{parse_speech_json, SpeechAction, SpeechConfig};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embedded_storage::{ReadStorage, Storage};
use esp_hal::peripherals::FLASH;
use esp_println::println;
use esp_storage::FlashStorage;

const MAGIC: [u8; 4] = *b"STWF";
const SECTOR: u32 = 4096;
/// 16-byte firmware stamp at the end of the 256-byte wifi header.
const BUILD_ID_OFF: usize = 240;
const BUILD_ID: [u8; 16] = *include_bytes!(concat!(env!("OUT_DIR"), "/stickman_build_id.bin"));

static FLASH: Mutex<CriticalSectionRawMutex, Option<FlashStorage<'static>>> = Mutex::new(None);

/// Load credentials from the last 4 KiB of flash, if present.
///
/// A new firmware image (different build stamp) clears Wi-Fi, speech, and rooms.
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
    if buf[BUILD_ID_OFF..BUILD_ID_OFF + 16] != BUILD_ID {
        println!("config: new firmware image — clearing saved Wi-Fi, speech, and rooms");
        wipe_persisted(&mut storage);
        return (storage, None);
    }
    if buf[0..4] != MAGIC {
        return (storage, None);
    }
    let n = buf[4] as usize;
    if n == 0 || 5 + n > buf.len() || 5 + n > BUILD_ID_OFF {
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

pub async fn install(storage: FlashStorage<'static>) {
    *FLASH.lock().await = Some(storage);
}

pub async fn persist(creds: &WifiCreds) {
    let mut g = FLASH.lock().await;
    if let Some(storage) = g.as_mut() {
        save(storage, creds);
    }
}

pub async fn persist_clear() {
    let mut g = FLASH.lock().await;
    if let Some(storage) = g.as_mut() {
        clear(storage);
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
    write_wifi_sector(storage, offset, Some(json.as_bytes()));
    println!("wifi.json: persisted SSID to flash");
}

/// Erase stored station credentials.
pub fn clear(storage: &mut FlashStorage<'static>) {
    let Some(offset) = last_sector(storage) else {
        println!("wifi.json: flash too small to clear");
        return;
    };
    write_wifi_sector(storage, offset, None);
    println!("wifi.json: cleared");
}

fn write_wifi_sector(storage: &mut FlashStorage<'static>, offset: u32, json: Option<&[u8]>) {
    let mut sector = alloc::vec![0xFFu8; SECTOR as usize];
    sector[0..4].copy_from_slice(&MAGIC);
    if let Some(json) = json {
        if json.len() + 5 <= BUILD_ID_OFF {
            sector[4] = json.len() as u8;
            sector[5..5 + json.len()].copy_from_slice(json);
        }
    } else {
        sector[4] = 0;
    }
    sector[BUILD_ID_OFF..BUILD_ID_OFF + 16].copy_from_slice(&BUILD_ID);
    if let Err(e) = storage.write(offset, &sector) {
        println!("wifi.json: flash write failed: {e:?}");
    }
}

fn wipe_persisted(storage: &mut FlashStorage<'static>) {
    if let Some(offset) = last_sector(storage) {
        write_wifi_sector(storage, offset, None);
    }
    clear_speech(storage);
    clear_rooms_meta(storage);
}

fn last_sector(storage: &FlashStorage<'static>) -> Option<u32> {
    let cap = storage.capacity() as u32;
    if cap < SECTOR {
        return None;
    }
    Some(cap - SECTOR)
}

const SPEECH_MAGIC: [u8; 4] = *b"STSP";

fn speech_sector(storage: &FlashStorage<'static>) -> Option<u32> {
    let last = last_sector(storage)?;
    last.checked_sub(SECTOR)
}

/// Load speech banks from the sector below `wifi.json`.
pub fn read_speech(storage: &mut FlashStorage<'static>) -> SpeechConfig {
    let Some(offset) = speech_sector(storage) else {
        return SpeechConfig::default();
    };
    let mut buf = [0u8; SECTOR as usize];
    if storage.read(offset, &mut buf).is_err() {
        return SpeechConfig::default();
    }
    if buf[0..4] != SPEECH_MAGIC {
        return SpeechConfig::default();
    }
    let n = u16::from_le_bytes([buf[4], buf[5]]) as usize;
    if n == 0 || 6 + n > buf.len() {
        return SpeechConfig::default();
    }
    match parse_speech_json(&buf[6..6 + n]) {
        Ok(SpeechAction::Save(cfg)) => cfg,
        _ => SpeechConfig::default(),
    }
}

pub fn write_speech(storage: &mut FlashStorage<'static>, cfg: &SpeechConfig) {
    let Some(offset) = speech_sector(storage) else {
        println!("speech: flash too small");
        return;
    };
    let json = cfg.to_json();
    if json.len() + 6 > SECTOR as usize {
        println!("speech: json too large");
        return;
    }
    let mut sector = alloc::vec![0xFFu8; SECTOR as usize];
    sector[0..4].copy_from_slice(&SPEECH_MAGIC);
    let n = json.len() as u16;
    sector[4..6].copy_from_slice(&n.to_le_bytes());
    sector[6..6 + json.len()].copy_from_slice(json.as_bytes());
    match storage.write(offset, &sector) {
        Ok(()) => println!("speech: persisted banks"),
        Err(e) => println!("speech: flash write failed: {e:?}"),
    }
}

pub fn clear_speech(storage: &mut FlashStorage<'static>) {
    let Some(offset) = speech_sector(storage) else {
        return;
    };
    let sector = alloc::vec![0xFFu8; SECTOR as usize];
    match storage.write(offset, &sector) {
        Ok(()) => println!("speech: cleared"),
        Err(e) => println!("speech: flash clear failed: {e:?}"),
    }
}

pub async fn persist_speech(cfg: &SpeechConfig) {
    let mut g = FLASH.lock().await;
    if let Some(storage) = g.as_mut() {
        write_speech(storage, cfg);
    }
}

pub async fn persist_speech_clear() {
    let mut g = FLASH.lock().await;
    if let Some(storage) = g.as_mut() {
        clear_speech(storage);
    }
}

const ROOM_MAGIC: [u8; 4] = *b"STRM";
const IMAGE_MAGIC: [u8; 4] = *b"STIM";
const IMAGE_SLOT: u32 = 60 * SECTOR;
const NAME_BYTES: usize = crate::room::ROOM_NAME_MAX;

fn rooms_meta_offset(storage: &FlashStorage<'static>) -> Option<u32> {
    last_sector(storage)?.checked_sub(2 * SECTOR)
}

fn image_slot_offset(storage: &FlashStorage<'static>, i: usize) -> Option<u32> {
    if i >= crate::room::MAX_ROOMS {
        return None;
    }
    let meta = rooms_meta_offset(storage)?;
    let base = meta.checked_sub(IMAGE_SLOT * crate::room::MAX_ROOMS as u32)?;
    Some(base + i as u32 * IMAGE_SLOT)
}

/// Copy an SM65 blob into a reused PSRAM slot and return a `'static` view.
pub fn install_room_sm65(i: usize, sm65: &[u8]) -> Result<&'static [u8], &'static str> {
    if i >= crate::room::MAX_ROOMS {
        return Err("room");
    }
    let img = crate::assets::Rgb565Image::from_sm65(sm65).ok_or("sm65")?;
    if img.width != crate::room::ROOM_BG_W || img.height != crate::room::ROOM_BG_H {
        return Err("size");
    }
    if sm65.len() > crate::room::SM65_ROOM_LEN {
        return Err("too large");
    }
    let slot = room_ram_slot(i);
    slot[..sm65.len()].copy_from_slice(sm65);
    Ok(&slot[..sm65.len()])
}

fn room_ram_slot(i: usize) -> &'static mut [u8] {
    const CAP: usize = crate::room::SM65_ROOM_LEN;
    static mut PTRS: [*mut u8; crate::room::MAX_ROOMS] =
        [core::ptr::null_mut(); crate::room::MAX_ROOMS];
    unsafe {
        if PTRS[i].is_null() {
            let leaked = alloc::vec![0u8; CAP].into_boxed_slice();
            let s = alloc::boxed::Box::leak(leaked);
            PTRS[i] = s.as_mut_ptr();
        }
        core::slice::from_raw_parts_mut(PTRS[i], CAP)
    }
}

/// Load room names/count and any saved SM65 backdrops.
pub fn read_rooms(
    storage: &mut FlashStorage<'static>,
) -> (
    crate::room::RoomsConfig,
    [Option<&'static [u8]>; crate::room::MAX_ROOMS],
) {
    let mut cfg = crate::room::RoomsConfig::default();
    let mut images = [None; crate::room::MAX_ROOMS];
    let Some(offset) = rooms_meta_offset(storage) else {
        return (cfg, images);
    };
    let mut buf = [0u8; SECTOR as usize];
    if storage.read(offset, &mut buf).is_err() || buf[0..4] != ROOM_MAGIC {
        return (cfg, images);
    }
    cfg.count = buf[4].clamp(1, crate::room::MAX_ROOMS as u8);
    let flags = buf[5];
    let names_off = 8;
    for i in 0..crate::room::MAX_ROOMS {
        let start = names_off + i * NAME_BYTES;
        let raw = &buf[start..start + NAME_BYTES];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(NAME_BYTES);
        let text = core::str::from_utf8(&raw[..end]).unwrap_or("");
        cfg.names[i] = {
            let mut n = crate::room::RoomName::new();
            if text.is_empty() {
                let _ = n.push_str(crate::room::default_room_name(i));
            } else {
                let _ = n.push_str(text);
            }
            n
        };
        if flags & (1 << i) != 0 {
            images[i] = load_room_image(storage, i);
        }
    }
    (cfg, images)
}

fn load_room_image(storage: &mut FlashStorage<'static>, i: usize) -> Option<&'static [u8]> {
    let offset = image_slot_offset(storage, i)?;
    let mut hdr = [0u8; 8];
    storage.read(offset, &mut hdr).ok()?;
    if hdr[0..4] != IMAGE_MAGIC {
        return None;
    }
    let n = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
    if n < 12 || n > crate::room::SM65_ROOM_LEN {
        return None;
    }
    let mut body = alloc::vec![0u8; n];
    storage.read(offset + 8, &mut body).ok()?;
    install_room_sm65(i, &body).ok()
}

pub fn write_rooms(
    storage: &mut FlashStorage<'static>,
    cfg: &crate::room::RoomsConfig,
    images: &[Option<&'static [u8]>; crate::room::MAX_ROOMS],
) {
    let Some(offset) = rooms_meta_offset(storage) else {
        println!("rooms: flash too small");
        return;
    };
    let mut sector = alloc::vec![0xFFu8; SECTOR as usize];
    sector[0..4].copy_from_slice(&ROOM_MAGIC);
    sector[4] = cfg.clamped_count();
    let mut flags = 0u8;
    for (i, img) in images.iter().enumerate() {
        if img.is_some() {
            flags |= 1 << i;
        }
    }
    sector[5] = flags;
    let names_off = 8;
    for i in 0..crate::room::MAX_ROOMS {
        let start = names_off + i * NAME_BYTES;
        let bytes = cfg.names[i].as_bytes();
        sector[start..start + bytes.len()].copy_from_slice(bytes);
    }
    match storage.write(offset, &sector) {
        Ok(()) => println!("rooms: persisted names"),
        Err(e) => println!("rooms: flash write failed: {e:?}"),
    }
}

fn clear_rooms_meta(storage: &mut FlashStorage<'static>) {
    let Some(offset) = rooms_meta_offset(storage) else {
        return;
    };
    let sector = alloc::vec![0xFFu8; SECTOR as usize];
    match storage.write(offset, &sector) {
        Ok(()) => println!("rooms: cleared"),
        Err(e) => println!("rooms: flash clear failed: {e:?}"),
    }
}

pub fn write_room_image(storage: &mut FlashStorage<'static>, i: usize, sm65: &[u8]) {
    let Some(offset) = image_slot_offset(storage, i) else {
        println!("rooms: no image slot");
        return;
    };
    if sm65.len() > crate::room::SM65_ROOM_LEN {
        println!("rooms: image too large");
        return;
    }
    let mut buf = alloc::vec![0xFFu8; SECTOR as usize];
    buf[0..4].copy_from_slice(&IMAGE_MAGIC);
    buf[4..8].copy_from_slice(&(sm65.len() as u32).to_le_bytes());
    let first = (SECTOR as usize - 8).min(sm65.len());
    buf[8..8 + first].copy_from_slice(&sm65[..first]);
    if let Err(e) = storage.write(offset, &buf) {
        println!("rooms: image write failed: {e:?}");
        return;
    }
    let mut done = first;
    let mut pos = offset + SECTOR;
    while done < sm65.len() {
        buf.fill(0xFF);
        let n = (sm65.len() - done).min(SECTOR as usize);
        buf[..n].copy_from_slice(&sm65[done..done + n]);
        if let Err(e) = storage.write(pos, &buf) {
            println!("rooms: image write failed: {e:?}");
            return;
        }
        done += n;
        pos += SECTOR;
    }
    println!("rooms: persisted image {i}");
}

pub async fn persist_rooms(
    cfg: &crate::room::RoomsConfig,
    images: &[Option<&'static [u8]>; crate::room::MAX_ROOMS],
) {
    let mut g = FLASH.lock().await;
    if let Some(storage) = g.as_mut() {
        write_rooms(storage, cfg, images);
    }
}

pub async fn persist_room_image(i: usize, sm65: &[u8]) {
    let mut g = FLASH.lock().await;
    if let Some(storage) = g.as_mut() {
        write_room_image(storage, i, sm65);
    }
}
