#!/usr/bin/env python3
"""Generate Bluey Browser platform icons from Bluey's canonical terminal mark."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from PIL import Image, ImageDraw
except ImportError as error:
    raise SystemExit("Pillow is required to regenerate Bluey Browser icons") from error

BROWSER_ROOT = Path(__file__).resolve().parent.parent
ASSETS = BROWSER_ROOT / "assets"
VECTOR_ICON = ASSETS / "icon-source.svg"
SMALL_VECTOR_ICON = ASSETS / "icon-small-source.svg"
LEGACY_ICONSET = ASSETS / "icon.iconset"
PNG_SIZES = (16, 32, 48, 64, 128, 256, 512, 1024)


def render_vector(path: Path, size: int) -> Image.Image:
    sips = shutil.which("sips")
    if sips:
        with tempfile.TemporaryDirectory(prefix="bluey-browser-icon-") as directory:
            rendered = Path(directory) / "rendered.png"
            subprocess.run(
                [sips, "-s", "format", "png", str(path), "--out", str(rendered)],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            with Image.open(rendered) as image:
                vector_render = image.convert("RGBA").copy()
                if vector_render.size != (size, size):
                    vector_render = vector_render.resize((size, size), Image.Resampling.LANCZOS)
                return vector_render
    try:
        import cairosvg
    except ImportError as error:
        raise SystemExit("sips (macOS) or CairoSVG is required to rasterize the vector icons") from error
    from io import BytesIO
    rendered_bytes = cairosvg.svg2png(
        bytestring=path.read_bytes(),
        output_width=size,
        output_height=size,
    )
    return Image.open(BytesIO(rendered_bytes)).convert("RGBA")


def build_source() -> tuple[Image.Image, Image.Image]:
    # The 1024px Retina master is rasterized directly from SVG. No packaged
    # representation is enlarged from Bluey's older 512px favicon.
    large = render_vector(VECTOR_ICON, 1024)
    compact = render_vector(SMALL_VECTOR_ICON, 1024)
    return large, compact


def resized(source: Image.Image, compact: Image.Image, size: int) -> Image.Image:
    chosen = compact if size <= 48 else source
    return chosen.resize((size, size), Image.Resampling.LANCZOS)


def save_png_family(source: Image.Image, compact: Image.Image) -> None:
    for size in PNG_SIZES:
        icon = resized(source, compact, size)
        icon.save(ASSETS / f"icon-{size}.png", optimize=True)
    source.save(ASSETS / "icon-source.png", optimize=True)
    source.resize((512, 512), Image.Resampling.LANCZOS).save(ASSETS / "icon-512-linux.png", optimize=True)


def save_icns(source: Image.Image, compact: Image.Image) -> None:
    specifications = {
        "icon_16x16.png": 16,
        "icon_16x16@2x.png": 32,
        "icon_32x32.png": 32,
        "icon_32x32@2x.png": 64,
        "icon_128x128.png": 128,
        "icon_128x128@2x.png": 256,
        "icon_256x256.png": 256,
        "icon_256x256@2x.png": 512,
        "icon_512x512.png": 512,
        "icon_512x512@2x.png": 1024,
    }
    with tempfile.TemporaryDirectory(prefix="bluey-browser-iconset-") as directory:
        iconset = Path(directory) / "icon.iconset"
        iconset.mkdir()
        for filename, size in specifications.items():
            resized(source, compact, size).save(iconset / filename, optimize=True)
        iconutil = shutil.which("iconutil")
        if iconutil:
            subprocess.run(
                [iconutil, "--convert", "icns", "--output", str(ASSETS / "icon.icns"), str(iconset)],
                check=True,
            )
        else:
            source.save(ASSETS / "icon.icns", format="ICNS")


def save_windows_icon(source: Image.Image) -> None:
    source.save(
        ASSETS / "icon.ico",
        format="ICO",
        sizes=[(size, size) for size in PNG_SIZES if size <= 256],
    )


def tray_source() -> Image.Image:
    scale = 8
    canvas = Image.new("L", (32 * scale, 32 * scale), 0)
    draw = ImageDraw.Draw(canvas)
    draw.rounded_rectangle((4 * scale, 5 * scale, 28 * scale, 27 * scale), 5 * scale, outline=255, width=2 * scale)
    draw.line((5 * scale, 11 * scale, 27 * scale, 11 * scale), fill=255, width=2 * scale)
    draw.line((10 * scale, 17 * scale, 14 * scale, 21 * scale, 10 * scale, 25 * scale), fill=255, width=2 * scale, joint="curve")
    draw.rounded_rectangle((18 * scale, 23 * scale, 23 * scale, 25 * scale), scale, fill=255)
    return canvas


def save_tray_icons() -> None:
    source = tray_source()
    for size, filename in ((16, "trayTemplate.png"), (32, "trayTemplate@2x.png"), (32, "tray-32.png")):
        alpha = source.resize((size, size), Image.Resampling.LANCZOS)
        image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        image.putalpha(alpha)
        image.save(ASSETS / filename, optimize=True)


def main() -> None:
    if not VECTOR_ICON.is_file() or not SMALL_VECTOR_ICON.is_file():
        raise SystemExit("Missing Bluey Browser vector icon sources")
    ASSETS.mkdir(parents=True, exist_ok=True)
    if LEGACY_ICONSET.exists():
        shutil.rmtree(LEGACY_ICONSET)
    source, compact = build_source()
    save_png_family(source, compact)
    save_icns(source, compact)
    save_windows_icon(source)
    save_tray_icons()


if __name__ == "__main__":
    main()
