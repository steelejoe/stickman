fn main() {
    // ESP linker script is only needed for the device firmware binary.
    if std::env::var("CARGO_FEATURE_DEVICE").is_ok() {
        println!("cargo:rustc-link-arg-bins=-Tlinkall.x");
    }
}
