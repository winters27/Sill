"""Makes the pictures the README shows, from what `shoot.ps1` captured.

    python scripts/shoot-compose.py backdrop   # the desktop the shoot runs over
    python scripts/shoot-compose.py compose    # docs/media/raw/*.png -> docs/media/

The backdrop is a near-black canvas with two soft, desaturated washes of
colour, so the launcher's glass has something to blur without anything behind
it competing for attention. Composing crops nothing: the shoot already saved
each window with the backdrop around it. It scales the feature pictures to one
width, cuts the logo out of the icon master, and lays out the social preview.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
RAW = ROOT / "docs" / "media" / "raw"
OUT = ROOT / "docs" / "media"
MASTER = ROOT / "src-tauri" / "icons" / "master.png"

# The primary display the shoot runs on.
SCREEN = (2560, 1440)
FEATURE_WIDTH = 1200
HERO_WIDTH = 1400


def backdrop(size=SCREEN) -> Image.Image:
    w, h = size
    canvas = Image.new("RGB", size, (11, 13, 16))

    # Washes are drawn small and scaled up after blurring: blurring a
    # full-size layer with a radius this large is slow for no visible gain.
    scale = 8
    small = Image.new("RGB", (w // scale, h // scale), (11, 13, 16))
    draw = ImageDraw.Draw(small)
    # Cool wash, upper left. Warm wash, lower right. Both far from saturated.
    draw.ellipse(
        [w * -0.15 // scale, h * -0.35 // scale, w * 0.62 // scale, h * 0.55 // scale],
        fill=(44, 58, 74),
    )
    draw.ellipse(
        [w * 0.50 // scale, h * 0.45 // scale, w * 1.25 // scale, h * 1.35 // scale],
        fill=(70, 54, 46),
    )
    small = small.filter(ImageFilter.GaussianBlur(radius=34))
    washes = small.resize(size, Image.LANCZOS)
    canvas = Image.blend(canvas, washes, 1.0)

    # A faint vignette so the edges fall away rather than stop.
    vignette = Image.new("L", (w // scale, h // scale), 0)
    ImageDraw.Draw(vignette).ellipse(
        [w * -0.1 // scale, h * -0.2 // scale, w * 1.1 // scale, h * 1.2 // scale], fill=255
    )
    vignette = vignette.filter(ImageFilter.GaussianBlur(radius=40)).resize(size, Image.LANCZOS)
    dark = Image.new("RGB", size, (6, 7, 9))
    canvas = Image.composite(canvas, dark, vignette)
    return canvas


def logo() -> Image.Image:
    art = Image.open(MASTER).convert("RGBA")
    art = art.crop(art.getbbox())
    side = max(art.size)
    square = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    square.paste(art, ((side - art.width) // 2, (side - art.height) // 2))
    return square.resize((512, 512), Image.LANCZOS)


def font(size: int, weight: str = "semibold") -> ImageFont.FreeTypeFont:
    # Windows ships Segoe UI Variable; rendering an image with it is ordinary
    # use of a system font and nothing is redistributed.
    candidates = {
        "semibold": ["C:/Windows/Fonts/seguisb.ttf", "C:/Windows/Fonts/segoeuib.ttf"],
        "regular": ["C:/Windows/Fonts/segoeui.ttf"],
    }[weight]
    for path in candidates:
        if Path(path).exists():
            return ImageFont.truetype(path, size)
    return ImageFont.load_default(size)


def social(hero: Image.Image, mark: Image.Image) -> Image.Image:
    w, h = 1280, 640
    card = backdrop((w, h))
    draw = ImageDraw.Draw(card)

    # Logo and words on the left third, the launcher on the right.
    mark = mark.resize((132, 132), Image.LANCZOS)
    card.paste(mark, (72, 96), mark)
    draw.text((72, 262), "Sill", font=font(64), fill=(238, 240, 243))
    lines = ["Press one key,", "type what you want,", "and it happens."]
    y = 350
    for line in lines:
        draw.text((72, y), line, font=font(30, "regular"), fill=(186, 192, 200))
        y += 42
    draw.text((72, 512), "An open-source command palette for Windows", font=font(22, "regular"), fill=(140, 148, 158))

    # The hero already carries its own backdrop; fade its left edge into ours.
    shot = hero.convert("RGBA")
    target_h = 560
    shot = shot.resize((int(shot.width * target_h / shot.height), target_h), Image.LANCZOS)
    mask = Image.new("L", shot.size, 255)
    fade = Image.linear_gradient("L").rotate(90, expand=True).resize((160, shot.height))
    mask.paste(fade, (0, 0))
    shot.putalpha(mask)
    card.paste(shot, (w - shot.width + 60, (h - target_h) // 2), shot)
    return card


def compose() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    mark = logo()
    mark.save(OUT / "logo.png", optimize=True)

    hero = None
    for raw in sorted(RAW.glob("*.png")):
        if raw.stem == "backdrop":
            continue
        picture = Image.open(raw).convert("RGB")
        width = HERO_WIDTH if raw.stem == "hero" else FEATURE_WIDTH
        if picture.width > width:
            picture = picture.resize((width, int(picture.height * width / picture.width)), Image.LANCZOS)
        picture.save(OUT / raw.name, optimize=True)
        if raw.stem == "hero":
            hero = Image.open(raw).convert("RGB")
        print(f"{raw.name}: {picture.width}x{picture.height}")

    if hero is not None:
        social(hero, mark).save(OUT / "social-preview.png", optimize=True)
        print("social-preview.png: 1280x640")


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else "compose"
    if what == "backdrop":
        RAW.mkdir(parents=True, exist_ok=True)
        backdrop().save(RAW / "backdrop.png", optimize=True)
        print(RAW / "backdrop.png")
    elif what == "compose":
        compose()
    else:
        sys.exit(f"unknown step {what}; use backdrop or compose")
