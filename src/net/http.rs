//! Tiny HTTP server for the config website (core 1).

use crate::config::{parse_config_json, parse_wifi_action, NetStatus, WifiAction, WifiCreds};
use crate::net::{
    reset_rooms, reset_speech, reset_wifi, room_image_bytes, save_room_image, save_rooms,
    save_speech, save_wifi, stored_rooms, stored_speech, submit_config, NET_STATUS, STORED_CREDS,
};
use crate::room::{parse_rooms_json, RoomsAction, SM65_ROOM_LEN};
use crate::speech::{parse_speech_json, SpeechAction};
use core::fmt::Write as _;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use esp_println::println;

const INDEX: &[u8] = br##"<!doctype html>
<html><head><meta charset=utf-8><meta name=viewport content="width=device-width,initial-scale=1">
<title>Stickman</title>
<style>body{font-family:sans-serif;max-width:28rem;margin:1.5rem auto;padding:0 1rem}label,button{display:block;margin:.55rem 0}input,textarea{width:100%;box-sizing:border-box;padding:.45rem}textarea{min-height:6rem;font:inherit}p.row{display:flex;gap:.5rem}p.row button{flex:1}h2{margin:1.4rem 0 .4rem}#m,#sm,#rm{min-height:1.3em}.pw{display:flex;gap:.4rem;align-items:center}.pw input{flex:1;min-width:0}.pw button{margin:0;flex:0 0 auto;padding:.3rem .45rem;line-height:0}.pw button svg{display:block}.pw button .slash{display:none}.pw button.on .slash{display:block}.room{border:1px solid #ccc;padding:.7rem;margin:.7rem 0}.room img{width:100%;max-width:480px;aspect-ratio:2/1;object-fit:contain;background:#111;display:block;margin-top:.4rem}</style>
</head><body>
<h1>Stickman</h1>
<h2>Wi-Fi</h2>
<p>2.4 GHz only. Save the network this device should join.</p>
<form id=w>
<label>Network name (SSID) <input name=ssid maxlength=32 autocomplete=off></label>
<label>Password <span class=pw><input name=password type=password maxlength=64><button type=button id=pw aria-label=Show title=Show><svg viewBox="0 0 24 24" width=22 height=22 fill=none stroke=currentColor stroke-width=2><path d="M1 12s4-7 11-7 11 7 11 7-4 7-11 7S1 12 1 12z"/><circle cx=12 cy=12 r=3/><path class=slash d="M3 3l18 18"/></svg></button></span></label>
<p class=row>
<button>Save</button>
<button type=button id=r>Reset</button>
</p>
</form>
<p id=m></p>
<h2>Speech</h2>
<p>One phrase per line (max 16 characters). Talk % is the chance a tap or loop starts a line.</p>
<form id=sp>
<label>Man talk % <input name=man_talk type=number min=0 max=100></label>
<label>Man phrases <textarea name=man_lines></textarea></label>
<label>Dog talk % <input name=dog_talk type=number min=0 max=100></label>
<label>Dog phrases <textarea name=dog_lines></textarea></label>
<label>Crate talk % <input name=box_talk type=number min=0 max=100></label>
<label>Crate phrases <textarea name=box_lines></textarea></label>
<p class=row>
<button>Save</button>
<button type=button id=sr>Reset</button>
</p>
</form>
<p id=sm></p>
<h2>Rooms</h2>
<p>Home is first. Add up to two more. Pick a background on this device; it is shown in its original format, scaled to 480x240, converted to RGB565, and uploaded when you save.</p>
<div id=rooms></div>
<p class=row><button type=button id=addRoom>Add Room</button></p>
<p class=row><button type=button id=saveRooms>Save</button><button type=button id=resetRooms>Reset</button></p>
<p id=rm></p>
<script>
const w=document.getElementById('w'),m=document.getElementById('m');
const sp=document.getElementById('sp'),sm=document.getElementById('sm');
async function load(){
  const j=await(await fetch('/api/status')).json();
  w.ssid.value=j.ssid||'';
  w.password.value=j.password||'';
  const s=await(await fetch('/api/speech')).json();
  sp.man_talk.value=s.man.talk; sp.man_lines.value=s.man.lines||'';
  sp.dog_talk.value=s.dog.talk; sp.dog_lines.value=s.dog.lines||'';
  sp.box_talk.value=s.box.talk; sp.box_lines.value=s.box.lines||'';
  await loadRooms();
}
load();
document.getElementById('pw').onclick=()=>{
  const p=w.password,b=document.getElementById('pw'),show=p.type==='password';
  p.type=show?'text':'password';
  b.classList.toggle('on',show);
  b.title=show?'Hide':'Show';
  b.setAttribute('aria-label',b.title);
};
w.onsubmit=async e=>{
  e.preventDefault();
  m.textContent='Saving...';
  const j=await(await fetch('/api/wifi',{method:'POST',headers:{'content-type':'application/json'},
    body:JSON.stringify({ssid:w.ssid.value,password:w.password.value})})).json();
  m.textContent=j.ok?'Saved. Takes effect the next time you open Config.':('Error: '+(j.error||'failed'));
};
document.getElementById('r').onclick=async()=>{
  m.textContent='Resetting...';
  await fetch('/api/wifi',{method:'POST',headers:{'content-type':'application/json'},body:'{"reset":true}'});
  w.ssid.value='';w.password.value='';
  m.textContent='Cleared. Takes effect the next time you open Config.';
};
sp.onsubmit=async e=>{
  e.preventDefault();
  sm.textContent='Saving...';
  const j=await(await fetch('/api/speech',{method:'POST',headers:{'content-type':'application/json'},
    body:JSON.stringify({
      man:{talk:+sp.man_talk.value,lines:sp.man_lines.value},
      dog:{talk:+sp.dog_talk.value,lines:sp.dog_lines.value},
      box:{talk:+sp.box_talk.value,lines:sp.box_lines.value}
    })})).json();
  sm.textContent=j.ok?'Saved.':('Error: '+(j.error||'failed'));
};
document.getElementById('sr').onclick=async()=>{
  sm.textContent='Resetting...';
  await fetch('/api/speech',{method:'POST',headers:{'content-type':'application/json'},body:'{"reset":true}'});
  sm.textContent='Restored defaults.';
  load();
};
const roomsEl=document.getElementById('rooms'),addBtn=document.getElementById('addRoom'),rm=document.getElementById('rm');
let rooms=[],homePreview='';
function packSm65(id){
  const w=480,h=240,ox=56,oy=0,pix=new Uint8Array(12+w*h*2);
  pix[0]=83;pix[1]=77;pix[2]=54;pix[3]=53;
  pix[4]=w&255;pix[5]=w>>8;pix[6]=h&255;pix[7]=h>>8;
  pix[8]=ox&255;pix[9]=ox>>8;pix[10]=oy&255;pix[11]=oy>>8;
  const d=id.data;let o=12;
  for(let i=0;i<w*h;i++){
    const r=d[i*4],g=d[i*4+1],b=d[i*4+2];
    const c=((r&0xF8)<<8)|((g&0xFC)<<3)|(b>>3);
    pix[o++]=c>>8;pix[o++]=c&255;
  }
  return pix;
}
async function fileToSm65(file){
  const url=URL.createObjectURL(file);
  const img=new Image();
  await new Promise((res,rej)=>{img.onload=res;img.onerror=rej;img.src=url;});
  const c=document.createElement('canvas');c.width=480;c.height=240;
  c.getContext('2d').drawImage(img,0,0,480,240);
  return {url,sm65:packSm65(c.getContext('2d').getImageData(0,0,480,240))};
}
function sm65ToUrl(buf){
  const u=new Uint8Array(buf);
  if(u.length<12)return '';
  const w=u[4]|(u[5]<<8),h=u[6]|(u[7]<<8);
  const c=document.createElement('canvas');c.width=w;c.height=h;
  const ctx=c.getContext('2d'),id=ctx.createImageData(w,h);
  let o=12;
  for(let i=0;i<w*h;i++){
    const v=(u[o]<<8)|u[o+1];o+=2;
    id.data[i*4]=((v>>11)&31)*255/31;
    id.data[i*4+1]=((v>>5)&63)*255/63;
    id.data[i*4+2]=(v&31)*255/31;
    id.data[i*4+3]=255;
  }
  ctx.putImageData(id,0,0);
  return c.toDataURL('image/png');
}
async function fetchImage(i){
  const r=await fetch('/api/room/'+i+'/image');
  if(!r.ok)return '';
  return sm65ToUrl(await r.arrayBuffer());
}
function renderRooms(){
  roomsEl.innerHTML='';
  rooms.forEach((room,i)=>{
    const d=document.createElement('div');d.className='room';
    const name=document.createElement('input');name.maxLength=24;name.value=room.name;name.dataset.i=i;name.className='rn';
    const nl=document.createElement('label');nl.textContent='Name ';nl.appendChild(name);
    const file=document.createElement('input');file.type='file';file.accept='image/*';file.dataset.i=i;file.className='rf';
    const fl=document.createElement('label');fl.textContent='Background ';fl.appendChild(file);
    d.appendChild(nl);d.appendChild(fl);
    if(room.preview){const img=document.createElement('img');img.alt='';img.src=room.preview;d.appendChild(img);}
    if(i){
      const p=document.createElement('p');p.className='row';
      const b=document.createElement('button');b.type='button';b.className='rmv';b.dataset.i=i;b.textContent='Remove';
      p.appendChild(b);d.appendChild(p);
    }
    roomsEl.appendChild(d);
  });
  addBtn.disabled=rooms.length>=3;
}
roomsEl.oninput=e=>{
  if(e.target.classList.contains('rn')) rooms[+e.target.dataset.i].name=e.target.value;
};
roomsEl.onchange=async e=>{
  if(!e.target.classList.contains('rf')||!e.target.files[0])return;
  const i=+e.target.dataset.i,conv=await fileToSm65(e.target.files[0]);
  if(rooms[i].objectUrl) URL.revokeObjectURL(rooms[i].objectUrl);
  rooms[i].objectUrl=conv.url;
  rooms[i].preview=conv.url;
  rooms[i].sm65=conv.sm65;
  rooms[i].dirty=true;
  renderRooms();
};
roomsEl.onclick=e=>{
  if(!e.target.classList.contains('rmv'))return;
  const i=+e.target.dataset.i;
  if(rooms[i]&&rooms[i].objectUrl) URL.revokeObjectURL(rooms[i].objectUrl);
  rooms.splice(i,1);
  renderRooms();
};
addBtn.onclick=()=>{
  if(rooms.length>=3)return;
  rooms.push({name:'Room '+(rooms.length+1),preview:homePreview,sm65:null,dirty:false,objectUrl:null});
  renderRooms();
};
async function loadRooms(){
  const j=await(await fetch('/api/rooms')).json();
  homePreview=await fetchImage(0);
  rooms=[];
  const n=Math.max(1,Math.min(3,j.count||1));
  for(let i=0;i<n;i++){
    const preview=i?((await fetchImage(i))||homePreview):homePreview;
    rooms.push({name:j['n'+i]||(i?('Room '+(i+1)):'Home'),preview,sm65:null,dirty:false,objectUrl:null});
  }
  renderRooms();
}
document.getElementById('saveRooms').onclick=async()=>{
  rm.textContent='Saving...';
  try{
    for(let i=0;i<rooms.length;i++){
      if(rooms[i].dirty&&rooms[i].sm65){
        const r=await fetch('/api/room/'+i+'/image',{method:'POST',headers:{'content-type':'application/octet-stream'},body:rooms[i].sm65});
        if(!r.ok){rm.textContent='Error: image '+(i+1);return;}
        rooms[i].dirty=false;
      }
    }
    const body={count:rooms.length,n0:'',n1:'',n2:''};
    rooms.forEach((r,i)=>body['n'+i]=r.name);
    const j=await(await fetch('/api/rooms',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)})).json();
    rm.textContent=j.ok?'Saved.':('Error: '+(j.error||'failed'));
  }catch(e){rm.textContent='Error: '+(e.message||e);}
};
document.getElementById('resetRooms').onclick=async()=>{
  rm.textContent='Resetting...';
  await fetch('/api/rooms',{method:'POST',headers:{'content-type':'application/json'},body:'{"reset":true}'});
  rm.textContent='Restored defaults.';
  loadRooms();
};
</script>
</body></html>"##;

