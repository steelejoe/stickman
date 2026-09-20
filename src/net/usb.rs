//! USB CDC (serial) + MSC (FAT12 drive for `wifi.json`).

use super::fat::{self, DISK_LEN, SECTOR};
use crate::net::{creds, submit_creds};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::{Driver, EndpointIn, EndpointOut};
use embassy_usb::msos::{self, windows_version};
use embassy_usb::{Builder, Config, Handler};
use esp_hal::otg_fs::{asynch, Usb};
use esp_hal::peripherals::{GPIO19, GPIO20, USB0};
use esp_println::println;
use esp_storage::FlashStorage;
use static_cell::StaticCell;

const WIFI_JSON_HINT_SECTORS: u32 = 6; // data clusters around WIFI.JSON

struct UsbHandler;

impl Handler for UsbHandler {
    fn enabled(&mut self, _enabled: bool) {}
}

/// Run the USB gadget forever (core 1).
pub async fn run(
    usb0: USB0<'static>,
    usb_dp: GPIO20<'static>,
    usb_dm: GPIO19<'static>,
    flash: &'static mut FlashStorage<'static>,
) -> ! {
    static DISK: StaticCell<[u8; DISK_LEN]> = StaticCell::new();
    let disk = DISK.init([0u8; DISK_LEN]);
    fat::format_disk(disk);

    let usb = Usb::new(usb0, usb_dp, usb_dm);
    static EP_OUT: StaticCell<[u8; 1024]> = StaticCell::new();
    let mut otg_cfg = asynch::Config::default();
    otg_cfg.vbus_detection = false;
    let driver = asynch::Driver::new(usb, EP_OUT.init([0; 1024]), otg_cfg);

    let mut config = Config::new(0x303A, 0x40F1);
    config.manufacturer = Some("stickman");
    config.product = Some("Stickman Config");
    config.serial_number = Some("0001");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
    static CDC_STATE: StaticCell<State> = StaticCell::new();
    static HANDLER: StaticCell<UsbHandler> = StaticCell::new();

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        MSOS_DESC.init([0; 256]),
        CONTROL_BUF.init([0; 64]),
    );
    builder.handler(HANDLER.init(UsbHandler));
    builder.msos_descriptor(windows_version::WIN8_1, 0);
    builder.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));

    let mut cdc = CdcAcmClass::new(&mut builder, CDC_STATE.init(State::new()), 64);
    let (mut ep_out, mut ep_in) = alloc_msc(&mut builder);

    let mut usb = builder.build();
    println!("USB: MSC drive + CDC (drop WIFI.JSON, 2.4 GHz only)");

    let usb_fut = usb.run();
    let msc_fut = msc_task(&mut ep_out, &mut ep_in, disk, flash);
    let cdc_fut = cdc_discard(&mut cdc);

    embassy_futures::join::join3(usb_fut, msc_fut, cdc_fut).await;
    loop {
        embassy_time::Timer::after_secs(1).await;
    }
}

fn alloc_msc<'d, D: Driver<'d>>(builder: &mut Builder<'d, D>) -> (D::EndpointOut, D::EndpointIn) {
    let mut func = builder.function(0x08, 0x06, 0x50);
    let mut iface = func.interface();
    let mut alt = iface.alt_setting(0x08, 0x06, 0x50, None);
    let ep_out = alt.endpoint_bulk_out(None, 64);
    let ep_in = alt.endpoint_bulk_in(None, 64);
    drop(func);
    (ep_out, ep_in)
}

async fn cdc_discard<'d, D: Driver<'d>>(cdc: &mut CdcAcmClass<'d, D>) {
    let mut buf = [0u8; 64];
    loop {
        cdc.wait_connection().await;
        while cdc.read_packet(&mut buf).await.is_ok() {}
    }
}

async fn msc_task(
    ep_out: &mut impl EndpointOut,
    ep_in: &mut impl EndpointIn,
    disk: &mut [u8; DISK_LEN],
    flash: &mut FlashStorage<'static>,
) {
    let mut cbw = [0u8; 31];
    loop {
        match ep_out.read(&mut cbw).await {
            Ok(n) if n == 31 && &cbw[0..4] == b"USBC" => {}
            _ => continue,
        }
        let tag = u32::from_le_bytes(cbw[4..8].try_into().unwrap());
        let xfer_len = u32::from_le_bytes(cbw[8..12].try_into().unwrap());
        let flags = cbw[12];
        let cdb_len = cbw[14].min(16) as usize;
        let cdb = &cbw[15..15 + cdb_len];
        let dir_in = flags & 0x80 != 0;

        let (ok, residue) = handle_scsi(cdb, xfer_len, dir_in, ep_out, ep_in, disk, flash).await;

        let mut csw = [0u8; 13];
        csw[0..4].copy_from_slice(b"USBS");
        csw[4..8].copy_from_slice(&tag.to_le_bytes());
        csw[8..12].copy_from_slice(&residue.to_le_bytes());
        csw[12] = if ok { 0 } else { 1 };
        let _ = ep_in.write(&csw).await;
    }
}

