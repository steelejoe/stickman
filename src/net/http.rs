//! Tiny HTTP server for the config website (core 1).

use crate::config::{parse_config_json, NetStatus, SOFTAP_IP};
use crate::net::{submit_config, NET_STATUS};
use core::fmt::Write as _;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use esp_println::println;

const INDEX: &[u8] = br##"<!doctype html>
<html><head><meta charset=utf-8><meta name=viewport content="width=device-width,initial-scale=1">
<title>Stickman</title>
<style>body{font-family:sans-serif;max-width:36rem;margin:2rem auto;padding:0 1rem}label,button{display:block;margin:.6rem 0}pre{background:#f4f4f4;padding:1rem}</style>
</head><body>
<h1>Stickman</h1>
<p>2.4 GHz Wi-Fi only. USB: drop WIFI.JSO ({"ssid":"...","password":"..."}).</p>
<pre id=st>loading...</pre>
<form id=f>
<label>Backdrop color <input name=backdrop value="#112233" placeholder="#RRGGBB"></label>
<button>Apply color</button>
</form>
<button type=button id=clr>Clear color (restore image)</button>
<script>
async function st(){try{document.getElementById('st').textContent=JSON.stringify(await(await fetch('/api/status')).json(),null,2)}catch(e){document.getElementById('st').textContent=String(e)}}
st();setInterval(st,3000);
document.getElementById('f').onsubmit=async e=>{e.preventDefault();
await fetch('/api/config',{method:'POST',headers:{'content-type':'application/json'},
body:JSON.stringify({backdrop:e.target.backdrop.value})});st()};
document.getElementById('clr').onclick=async()=>{await fetch('/api/config',{method:'POST',headers:{'content-type':'application/json'},body:'{"clear":true}'});st()};
</script>
</body></html>"##;

/// Serve GET / , GET /api/status, POST /api/config on port 80.
#[embassy_executor::task(pool_size = 2)]
pub async fn serve(stack: Stack<'static>, label: &'static str) -> ! {
    let mut rx = [0u8; 2048];
    let mut tx = [0u8; 2048];
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(10)));
        if socket.accept(80).await.is_err() {
            Timer::after_millis(50).await;
            continue;
        }
        println!("HTTP: {label} client");
        let mut req = [0u8; 1536];
        let n = match socket.read(&mut req).await {
            Ok(0) | Err(_) => {
                socket.close();
                continue;
            }
            Ok(n) => n,
        };
        handle(&mut socket, &req[..n]).await;
        socket.close();
        let _ = socket.flush().await;
    }
}

async fn handle(socket: &mut TcpSocket<'_>, req: &[u8]) {
    let text = core::str::from_utf8(req).unwrap_or("");
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => {
            send(socket, b"200 OK", b"text/html; charset=utf-8", INDEX).await;
        }
        ("GET", "/api/status") => {
            let body = status_json().await;
            send(socket, b"200 OK", b"application/json", body.as_bytes()).await;
        }
        ("POST", "/api/config") => {
            let body = extract_body(req);
            match parse_config_json(body) {
                Ok(cmd) => {
                    submit_config(cmd);
                    send(socket, b"200 OK", b"application/json", br#"{"ok":true}"#).await;
                }
                Err(e) => {
                    let mut msg = heapless::String::<80>::new();
                    let _ = write!(msg, "{{\"ok\":false,\"error\":\"{e}\"}}");
                    send(socket, b"400 Bad Request", b"application/json", msg.as_bytes()).await;
                }
            }
        }
        _ => send(socket, b"404 Not Found", b"text/plain", b"not found").await,
    }
}

fn extract_body(req: &[u8]) -> &[u8] {
    if let Some(i) = find_sub(req, b"\r\n\r\n") {
        &req[i + 4..]
    } else {
        b""
    }
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

async fn status_json() -> heapless::String<256> {
    let g = NET_STATUS.lock().await;
    format_status(&g)
}

fn format_status(s: &NetStatus) -> heapless::String<256> {
    let mut out = heapless::String::new();
    let _ = write!(
        out,
        "{{\"mode\":\"{}\",\"ssid\":\"{}\",\"ip\":\"{}\",\"error\":\"{}\",\"hint\":\"2.4GHz only. SoftAP {} if station fails.\"}}",
        s.mode.as_str(),
        s.ssid,
        s.ip,
        s.last_error,
        SOFTAP_IP
    );
    out
}

async fn send(socket: &mut TcpSocket<'_>, status: &[u8], ctype: &[u8], body: &[u8]) {
    let mut head = heapless::String::<192>::new();
    let _ = write!(
        head,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        core::str::from_utf8(status).unwrap_or("200 OK"),
        core::str::from_utf8(ctype).unwrap_or("text/plain"),
        body.len()
    );
    let _ = socket.write(head.as_bytes()).await;
    let _ = socket.write(body).await;
}
