use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // ESP linker script is only needed for the device firmware binary.
    if std::env::var("CARGO_FEATURE_DEVICE").is_ok() {
        println!("cargo:rustc-link-arg-bins=-Tlinkall.x");
    }

    // Embed assets/background.rgb565 when present (make import NAME=background).
    let background = std::path::Path::new("assets/background.rgb565");
    println!("cargo:rerun-if-changed=assets/background.rgb565");
    println!("cargo:rustc-check-cfg=cfg(has_background)");
    if background.is_file() {
        println!("cargo:rustc-cfg=has_background");
    }

    // Stamp this firmware image so device flash can drop stale saved config.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
    write_build_id();
}

fn write_build_id() {
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let path = std::path::Path::new(&out).join("stickman_build_id.bin");
    let mut id = [0u8; 16];
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    id[..8].copy_from_slice(&ts.to_le_bytes());
    let git = git_short();
    let n = git.len().min(8);
    id[8..8 + n].copy_from_slice(&git.as_bytes()[..n]);
    std::fs::write(path, id).expect("write stickman_build_id.bin");
}

fn git_short() -> String {
    Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "nogit".into())
}