async fn handle_scsi(
    cdb: &[u8],
    xfer_len: u32,
    dir_in: bool,
    ep_out: &mut impl EndpointOut,
    ep_in: &mut impl EndpointIn,
    disk: &mut [u8; DISK_LEN],
    flash: &mut FlashStorage<'static>,
) -> (bool, u32) {
    if cdb.is_empty() {
        return (false, xfer_len);
    }
    match cdb[0] {
        0x00 => (true, xfer_len), // TEST UNIT READY
        0x12 => {
            // INQUIRY
            let mut inq = [0u8; 36];
            inq[0] = 0x00;
            inq[1] = 0x80; // RMB
            inq[2] = 0x04;
            inq[3] = 0x02;
            inq[4] = 31;
            inq[8..16].copy_from_slice(b"STICKMAN");
            inq[16..32].copy_from_slice(b"WIFI CONFIG     ");
            inq[32..36].copy_from_slice(b"1.00");
            let n = write_in(ep_in, &inq, xfer_len).await;
            (true, xfer_len.saturating_sub(n))
        }
        0x03 => {
            // REQUEST SENSE
            let mut sense = [0u8; 18];
            sense[0] = 0x70;
            sense[7] = 10;
            let n = write_in(ep_in, &sense, xfer_len).await;
            (true, xfer_len.saturating_sub(n))
        }
        0x1A | 0x5A => {
            let mut mode = [0u8; 8];
            mode[0] = 3;
            mode[4] = 0x08;
            let n = write_in(ep_in, &mode, xfer_len).await;
            (true, xfer_len.saturating_sub(n))
        }
        0x23 | 0x25 => {
            // READ CAPACITY (10)
            let last = (fat::SECTORS as u32 - 1).to_be_bytes();
            let bps = (SECTOR as u32).to_be_bytes();
            let mut cap = [0u8; 8];
            cap[0..4].copy_from_slice(&last);
            cap[4..8].copy_from_slice(&bps);
            let n = write_in(ep_in, &cap, xfer_len).await;
            (true, xfer_len.saturating_sub(n))
        }
        0x1E | 0x1B => (true, xfer_len),
        0x28 => {
            // READ(10)
            let lba = u32::from_be_bytes([cdb[2], cdb[3], cdb[4], cdb[5]]);
            let blocks = u16::from_be_bytes([cdb[7], cdb[8]]) as u32;
            let mut sent = 0u32;
            let mut buf = [0u8; SECTOR];
            for i in 0..blocks {
                fat::read_sector(disk, lba + i, &mut buf);
                sent += write_in(ep_in, &buf, xfer_len - sent).await;
            }
            (true, xfer_len.saturating_sub(sent))
        }
        0x2A => {
            // WRITE(10)
            let lba = u32::from_be_bytes([cdb[2], cdb[3], cdb[4], cdb[5]]);
            let blocks = u16::from_be_bytes([cdb[7], cdb[8]]) as u32;
            let mut got = 0u32;
            let mut buf = [0u8; SECTOR];
            for i in 0..blocks {
                got += read_out(ep_out, &mut buf, xfer_len - got).await;
                fat::write_sector(disk, lba + i, &buf);
            }
            if let Some(creds) = fat::take_wifi_json(disk) {
                if !creds.ssid.is_empty() {
                    println!("USB: got wifi.json SSID={}", creds.ssid);
                    creds::save(flash, &creds);
                    submit_creds(creds);
                }
            }
            let _ = WIFI_JSON_HINT_SECTORS;
            let _ = dir_in;
            (true, xfer_len.saturating_sub(got))
        }
        _ => {
            if dir_in && xfer_len > 0 {
                let zeros = [0u8; 64];
                let _ = write_in(ep_in, &zeros, xfer_len.min(64)).await;
            } else if !dir_in && xfer_len > 0 {
                let mut dump = [0u8; 64];
                let mut left = xfer_len;
                while left > 0 {
                    let n = read_out(ep_out, &mut dump, left.min(64)).await;
                    if n == 0 {
                        break;
                    }
                    left -= n;
                }
            }
            (false, 0)
        }
    }
}

async fn write_in(ep: &mut impl EndpointIn, data: &[u8], max: u32) -> u32 {
    let n = (data.len() as u32).min(max) as usize;
    if n == 0 {
        return 0;
    }
    let _ = ep.write(&data[..n]).await;
    n as u32
}

async fn read_out(ep: &mut impl EndpointOut, dest: &mut [u8], max: u32) -> u32 {
    let n = dest.len().min(max as usize);
    if n == 0 {
        return 0;
    }
    match ep.read(&mut dest[..n]).await {
        Ok(got) => got as u32,
        Err(_) => 0,
    }
}
