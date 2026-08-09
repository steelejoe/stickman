# Stickman Tamagotchi

A Tamagotchi-like stick figure game for the **LilyGo T-Display-S3 AMOLED Touch** (ESP32-S3). Features an animated stick figure with a trait-based behavior plugin system, touch and button input for switching behaviors.

Core logic is shared between the **device firmware** and a **desktop simulation** so you can iterate without hardware.

## Features

- **Animated stick figure** drawn with embedded-graphics
- **Plugin behavior system** - configurable behaviors via traits
- **Walking behavior** - stick figure walks back and forth with screen-edge bounce
- **Idle behavior** - standing still
- **Touch input** - tap screen to cycle behaviors (when CST816 I2C wired)
- **Button input** - GPIO21 to cycle behaviors
- **Desktop simulation** - Linux window with Space/mouse input (`make sim`)

## Hardware

- **Target:** LilyGo T-Display-S3 AMOLED Touch (240x536 or 536x240)
- **MCU:** ESP32-S3 (16MB Flash, 8MB PSRAM)
- **Display:** RM67162 AMOLED via QSPI
- **Touch:** CST816/CST816S (I2C, optional)
- **Buttons:** Boot (GPIO0), GPIO21

## Quick start (simulation)

Develop core logic on Ubuntu without the board:

```bash
# Prerequisites: Rust (stable is fine), X11, libx11-dev
cd /path/to/stickman
make sim
```

### Simulation input mapping

| Desktop input       | Device input                         |
|---------------------|--------------------------------------|
| **Spacebar**        | GPIO21 button (cycle behavior)       |
| **Left mouse click**| Touch tap (cycle behavior)           |
| **Escape** / close  | Quit simulator                       |

The window is 536×240 (landscape) at 2× scale.

## Device build requirements

1. **ESP Rust toolchain** - Install via [espup](https://github.com/esp-rs/espup):

   ```bash
   export PATH="$HOME/.cargo/bin:$PATH"
   cargo install espup
   espup install
   . $HOME/export-esp.sh
   ```

2. **espflash** for flashing:

   ```bash
   cargo install espflash
   ```

## Build commands

```bash
export PATH="$HOME/.cargo/bin:$PATH"

# Desktop simulation (window + Space/mouse)
make sim          # build and run
make build-sim    # build only

# Device firmware (ESP32-S3)
. $HOME/export-esp.sh
make device       # release build
make flash        # build, flash, and monitor
```

Equivalent Cargo invocations:

```bash
# Simulation (host target; +stable avoids the esp toolchain pin)
cargo +stable build --bin stickman-sim --no-default-features --features sim \
  --target x86_64-unknown-linux-gnu
cargo +stable run --bin stickman-sim --no-default-features --features sim \
  --target x86_64-unknown-linux-gnu

# Device (uses rust-toolchain.toml → esp)
. $HOME/export-esp.sh
cargo build --release --bin stickman --features device \
  --target xtensa-esp32s3-none-elf -Z build-std=core,alloc
```

This repository includes `.cargo/config.toml`, so device builds default to:

- target `xtensa-esp32s3-none-elf`
- runner `espflash flash --monitor` (used by `make flash` / `cargo run` for the device binary)
- the checked-in `rust-toolchain.toml` pin to the `esp` toolchain installed by `espup`

The display driver crate is vendored under `vendor/t-display-s3-amoled` so the build does not depend on a floating upstream Git revision.

## Upload Mode

If the USB port keeps disconnecting:

1. Hold **BOOT** button
2. While holding, press and release **RST**
3. Release **BOOT**
4. Run `make flash`
5. Press **RST** to exit download mode

## Project Structure

```
stickman/
├── Makefile             # make sim / make device / make flash
├── src/
│   ├── main.rs          # Device entry, heap init
│   ├── bin/sim.rs       # Desktop simulation entry
│   ├── game.rs          # Shared game loop (device + sim)
│   ├── app.rs           # Device display/input wiring
│   ├── hardware/        # Display, touch, buttons
│   ├── stickman/        # Geometry and rendering
│   └── behavior/        # Plugin behaviors (walking, idle)
```

## Adding New Behaviors

1. Implement the `Behavior` trait in `src/behavior/<name>.rs`
2. Add one line to the `behaviors!` list in `src/behavior/plugin.rs` (cycle order)

## References

- [LilyGo-AMOLED-Series](https://github.com/Xinyuan-LilyGO/LilyGo-AMOLED-Series)
- [t-display-s3-amoled-rs](https://github.com/bh1xuw/t-display-s3-amoled-rs)
- [esp-rs book](https://docs.esp-rs.org/book/)
