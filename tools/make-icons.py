#!/usr/bin/env python3
"""Regenerate every ReCast icon from assets/recast-icon.jpg.

Requires Pillow:  python -m pip install pillow

Run from anywhere:
    python tools/make-icons.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageOps

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets" / "recast-icon.jpg"

ICO_SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
ANDROID_DENSITIES = {
    "mdpi": 1.0,
    "hdpi": 1.5,
    "xhdpi": 2.0,
    "xxhdpi": 3.0,
    "xxxhdpi": 4.0,
}


def write_raw_rgba(image: Image.Image, path: Path) -> None:
    """egui loads the window icon as raw RGBA with no image decoder."""
    with path.open("wb") as handle:
        handle.write(image.convert("RGBA").tobytes())


def transparent_logo(source: Image.Image) -> Image.Image:
    """White mark with the black background turned into transparency."""
    gray = ImageOps.grayscale(source)
    alpha = gray.point(lambda value: 255 if value > 30 else 0)
    logo = Image.new("RGBA", source.size, (255, 255, 255, 0))
    logo.putalpha(alpha)
    return logo.crop(logo.getbbox())


def fit_logo(logo: Image.Image, canvas: int, fraction: float) -> Image.Image:
    side = int(canvas * fraction)
    scaled = logo.copy()
    scaled.thumbnail((side, side), Image.LANCZOS)
    layer = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    layer.alpha_composite(scaled, ((canvas - scaled.width) // 2, (canvas - scaled.height) // 2))
    return layer


def main() -> None:
    source = Image.open(SOURCE).convert("RGB")

    launcher = ROOT / "apps" / "launcher" / "assets"
    launcher.mkdir(parents=True, exist_ok=True)
    source.save(launcher / "recast.ico", sizes=ICO_SIZES)
    source.resize((512, 512), Image.LANCZOS).save(launcher / "recast.png")

    app = ROOT / "apps" / "app" / "assets"
    app.mkdir(parents=True, exist_ok=True)
    source.resize((256, 256), Image.LANCZOS).save(app / "icon.png")
    write_raw_rgba(source.resize((256, 256), Image.LANCZOS), app / "icon.rgba")
    (app / "recast.ico").write_bytes((launcher / "recast.ico").read_bytes())

    logo = transparent_logo(source)
    res = ROOT / "android" / "app" / "src" / "main" / "res"
    for name, scale in ANDROID_DENSITIES.items():
        folder = res / f"mipmap-{name}"
        folder.mkdir(parents=True, exist_ok=True)
        size = int(48 * scale)

        icon = source.resize((size, size), Image.LANCZOS).convert("RGBA")
        icon.save(folder / "ic_launcher.png")

        mask = Image.new("L", (size * 4, size * 4), 0)
        ImageDraw.Draw(mask).ellipse((0, 0, size * 4 - 1, size * 4 - 1), fill=255)
        mask = mask.resize((size, size), Image.LANCZOS)
        round_icon = icon.copy()
        round_icon.putalpha(mask)
        round_icon.save(folder / "ic_launcher_round.png")

        foreground = int(108 * scale)
        fit_logo(logo, foreground, 0.58).save(folder / "ic_launcher_foreground.png")

    print("ReCast icons regenerated from", SOURCE)


if __name__ == "__main__":
    main()
