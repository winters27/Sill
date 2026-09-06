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


def social(mark: Image.Image) -> Image.Image:
    """The card link previews show: the logo, the name, and the sentence.

    No screenshot. A launcher window shrunk to fit beside the words is
    illegible at the size these are actually seen at, and it makes the card
    about a picture nobody can read rather than about what the thing does.

    Laid out by measuring rather than by hardcoding a first line: the gaps are
    optical, so they are set between the drawn edges of the type, and the whole
    block is centred on what it actually measures.
    """
    w, h = 1280, 640
    card = backdrop((w, h))
    draw = ImageDraw.Draw(card)

    side = 128
    lines = [
        ("Sill", font(76), (238, 240, 243), 42),
        ("Press one key, type what you want, and it happens.", font(32, "regular"), (186, 192, 200), 24),
        ("An open-source command palette for Windows", font(22, "regular"), (128, 136, 146), 0),
    ]

    # Ink heights, so a line with no descender does not leave a bigger gap
    # under it than one that has.
    measured = [(t, f, c, gap, draw.textbbox((0, 0), t, font=f)) for t, f, c, gap in lines]
    text_block = sum((box[3] - box[1]) + gap for _, _, _, gap, box in measured)
    total = side + 34 + text_block

    y = (h - total) // 2
    small = mark.resize((side, side), Image.LANCZOS)
    card.paste(small, ((w - side) // 2, y), small)
    y += side + 34

    for text, font_, fill, gap, box in measured:
        draw.text(((w - (box[2] - box[0])) / 2 - box[0], y - box[1]), text, font=font_, fill=fill)
        y += (box[3] - box[1]) + gap

    return card


def compose() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    mark = logo()
    mark.save(OUT / "logo.png", optimize=True)

    for raw in sorted(RAW.glob("*.png")):
        if raw.stem == "backdrop":
            continue
        picture = Image.open(raw).convert("RGB")
        width = HERO_WIDTH if raw.stem == "hero" else FEATURE_WIDTH
        if picture.width > width:
            picture = picture.resize((width, int(picture.height * width / picture.width)), Image.LANCZOS)
        picture.save(OUT / raw.name, optimize=True)
        print(f"{raw.name}: {picture.width}x{picture.height}")

    social(mark).save(OUT / "social-preview.png", optimize=True)
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
