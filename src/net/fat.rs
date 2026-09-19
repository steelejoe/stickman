//! Tiny FAT12 RAM disk for the USB MSC gadget (README + WIFI.JSON).

use crate::config::{parse_wifi_json, WifiCreds};

pub const SECTOR: usize = 512;
pub const SECTORS: usize = 64;
pub const DISK_LEN: usize = SECTOR * SECTORS;

const RESERVED: u16 = 1;
const FAT_COUNT: u8 = 2;
const FAT_SECS: u16 = 1;
const ROOT_ENTRIES: u16 = 16;
const ROOT_SECS: u16 = 1; // 16 * 32 / 512
const DATA_START: u16 = RESERVED + FAT_COUNT as u16 * FAT_SECS + ROOT_SECS;

const CLUSTER_README: u16 = 2;
const CLUSTER_WIFI: u16 = 3;

const README: &[u8] = b"Stickman Wi-Fi config (2.4 GHz only).\r\n\
Edit WIFI.JSO then eject this drive:\r\n\
{\"ssid\":\"YourNetwork\",\"password\":\"secret\"}\r\n";

const WIFI_TEMPLATE: &[u8] = b"{\n  \"ssid\": \"\",\n  \"password\": \"\"\n}\n";

/// Format `disk` as FAT12 with README.TXT and WIFI.JSON.
pub fn format_disk(disk: &mut [u8; DISK_LEN]) {
    disk.fill(0);
    write_boot(disk);
    write_fat(disk);
    write_dir_entry(disk, 0, b"STICKMAN   ", 0x08, 0, 0);
    write_dir_entry(
        disk,
        1,
        b"README  TXT",
        0x21,
        CLUSTER_README,
        README.len() as u32,
    );
    write_dir_entry(
        disk,
        2,
        b"WIFI    JSO",
        0x20,
        CLUSTER_WIFI,
        WIFI_TEMPLATE.len() as u32,
    );
    write_cluster(disk, CLUSTER_README, README);
    write_cluster(disk, CLUSTER_WIFI, WIFI_TEMPLATE);
}

pub fn read_sector(disk: &[u8; DISK_LEN], lba: u32, dest: &mut [u8]) {
    let start = (lba as usize).saturating_mul(SECTOR);
    dest.fill(0);
    if start >= DISK_LEN {
        return;
    }
    let n = dest.len().min(SECTOR).min(DISK_LEN - start);
    dest[..n].copy_from_slice(&disk[start..start + n]);
}

pub fn write_sector(disk: &mut [u8; DISK_LEN], lba: u32, src: &[u8]) {
    let start = (lba as usize).saturating_mul(SECTOR);
    if start >= DISK_LEN {
        return;
    }
    let n = src.len().min(SECTOR).min(DISK_LEN - start);
    disk[start..start + n].copy_from_slice(&src[..n]);
}

/// After a host write, try to parse WIFI.JSON from the RAM disk.
pub fn take_wifi_json(disk: &[u8; DISK_LEN]) -> Option<WifiCreds> {
    let (cluster, size) = find_wifi_file(disk)?;
    if size == 0 || size > 1024 {
        return None;
    }
    let mut buf = [0u8; 1024];
    let n = size.min(buf.len());
    read_cluster_bytes(disk, cluster, &mut buf[..n]);
    // Trim trailing NULs from a preallocated cluster.
    let end = buf[..n]
        .iter()
        .rposition(|&b| b != 0)
        .map(|i| i + 1)
        .unwrap_or(0);
    parse_wifi_json(&buf[..end]).ok()
}

fn write_boot(disk: &mut [u8; DISK_LEN]) {
    let b = &mut disk[0..SECTOR];
    b[0] = 0xEB;
    b[1] = 0x3C;
    b[2] = 0x90;
    b[3..11].copy_from_slice(b"MSDOS5.0");
    b[11..13].copy_from_slice(&(SECTOR as u16).to_le_bytes());
    b[13] = 1; // sectors per cluster
    b[14..16].copy_from_slice(&RESERVED.to_le_bytes());
    b[16] = FAT_COUNT;
    b[17..19].copy_from_slice(&ROOT_ENTRIES.to_le_bytes());
    b[19..21].copy_from_slice(&(SECTORS as u16).to_le_bytes());
    b[21] = 0xF8;
    b[22..24].copy_from_slice(&FAT_SECS.to_le_bytes());
    b[24..26].copy_from_slice(&1u16.to_le_bytes());
    b[26..28].copy_from_slice(&1u16.to_le_bytes());
    b[36] = 0x80;
    b[38] = 0x29;
    b[39..43].copy_from_slice(&0x1234_5678u32.to_le_bytes());
    b[43..54].copy_from_slice(b"STICKMAN   ");
    b[54..62].copy_from_slice(b"FAT12   ");
    b[510] = 0x55;
    b[511] = 0xAA;
}

