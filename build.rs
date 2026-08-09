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
}