/// Serve GET / , GET /api/status, POST /api/wifi, POST /api/speech, POST /api/config,
/// GET/POST /api/rooms and GET/POST /api/room/N/image on port 80.
#[embassy_executor::task(pool_size = 2)]
pub async fn serve(stack: Stack<'static>, label: &'static str) -> ! {
    let mut rx = [0u8; 4096];
    let mut tx = [0u8; 2048];
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(30)));
        if socket.accept(80).await.is_err() {
            Timer::after_millis(50).await;
            continue;
        }
        println!("HTTP: {label} client");
        handle(&mut socket).await;
        socket.close();
        let _ = socket.flush().await;
    }
}

async fn handle(socket: &mut TcpSocket<'_>) {
    let (headers, body) = match read_http(socket).await {
        Ok(v) => v,
        Err(e) => {
            send_err(socket, e).await;
            return;
        }
    };
    let mut lines = headers.split("\r\n");
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    if let Some(i) = parse_room_image_path(path) {
        match method {
            "GET" => match room_image_bytes(i).await {
                Some(bytes) => {
                    send(socket, b"200 OK", b"application/octet-stream", bytes).await;
                }
                None => send(socket, b"404 Not Found", b"text/plain", b"no image").await,
            },
            "POST" => match save_room_image(i, &body).await {
                Ok(()) => send(socket, b"200 OK", b"application/json", br#"{"ok":true}"#).await,
                Err(e) => send_err(socket, e).await,
            },
            _ => send(socket, b"404 Not Found", b"text/plain", b"not found").await,
        }
        return;
    }

    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => {
            send(socket, b"200 OK", b"text/html; charset=utf-8", INDEX).await;
        }
        ("GET", "/api/status") => {
            let body = status_json().await;
            send(socket, b"200 OK", b"application/json", body.as_bytes()).await;
        }
        ("GET", "/api/speech") => {
            let body = stored_speech().await.to_json();
            send(socket, b"200 OK", b"application/json", body.as_bytes()).await;
        }
        ("GET", "/api/rooms") => {
            let body = rooms_json().await;
            send(socket, b"200 OK", b"application/json", body.as_bytes()).await;
        }
        ("POST", "/api/wifi") => match parse_wifi_action(&body) {
            Ok(WifiAction::Save(creds)) => {
                save_wifi(creds).await;
                send(socket, b"200 OK", b"application/json", br#"{"ok":true}"#).await;
            }
            Ok(WifiAction::Reset) => {
                reset_wifi().await;
                send(socket, b"200 OK", b"application/json", br#"{"ok":true}"#).await;
            }
            Err(e) => send_err(socket, e).await,
        },
        ("POST", "/api/speech") => match parse_speech_json(&body) {
            Ok(SpeechAction::Save(cfg)) => {
                save_speech(cfg).await;
                send(socket, b"200 OK", b"application/json", br#"{"ok":true}"#).await;
            }
            Ok(SpeechAction::Reset) => {
                reset_speech().await;
                send(socket, b"200 OK", b"application/json", br#"{"ok":true}"#).await;
            }
            Err(e) => send_err(socket, e).await,
        },
        ("POST", "/api/rooms") => match parse_rooms_json(&body) {
            Ok(RoomsAction::Save(cfg)) => {
                save_rooms(cfg).await;
                send(socket, b"200 OK", b"application/json", br#"{"ok":true}"#).await;
            }
            Ok(RoomsAction::Reset) => {
                reset_rooms().await;
                send(socket, b"200 OK", b"application/json", br#"{"ok":true}"#).await;
            }
            Err(e) => send_err(socket, e).await,
        },
        ("POST", "/api/config") => match parse_config_json(&body) {
            Ok(cmd) => {
                submit_config(cmd);
                send(socket, b"200 OK", b"application/json", br#"{"ok":true}"#).await;
            }
            Err(e) => send_err(socket, e).await,
        },
        _ => send(socket, b"404 Not Found", b"text/plain", b"not found").await,
    }
}

fn parse_room_image_path(path: &str) -> Option<usize> {
    let rest = path.strip_prefix("/api/room/")?;
    let idx = rest.strip_suffix("/image")?;
    let n: usize = idx.parse().ok()?;
    (n < crate::room::MAX_ROOMS).then_some(n)
}

const MAX_HEADERS: usize = 4096;

async fn read_http(
    socket: &mut TcpSocket<'_>,
) -> Result<(alloc::string::String, alloc::vec::Vec<u8>), &'static str> {
    let mut buf = alloc::vec::Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = match socket.read(&mut tmp).await {
            Ok(0) | Err(_) => return Err("eof"),
            Ok(n) => n,
        };
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_sub(&buf, b"\r\n\r\n") {
            let headers = alloc::string::String::from(
                core::str::from_utf8(&buf[..pos]).map_err(|_| "headers")?,
            );
            let mut body = buf.split_off(pos + 4);
            let cl = content_length(&headers).unwrap_or(0);
            if cl > SM65_ROOM_LEN + 32 {
                return Err("too large");
            }
            while body.len() < cl {
                let n = match socket.read(&mut tmp).await {
                    Ok(0) | Err(_) => return Err("eof"),
                    Ok(n) => n,
                };
                body.extend_from_slice(&tmp[..n]);
            }
            body.truncate(cl);
            return Ok((headers, body));
        }
        if buf.len() > MAX_HEADERS {
            return Err("headers");
        }
    }
}

fn content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                return v.trim().parse().ok();
            }
        }
    }
    None
}

async fn send_err(socket: &mut TcpSocket<'_>, e: impl core::fmt::Display) {
    let mut msg = heapless::String::<80>::new();
    let _ = write!(msg, "{{\"ok\":false,\"error\":\"{e}\"}}");
    send(
        socket,
        b"400 Bad Request",
        b"application/json",
        msg.as_bytes(),
    )
    .await;
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

async fn rooms_json() -> alloc::string::String {
    let store = stored_rooms().await;
    let cfg = store.cfg;
    let mut out = alloc::string::String::new();
    let _ = write!(
        out,
        "{{\"count\":{},\"n0\":\"{}\",\"n1\":\"{}\",\"n2\":\"{}\"}}",
        cfg.clamped_count(),
        json_escape(cfg.names[0].as_str()),
        json_escape(cfg.names[1].as_str()),
        json_escape(cfg.names[2].as_str()),
    );
    out
}

async fn status_json() -> heapless::String<384> {
    let status = NET_STATUS.lock().await.clone();
    let stored = STORED_CREDS.lock().await.clone();
    format_status(&status, stored.as_ref())
}

fn format_status(s: &NetStatus, stored: Option<&WifiCreds>) -> heapless::String<384> {
    let ssid = stored.map(|c| c.ssid.as_str()).unwrap_or("");
    let password = stored.map(|c| c.password.as_str()).unwrap_or("");
    let mut out = heapless::String::new();
    let _ = write!(
        out,
        "{{\"mode\":\"{}\",\"ssid\":\"{}\",\"password\":\"{}\",\"ip\":\"{}\",\"error\":\"{}\",\"configured\":{}}}",
        s.mode.as_str(),
        json_escape(ssid),
        json_escape(password),
        json_escape(s.ip.as_str()),
        json_escape(s.last_error.as_str()),
        if s.configured { "true" } else { "false" }
    );
    out
}

fn json_escape(s: &str) -> heapless::String<160> {
    let mut out = heapless::String::new();
    for c in s.chars() {
        let _ = match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        };
    }
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
    write_all(socket, head.as_bytes()).await;
    write_all(socket, body).await;
}

async fn write_all(socket: &mut TcpSocket<'_>, buf: &[u8]) {
    let mut off = 0;
    while off < buf.len() {
        match socket.write(&buf[off..]).await {
            Ok(0) | Err(_) => break,
            Ok(n) => off += n,
        }
    }
}
