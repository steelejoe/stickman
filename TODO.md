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

TODOs:
  [x] Build a simulator to show in a window on my desktop (`make sim`)
- [ ] Add "import-image" command to the CLI to import an image from a file, URL, or base64 encoded string.
- [ ] Add PNG support
- [ ] Add JPEG support
- [ ] Add GIF support
- [ ] Add support for importing images from the web
- [ ] Add support for importing images from a file
- [ ] Add support for importing images from a URL
- [ ] Add support for importing images from a base64 encoded string

## Behaviors

I want to allow for a behavior to be attached to the figure.
The behavior will be able to control the figure's position, rotation, scale, and other properties.
The behavior will also be able to control the figure's animations.
I am not sure how I want to handle behavior attachment. Maybe they can be generic and work with any image?
Probably not though.

TODOs:
- [ ] Add behavior system to the infrastructure

## Tooling and hardware cleanup

Carried over from the completed build-rescue work. Device build/flash is working; these are follow-ups.

TODOs:
- [ ] Add `scripts/check-env.sh` to verify `cargo`, `rustup`, `espflash`, and the ESP toolchain resolve
- [ ] Add CI for device (`make device`) and simulation (`make build-sim`) builds
- [ ] Move display init out of `src/app.rs` into `src/hardware/display.rs` (currently a stub)
