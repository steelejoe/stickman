#!/usr/bin/env python3
"""Import an image and emit simulator PNG + device RGB565 assets.

Supported inputs: PNG, JPEG, GIF (first frame), WebP, SVG.
By default the image is scaled to fit inside the display (536×240) while
preserving aspect ratio, then centered on a full-size canvas.
With --stretch-x, height is fit to the display and width is stretched to
fill (useful for wide backgrounds).

Outputs (under --out-dir):
  <name>.png     — RGBA, for the desktop simulator
  <name>.rgb565  — packed bitmap for the device (see header below)

RGB565 file layout (little-endian header, display-native pixel bytes):
  offset 0  magic   b'SM65'
  offset 4  width   u16 LE
  offset 6  height  u16 LE
  offset 8  pixels  width*height big-endian RGB565 samples
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


def write_rgb565(path: Path, image: Image.Image) -> None:
    """Write SM65 header + big-endian RGB565 pixels (transparent → black)."""
    rgb = Image.new("RGB", image.size, (0, 0, 0))
    rgb.paste(image, mask=image.split()[3])
    width, height = rgb.size
    pixels = rgb.tobytes()  # RGBRGB...

    out = bytearray()
    out += MAGIC
    out += struct.pack("<HH", width, height)
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
        default=DISPLAY_WIDTH,
        help=f"Display width (default: {DISPLAY_WIDTH})",
    )
    p.add_argument(
        "--height",
        type=int,
        default=DISPLAY_HEIGHT,
        help=f"Display height (default: {DISPLAY_HEIGHT})",
    )
    p.add_argument(
        "--stretch-x",
        action="store_true",
        help="Fit height to the display, then stretch width to fill",
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

    image = load_image(src)
    fitted = fit_to_display(
        image, args.width, args.height, stretch_x=args.stretch_x
    )

    png_path = out_dir / f"{name}.png"
    rgb_path = out_dir / f"{name}.rgb565"

    fitted.save(png_path, format="PNG")
    write_rgb565(rgb_path, fitted)

    mode = "stretch-x" if args.stretch_x else "aspect preserved"
    print(f"imported {src.name} → {args.width}x{args.height} ({mode})")
    print(f"  sim:    {png_path}")
    print(f"  device: {rgb_path} ({rgb_path.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
