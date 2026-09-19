//! Station + SoftAP fallback and embassy-net stacks (core 1).

use super::http;
use crate::config::{write_ipv4, NetMode, WifiCreds, SOFTAP_IP, SOFTAP_SSID};
use crate::net::{set_status, CREDS_CH, CREDS_SIGNAL};
use alloc::string::String;
use embassy_net::{Config, Ipv4Address, Ipv4Cidr, Stack, StackResources, StaticConfigV4};
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_radio::wifi::{
    AccessPointConfig, AuthMethod, ClientConfig, Interfaces, ModeConfig, WifiController, WifiEvent,
};
use static_cell::StaticCell;

const AP_IP: Ipv4Address = Ipv4Address::new(192, 168, 4, 1);

/// Bring up SoftAP always, plus STA when credentials exist.
#[embassy_executor::task]
pub async fn run(
    mut controller: WifiController<'static>,
    interfaces: Interfaces<'static>,
    seed: u64,
    initial: Option<WifiCreds>,
) -> ! {
    let ap_cfg = AccessPointConfig::default().with_ssid(String::from(SOFTAP_SSID));

    static AP_RES: StaticCell<StackResources<4>> = StaticCell::new();
    static STA_RES: StaticCell<StackResources<4>> = StaticCell::new();

    let (ap_stack, ap_runner) = embassy_net::new(
        interfaces.ap,
        Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(AP_IP, 24),
            gateway: Some(AP_IP),
            dns_servers: Default::default(),
        }),
        AP_RES.init(StackResources::new()),
        seed,
    );
    let (sta_stack, sta_runner) = embassy_net::new(
        interfaces.sta,
        Config::dhcpv4(Default::default()),
        STA_RES.init(StackResources::new()),
        seed ^ 0xA5A5_A5A5_A5A5_A5A5,
    );

    let spawner = unsafe { embassy_executor::Spawner::for_current_executor().await };
    spawner.spawn(net_run(ap_runner)).ok();
    spawner.spawn(net_run_sta(sta_runner)).ok();
    spawner.spawn(http::serve(ap_stack, "softap")).ok();
    spawner.spawn(http::serve(sta_stack, "station")).ok();
    spawner.spawn(dhcp_server(ap_stack)).ok();

    let mut creds = initial;
    loop {
        apply_mode(&mut controller, creds.as_ref(), &ap_cfg).await;
        watch_links(ap_stack, sta_stack, creds.as_ref()).await;

        // Wait until USB drops a new wifi.json, then reconfigure.
        CREDS_SIGNAL.wait().await;
        while let Ok(next) = CREDS_CH.try_receive() {
            println!("WiFi: new credentials SSID={}", next.ssid);
            creds = Some(next);
        }
        let _ = controller.stop_async().await;
    }
}

async fn apply_mode(
    controller: &mut WifiController<'static>,
    creds: Option<&WifiCreds>,
    ap_cfg: &AccessPointConfig,
) {
    let mode = if let Some(c) = creds {
        let mut client = ClientConfig::default()
            .with_ssid(String::from(c.ssid.as_str()))
            .with_password(String::from(c.password.as_str()));
        if c.password.is_empty() {
            client = client.with_auth_method(AuthMethod::None);
        }
        println!("WiFi: AP+STA (2.4 GHz) SSID={}", c.ssid);
        ModeConfig::ApSta(client, ap_cfg.clone())
    } else {
        println!("WiFi: SoftAP only ({SOFTAP_SSID} / http://{SOFTAP_IP}/)");
        ModeConfig::AccessPoint(ap_cfg.clone())
    };

    if let Err(e) = controller.set_config(&mode) {
        set_status(|s| {
            s.last_error.clear();
            let _ = core::fmt::Write::write_fmt(&mut s.last_error, format_args!("{e:?}"));
        })
        .await;
        println!("WiFi: set_config failed: {e:?}");
        return;
    }
    if let Err(e) = controller.start_async().await {
        println!("WiFi: start failed: {e:?}");
        return;
    }
    if creds.is_some() {
        match controller.connect_async().await {
            Ok(()) => println!("WiFi: station associated"),
            Err(e) => {
                println!("WiFi: station join failed ({e:?}); SoftAP still up");
                set_status(|s| {
                    s.mode = NetMode::SoftAp;
                    s.ssid.clear();
                    let _ = s.ssid.push_str(SOFTAP_SSID);
                    s.ip.clear();
                    let _ = s.ip.push_str(SOFTAP_IP);
                    s.last_error.clear();
                    let _ = core::fmt::Write::write_fmt(
                        &mut s.last_error,
                        format_args!("STA failed: {e:?} (5 GHz? use 2.4 GHz)"),
                    );
                })
                .await;
            }
        }
    } else {
        set_status(|s| {
            s.mode = NetMode::SoftAp;
            s.ssid.clear();
            let _ = s.ssid.push_str(SOFTAP_SSID);
            s.ip.clear();
            let _ = s.ip.push_str(SOFTAP_IP);
            s.last_error.clear();
        })
        .await;
    }
    let _ = controller.wait_for_event(WifiEvent::ApStart);
}

