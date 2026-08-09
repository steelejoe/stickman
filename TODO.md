# TODO

Here are some thoughts on the project about things to come.

## Images

To make importing images easier, I will allow for SVG images to be imported and converted to a bitmap.
I will also support PNG, JPEG, and GIF images however all will be converted to bitmap.
I also need a way to allow for 2.5D image support. I want to support 3 "depth" layers.
The goal is to allow for a background image, a middleground image, and a foreground image.
And the figure can be positioned in the middleground image.
**Q**: is this enough layers to support the project?
**Q**: how can I represent this in the bitmaps?

Done so far:
- `make import` / `scripts/import-image.py` converts local files → `assets/<name>.png` (sim) + `.rgb565` (device)
- Formats: PNG, JPEG, GIF (first frame), WebP, SVG
- Three depth layers (`LayerId`: background / middle / foreground); stickman draws on middle or foreground
- Layer-0 background image embed + runtime sim load; dirty-rect restore under the figure

TODOs:
- [x] Build a simulator to show in a window on my desktop (`make sim`)
- [x] Add import-image tooling for local files (`make import IMAGE=...`)
- [x] Add PNG support
- [x] Add JPEG support
- [x] Add GIF support
- [x] Add SVG support
- [x] Add 3 depth layers (background / middle / foreground)
- [x] Background image on layer 0 (import + draw / dirty restore)
- [ ] Middleground / foreground *images* (not just stickman on those layers)
- [ ] Add support for importing images from the web / URL
- [ ] Add support for importing images from a base64 encoded string
- [ ] Richer CLI than `make import` (optional subcommands, URL/base64 inputs)

## Behaviors

I want to allow for a behavior to be attached to the figure.
The behavior will be able to control the figure's position, rotation, scale, and other properties.
The behavior will also be able to control the figure's animations.
I am not sure how I want to handle behavior attachment. Maybe they can be generic and work with any image?
Probably not though.

Done so far:
- Trait-based plugin system (`Behavior` + `behaviors!` enum dispatch in `plugin.rs`)
- Cycle order: walk → idle → jump → crouch → begging → knockback → tumble
  - **Walking** — gait + edge bounce
  - **Idle** — standing still (static frame skip)
  - **Jumping** — parabolic hop; head just above screen mid
  - **Crouching** — bent knees, torso lean ~30°, arms by sides
  - **Begging** — bent knees, arms reaching forward
  - **Knockback** — front-facing spin, travel opposite facing
  - **Tumbling** — side-profile crouch-ball roll (clockwise / CCW by facing)
- Touch (CST816) + BOOT button / sim Space+click cycle behaviors
- Shared `Game` loop with dirty-tile presents (device flicker-free)

TODOs:
- [x] Add behavior system to the infrastructure
- [x] Walking / idle / jump / crouch / begging / knockback / tumble behaviors
- [ ] Behaviors that control scale (and richer rotation beyond roll modes)
- [ ] Optional: attach behaviors to non-stickman images / sprites
- [ ] Behavior self-transitions / timed sequences (beyond tap-to-cycle)

## Tooling and hardware cleanup

Carried over from the completed build-rescue work. Device build/flash is working; these are follow-ups.

TODOs:
- [ ] Add `scripts/check-env.sh` to verify `cargo`, `rustup`, `espflash`, and the ESP toolchain resolve
- [ ] Add CI for device (`make device`) and simulation (`make build-sim`) builds
- [ ] Move display init out of `src/app.rs` into `src/hardware/display.rs` (currently a stub)