fn write_fat(disk: &mut [u8; DISK_LEN]) {
    // Media + EOC + two files (single-cluster each).
    let entries = [0xFF8u16, 0xFFF, 0xFFF, 0xFFF];
    for fat_i in 0..FAT_COUNT {
        let base = SECTOR * (RESERVED as usize + fat_i as usize * FAT_SECS as usize);
        let fat = &mut disk[base..base + SECTOR];
        // packed FAT12
        // 0: FF8, 1: FFF -> bytes F8 FF FF
        fat[0] = 0xF8;
        fat[1] = 0xFF;
        fat[2] = 0xFF;
        // cluster 2 = FFF, cluster 3 = FFF
        put_fat12(fat, 2, entries[2]);
        put_fat12(fat, 3, entries[3]);
    }
}

fn put_fat12(fat: &mut [u8], cluster: u16, value: u16) {
    let i = (cluster as usize * 3) / 2;
    if cluster & 1 == 0 {
        fat[i] = value as u8;
        fat[i + 1] = (fat[i + 1] & 0xF0) | ((value >> 8) as u8 & 0x0F);
    } else {
        fat[i] = (fat[i] & 0x0F) | ((value << 4) as u8);
        fat[i + 1] = (value >> 4) as u8;
    }
}

fn write_dir_entry(
    disk: &mut [u8; DISK_LEN],
    index: usize,
    name: &[u8; 11],
    attr: u8,
    cluster: u16,
    size: u32,
) {
    let root = SECTOR * (RESERVED as usize + FAT_COUNT as usize * FAT_SECS as usize);
    let e = &mut disk[root + index * 32..root + index * 32 + 32];
    e[0..11].copy_from_slice(name);
    e[11] = attr;
    e[26..28].copy_from_slice(&cluster.to_le_bytes());
    e[28..32].copy_from_slice(&size.to_le_bytes());
}

fn cluster_lba(cluster: u16) -> u16 {
    DATA_START + (cluster - 2)
}

fn write_cluster(disk: &mut [u8; DISK_LEN], cluster: u16, data: &[u8]) {
    let lba = cluster_lba(cluster) as usize;
    let start = lba * SECTOR;
    let n = data.len().min(SECTOR);
    disk[start..start + n].copy_from_slice(&data[..n]);
}

fn read_cluster_bytes(disk: &[u8; DISK_LEN], cluster: u16, dest: &mut [u8]) {
    let mut left = dest.len();
    let mut off = 0;
    let mut c = cluster;
    while left > 0 && (2..0xFF0).contains(&c) {
        let lba = cluster_lba(c) as usize;
        let start = lba * SECTOR;
        let n = left.min(SECTOR);
        dest[off..off + n].copy_from_slice(&disk[start..start + n]);
        off += n;
        left -= n;
        c = next_cluster(disk, c);
    }
}

fn next_cluster(disk: &[u8; DISK_LEN], cluster: u16) -> u16 {
    let fat = &disk[SECTOR..SECTOR * 2];
    let i = (cluster as usize * 3) / 2;
    if cluster & 1 == 0 {
        u16::from(fat[i]) | ((u16::from(fat[i + 1]) & 0x0F) << 8)
    } else {
        (u16::from(fat[i]) >> 4) | (u16::from(fat[i + 1]) << 4)
    }
}

fn find_wifi_file(disk: &[u8; DISK_LEN]) -> Option<(u16, usize)> {
    let root = SECTOR * (RESERVED as usize + FAT_COUNT as usize * FAT_SECS as usize);
    for i in 0..ROOT_ENTRIES as usize {
        let e = &disk[root + i * 32..root + i * 32 + 32];
        if e[0] == 0x00 || e[0] == 0xE5 || e[11] & 0x18 != 0 {
            continue;
        }
        let name = &e[0..11];
        let is_wifi = name.starts_with(b"WIFI") || name[8..11] == *b"JSO" || name[8..11] == *b"JSN";
        if !is_wifi {
            continue;
        }
        let cluster = u16::from_le_bytes([e[26], e[27]]);
        let size = u32::from_le_bytes([e[28], e[29], e[30], e[31]]) as usize;
        return Some((cluster, size));
    }
    None
}