async fn watch_links(ap: Stack<'static>, sta: Stack<'static>, creds: Option<&WifiCreds>) {
    // Refresh status while waiting for new credentials (or a few seconds after connect).
    for _ in 0..20 {
        if CREDS_SIGNAL.signaled() {
            break;
        }
        if let Some(cfg) = sta.config_v4() {
            let oct = cfg.address.address().octets();
            set_status(|s| {
                s.mode = NetMode::Station;
                s.ssid.clear();
                if let Some(c) = creds {
                    let _ = s.ssid.push_str(c.ssid.as_str());
                }
                write_ipv4(&mut s.ip, oct);
                s.last_error.clear();
            })
            .await;
        } else if ap.config_v4().is_some() {
            set_status(|s| {
                if s.mode == NetMode::Off {
                    s.mode = NetMode::SoftAp;
                }
                if s.ssid.is_empty() {
                    let _ = s.ssid.push_str(SOFTAP_SSID);
                }
                if s.ip.is_empty() {
                    let _ = s.ip.push_str(SOFTAP_IP);
                }
            })
            .await;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[embassy_executor::task]
async fn net_run(mut runner: embassy_net::Runner<'static, esp_radio::wifi::WifiDevice<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_run_sta(
    mut runner: embassy_net::Runner<'static, esp_radio::wifi::WifiDevice<'static>>,
) -> ! {
    runner.run().await
}

/// One-client DHCP so a phone on stickman-setup gets 192.168.4.2.
#[embassy_executor::task]
async fn dhcp_server(stack: Stack<'static>) -> ! {
    let mut rx_meta = [PacketMetadata::EMPTY; 2];
    let mut tx_meta = [PacketMetadata::EMPTY; 2];
    let mut rx = [0u8; 600];
    let mut tx = [0u8; 600];
    let mut sock = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    let _ = sock.bind(67);
    let offer_ip = [192, 168, 4, 2];
    let server_ip = [192, 168, 4, 1];
    loop {
        let mut buf = [0u8; 600];
        let (n, meta) = match sock.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(_) => {
                Timer::after_millis(20).await;
                continue;
            }
        };
        if n < 240 {
            continue;
        }
        let msg_type = dhcp_msg_type(&buf[..n]);
        let xid = &buf[4..8];
        let chaddr = &buf[28..44];
        let mut reply = [0u8; 300];
        reply[..n.min(240)].copy_from_slice(&buf[..n.min(240)]);
        reply[0] = 2; // BOOTREPLY
        reply[16..20].copy_from_slice(&offer_ip);
        reply[20..24].copy_from_slice(&server_ip);
        reply[236..240].copy_from_slice(&[99, 130, 83, 99]);
        let mut o = 240;
        reply[o] = 53;
        reply[o + 1] = 1;
        reply[o + 2] = if msg_type == 3 { 5 } else { 2 }; // ACK or OFFER
        o += 3;
        reply[o] = 1;
        reply[o + 1] = 4;
        reply[o + 2..o + 6].copy_from_slice(&[255, 255, 255, 0]);
        o += 6;
        reply[o] = 3;
        reply[o + 1] = 4;
        reply[o + 2..o + 6].copy_from_slice(&server_ip);
        o += 6;
        reply[o] = 54;
        reply[o + 1] = 4;
        reply[o + 2..o + 6].copy_from_slice(&server_ip);
        o += 6;
        reply[o] = 51;
        reply[o + 1] = 4;
        reply[o + 2..o + 6].copy_from_slice(&86400u32.to_be_bytes());
        o += 6;
        reply[o] = 255;
        o += 1;
        let _ = xid;
        let _ = chaddr;
        let dest = embassy_net::IpEndpoint::new(
            embassy_net::IpAddress::Ipv4(Ipv4Address::BROADCAST),
            68,
        );
        let _ = sock.send_to(&reply[..o], dest).await;
        let _ = meta;
    }
}

fn dhcp_msg_type(pkt: &[u8]) -> u8 {
    let mut i = 240;
    while i + 2 <= pkt.len() {
        let opt = pkt[i];
        if opt == 255 {
            break;
        }
        if opt == 0 {
            i += 1;
            continue;
        }
        let len = pkt[i + 1] as usize;
        if opt == 53 && i + 2 < pkt.len() {
            return pkt[i + 2];
        }
        i += 2 + len;
    }
    1
}
