#!/usr/bin/env python3
"""Import an image and emit simulator PNG + device RGB565 assets.

Supported inputs: PNG, JPEG, GIF (first frame), WebP, SVG.
By default the image is scaled to fit inside the room (480×240 — the
display minus the 56px menu strip) while preserving aspect ratio, then
centered on that canvas. Transparent padding is trimmed so firmware does
not store or blit unused pixels. The SM65 origin records where the crop
sits in display space (room starts at x=56).
With --stretch-x, height is fit to the room and width is stretched to
fill. --full-display uses the whole 536×240 panel instead of the room.
--no-trim keeps the letterboxed canvas.

Outputs (under --out-dir):
  <name>.png     — RGBA, trimmed, for preview
  <name>.rgb565  — packed bitmap for the device (see header below)

RGB565 file layout (little-endian header, display-native pixel bytes):
  offset 0  magic    b'SM65'
  offset 4  width    u16 LE
  offset 6  height   u16 LE
  offset 8  origin_x i16 LE  (display-space top-left)
  offset 10 origin_y i16 LE
  offset 12 pixels   width*height big-endian RGB565 samples
                     (matches RM67162 / Rgb565::to_be_bytes)
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

from PIL import Image

DISPLAY_WIDTH = 536
DISPLAY_HEIGHT = 240
# Must match src/menu.rs MENU_WIDTH / ROOM_LEFT.
ROOM_LEFT = 56
ROOM_WIDTH = DISPLAY_WIDTH - ROOM_LEFT
MAGIC = b"SM65"

RASTER_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"}
SVG_SUFFIXES = {".svg"}


def load_svg(path: Path) -> Image.Image:
    """Rasterize SVG via GdkPixbuf (librsvg)."""
    try:
        import gi

        gi.require_version("GdkPixbuf", "2.0")
        from gi.repository import GdkPixbuf
    except Exception as exc:  # pragma: no cover - env-specific
        raise SystemExit(
            "SVG import requires PyGObject + GdkPixbuf (librsvg).\n"
            f"Failed to import: {exc}"
        ) from exc

    pixbuf = GdkPixbuf.Pixbuf.new_from_file(str(path))
    width = pixbuf.get_width()
    height = pixbuf.get_height()
    channels = pixbuf.get_n_channels()
    mode = "RGBA" if channels == 4 else "RGB"
    image = Image.frombytes(
        mode,
        (width, height),
        bytes(pixbuf.get_pixels()),
        "raw",
        mode,
        pixbuf.get_rowstride(),
    )
    return image.convert("RGBA")


def load_image(path: Path) -> Image.Image:
    suffix = path.suffix.lower()
    if suffix in SVG_SUFFIXES:
        return load_svg(path)
    if suffix not in RASTER_SUFFIXES:
        raise SystemExit(
            f"Unsupported format '{suffix}'. "
            f"Use one of: {', '.join(sorted(RASTER_SUFFIXES | SVG_SUFFIXES))}"
        )

    with Image.open(path) as img:
        # Animated GIF/WebP: first frame only.
        img.seek(0)
        return img.convert("RGBA")


def fit_to_display(
    image: Image.Image,
    width: int,
    height: int,
    *,
    stretch_x: bool = False,
) -> Image.Image:
    """Scale image onto a width×height canvas.

    Default: uniform scale to fit inside, centered (letter/pillar boxed).
    stretch_x: fit height exactly, then stretch/squash width to fill.
    """
    if stretch_x:
        # Match display height, then force full width (horizontal only distortion).
        scaled = image.resize(
            (max(1, round(image.width * height / image.height)), height),
            Image.Resampling.LANCZOS,
        )
        return scaled.resize((width, height), Image.Resampling.LANCZOS)

    fitted = image.copy()
    fitted.thumbnail((width, height), Image.Resampling.LANCZOS)
    canvas = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    x = (width - fitted.width) // 2
    y = (height - fitted.height) // 2
    canvas.paste(fitted, (x, y), fitted)
    return canvas


def rgb888_to_rgb565_be(r: int, g: int, b: int) -> bytes:
    value = ((r & 0xF8) << 8) | ((g & 0xFC) << 3) | (b >> 3)
    return struct.pack(">H", value)


def trim_alpha(image: Image.Image) -> tuple[Image.Image, int, int]:
    """Crop to the opaque bounding box. Returns (cropped, dx, dy)."""
    rgba = image.convert("RGBA")
    bbox = rgba.split()[3].getbbox()
    if bbox is None:
        return rgba.crop((0, 0, 1, 1)), 0, 0
    return rgba.crop(bbox), bbox[0], bbox[1]


def write_rgb565(
    path: Path, image: Image.Image, origin: tuple[int, int]
) -> None:
    """Write SM65 header + origin + big-endian RGB565 pixels (transparent → black)."""
    rgb = Image.new("RGB", image.size, (0, 0, 0))
    rgb.paste(image, mask=image.split()[3])
    width, height = rgb.size
    pixels = rgb.tobytes()  # RGBRGB...

    out = bytearray()
    out += MAGIC
    out += struct.pack("<HH", width, height)
    out += struct.pack("<hh", origin[0], origin[1])
    for i in range(0, len(pixels), 3):
        r, g, b = pixels[i], pixels[i + 1], pixels[i + 2]
        out += rgb888_to_rgb565_be(r, g, b)

    path.write_bytes(out)


def default_name(path: Path) -> str:
    return path.stem


def parse_args(argv: list[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("image", type=Path, help="Source image path")
    p.add_argument(
        "--name",
        help="Output basename (default: source stem)",
    )
    p.add_argument(
        "--out-dir",
        type=Path,
        default=Path("assets"),
        help="Directory for .png and .rgb565 (default: assets)",
    )
    p.add_argument(
        "--width",
        type=int,
        default=ROOM_WIDTH,
        help=f"Canvas width (default: room {ROOM_WIDTH})",
    )
    p.add_argument(
        "--height",
        type=int,
        default=DISPLAY_HEIGHT,
        help=f"Canvas height (default: {DISPLAY_HEIGHT})",
    )
    p.add_argument(
        "--origin-x",
        type=int,
        default=ROOM_LEFT,
        help=f"Display-space X of the canvas (default: room {ROOM_LEFT})",
    )
    p.add_argument(
        "--origin-y",
        type=int,
        default=0,
        help="Display-space Y of the canvas (default: 0)",
    )
    p.add_argument(
        "--full-display",
        action="store_true",
        help="Fit to the full 536×240 panel at origin (0, 0) instead of the room",
    )
    p.add_argument(
        "--stretch-x",
        action="store_true",
        help="Fit height to the canvas, then stretch width to fill",
    )
    p.add_argument(
        "--no-trim",
        action="store_true",
        help="Keep letterbox padding instead of cropping transparent pixels",
    )
    return p.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    src = args.image.expanduser().resolve()
    if not src.is_file():
        print(f"error: file not found: {src}", file=sys.stderr)
        return 1

    name = args.name or default_name(src)
    out_dir = args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)

    width = DISPLAY_WIDTH if args.full_display else args.width
    height = args.height
    origin_x = 0 if args.full_display else args.origin_x
    origin_y = 0 if args.full_display else args.origin_y

    image = load_image(src)
    fitted = fit_to_display(image, width, height, stretch_x=args.stretch_x)
    if args.no_trim:
        out_image = fitted
        ox, oy = origin_x, origin_y
    else:
        out_image, dx, dy = trim_alpha(fitted)
        ox, oy = origin_x + dx, origin_y + dy

    png_path = out_dir / f"{name}.png"
    rgb_path = out_dir / f"{name}.rgb565"

    out_image.save(png_path, format="PNG")
    write_rgb565(rgb_path, out_image, (ox, oy))

    mode = "stretch-x" if args.stretch_x else "aspect preserved"
    canvas = f"{width}x{height}"
    print(
        f"imported {src.name} → {out_image.width}x{out_image.height} "
        f"@ ({ox},{oy}) on {canvas} ({mode})"
    )
    print(f"  sim:    {png_path}")
    print(f"  device: {rgb_path} ({rgb_path.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
