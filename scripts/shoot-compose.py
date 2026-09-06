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
FACE = ROOT / "src" / "lib" / "theme" / "fonts" / "Satoshi-Variable.woff2"

# The primary display the shoot runs on.
SCREEN = (2560, 1440)
FEATURE_WIDTH = 1200
HERO_WIDTH = 1400


# Two grounds, because the two surfaces are doing different jobs.
#
# The screenshots want the launcher to be the only thing with colour in the
# frame, so they sit on a neutral. The social card is the one place the project
# gets to look like itself, so it wears the Oilslick theme's own wash.
#
# Both are written as a base colour plus elliptical gradients carrying their
# colour stops and alphas, which is the shape `theme.css` states Oilslick in.
PALETTES = {
    # No hue anywhere. Black through charcoal, lit from the upper left.
    "graphite": ((9, 9, 10), [
        (1.05, 1.00, 0.22, 0.16, [(0.00, (58, 59, 63), 0.62), (1.00, None, 0.0)]),
        (0.95, 0.90, 0.82, 0.86, [(0.00, (30, 30, 33), 0.55), (1.00, None, 0.0)]),
    ], 1.0),
    # Oilslick, copied from `theme.css`. One hue dominates and the rest stay
    # subordinate, which is what reads as a sheen rather than three blobs.
    # The theme's alphas are set for a 750px window; across a whole canvas they
    # fall below what an 8-bit channel can show, so they are scaled.
    "oilslick": ((0x0B, 0x0A, 0x0E), [
        (0.96, 0.93, 0.28, 0.20, [(0.00, (0, 29, 35), 0.11), (0.42, (37, 24, 0), 0.09), (1.00, None, 0.0)]),
        (1.00, 0.96, 0.76, 0.62, [(0.00, (55, 0, 47), 0.11), (0.38, (0, 30, 20), 0.09),
                                  (0.72, (45, 19, 0), 0.07), (1.00, None, 0.0)]),
        (0.80, 0.70, 0.50, 0.30, [(0.00, (48, 30, 120), 0.165), (0.42, (42, 27, 102), 0.09), (1.00, None, 0.0)]),
    ], 3.0),
}


def _ramp(size, rx, ry, cx, cy) -> Image.Image:
    """Distance from a gradient's centre, 0 at the centre and 255 at its edge.

    `Image.radial_gradient` is a 256 square reaching 255 at the inscribed
    circle. Stretched to the ellipse's bounding box and pasted at its centre,
    it is the normalised elliptical radius a CSS gradient measures its stops
    along. Everything outside stays at 255, past the last stop and therefore
    fully transparent.
    """
    w, h = size
    box = (max(int(2 * rx * w), 1), max(int(2 * ry * h), 1))
    out = Image.new("L", size, 255)
    out.paste(Image.radial_gradient("L").resize(box, Image.BILINEAR),
              (int(cx * w) - box[0] // 2, int(cy * h) - box[1] // 2))
    return out


def _lut(stops, channel, strength):
    """A 256-entry table mapping the ramp to one colour channel, or to alpha."""
    table = []
    for i in range(256):
        t = i / 255
        value = 0.0
        for (o0, c0, a0), (o1, c1, a1) in zip(stops, stops[1:]):
            if not (o0 <= t <= o1):
                continue
            f = (t - o0) / max(o1 - o0, 1e-6)
            if channel == "a":
                value = (a0 * (1 - f) + a1 * f) * strength * 255
            else:
                v0 = c0[channel] if c0 else 0
                v1 = c1[channel] if c1 else v0
                value = v0 * (1 - f) + v1 * f
            break
        table.append(int(max(0, min(255, round(value)))))
    return table


def backdrop(size=SCREEN, palette="graphite") -> Image.Image:
    """The desktop the shoot runs over, and the ground the social card sits on.

    The launcher is transparent, so whatever is behind it is blurred through the
    glass and becomes part of every screenshot. That is why the shoot gets the
    neutral: the only colour in the frame should be the program's.
    """
    base, gradients, strength = PALETTES[palette]
    canvas = Image.new("RGB", size, base)

    for rx, ry, cx, cy, stops in gradients:
        t = _ramp(size, rx, ry, cx, cy)
        colour = Image.merge("RGB", tuple(t.point(_lut(stops, c, strength)) for c in (0, 1, 2)))
        canvas.paste(colour, (0, 0), t.point(_lut(stops, "a", strength)))

    w, h = size
    scale = 8
    vignette = Image.new("L", (w // scale, h // scale), 0)
    ImageDraw.Draw(vignette).ellipse(
        [w * -0.10 // scale, h * -0.20 // scale, w * 1.10 // scale, h * 1.20 // scale], fill=255
    )
    vignette = vignette.filter(ImageFilter.GaussianBlur(radius=40)).resize(size, Image.LANCZOS)
    return Image.composite(canvas, Image.new("RGB", size, (5, 5, 7)), vignette)


def logo() -> Image.Image:
    art = Image.open(MASTER).convert("RGBA")
    art = art.crop(art.getbbox())
    side = max(art.size)
    square = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    square.paste(art, ((side - art.width) // 2, (side - art.height) // 2))
    return square.resize((512, 512), Image.LANCZOS)


def wordmark(size: int) -> ImageFont.FreeTypeFont:
    """The name in the face the launcher itself is set in.

    Satoshi ships as a variable `.woff2`, which Pillow cannot open, so it is
    converted to a TrueType in memory. Nothing is written to disk: the licence
    is careful about the font file being made available, and an image rendered
    with a face does not contain that face.

    The file arrives per machine with `npm run fonts` and is not in the
    repository, so a clone that has not fetched it falls back rather than
    failing. The card is still a card in Segoe UI.
    """
    if not FACE.exists():
        return font(size)

    import io

    try:
        from fontTools.ttLib import TTFont
    except ImportError:
        # The conversion needs fontTools and brotli. Composing a card is not
        # worth a hard dependency, and the fallback is a card that still reads.
        print("  (fontTools is not installed; the name falls back to Segoe UI)")
        return font(size)

    face = TTFont(FACE)
    face.flavor = None
    buf = io.BytesIO()
    face.save(buf)
    buf.seek(0)

    drawn = ImageFont.truetype(buf, size)
    # The axis runs 300 to 900 and defaults to the top of it. A wordmark at 900
    # is a shout; Bold is the weight the interface uses for a heading.
    drawn.set_variation_by_name("Bold")
    return drawn


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


# The two ends of the wash behind the letters "AI".
AI_WARM = (255, 146, 52)
AI_COOL = (233, 78, 190)


def _gradient_text(text: str, font_: ImageFont.FreeTypeFont) -> Image.Image:
    """The text as an image, its letters filled with the warm-to-cool wash."""
    box = font_.getbbox(text)
    w, h = box[2] - box[0], box[3] - box[1]
    mask = Image.new("L", (w, h), 0)
    ImageDraw.Draw(mask).text((-box[0], -box[1]), text, font=font_, fill=255)

    wash = Image.new("RGB", (w, h))
    px = wash.load()
    for x in range(w):
        f = x / max(w - 1, 1)
        px[x, 0] = tuple(int(AI_WARM[i] * (1 - f) + AI_COOL[i] * f) for i in range(3))
    wash.paste(wash.crop((0, 0, w, 1)).resize((w, h), Image.NEAREST), (0, 0))

    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    out.paste(wash, (0, 0), mask)
    return out


def social(mark: Image.Image) -> Image.Image:
    """The card link previews show.

    No screenshot: a launcher window shrunk to fit beside the words is
    illegible at the size these are actually seen at.

    Laid out by measuring rather than by hardcoding a first line, so the gaps
    are optical and the block is centred on what it measures.
    """
    w, h = 1280, 640
    card = backdrop((w, h), "oilslick")
    draw = ImageDraw.Draw(card)

    headline = ("Your Windows toolbox, summoned with a keystroke.", font(34, "regular"), (222, 226, 232))
    chip_font = font(21, "regular")

    name = wordmark(76)
    hbox = draw.textbbox((0, 0), headline[0], font=headline[1])
    nbox = draw.textbbox((0, 0), "Sill", font=name)
    side = 122
    block = (side + 36 + (nbox[3] - nbox[1]) + 26
             + (hbox[3] - hbox[1]) + 48 + 44)

    y = (h - block) // 2
    small = mark.resize((side, side), Image.LANCZOS)
    card.paste(small, ((w - side) // 2, y), small)
    y += side + 36

    draw.text(((w - (nbox[2] - nbox[0])) / 2 - nbox[0], y - nbox[1] - 4), "Sill", font=name, fill=(240, 242, 245))
    y += (nbox[3] - nbox[1]) + 26

    draw.text(((w - (hbox[2] - hbox[0])) / 2 - hbox[0], y - hbox[1]), headline[0], font=headline[1], fill=headline[2])
    y += (hbox[3] - hbox[1]) + 48

    # The row underneath: three claims of two words each, divided by hairlines.
    # No fill and no border, and no ornament: the wash on one word is the whole
    # decoration, and it stays legible by being the only one.
    row_font = font(22, "regular")
    ai_font = font(24)
    dim = (150, 156, 167)

    lead = "Local "
    lead_w = int(draw.textlength(lead, font=row_font))
    ai_w = _gradient_text("AI", ai_font).width
    items = [
        ("Built in Rust", int(draw.textlength("Built in Rust", font=row_font))),
        ("AI", lead_w + ai_w),
        ("Open source", int(draw.textlength("Open source", font=row_font))),
    ]

    rule = 34
    x = (w - (sum(width for _, width in items) + rule * (len(items) - 1))) // 2
    mid = y + 22
    box = row_font.getbbox("Hg")
    baseline = mid - (box[3] - box[1]) // 2 - box[1]

    for i, (kind, width) in enumerate(items):
        if kind == "AI":
            draw.text((x, baseline), lead, font=row_font, fill=dim)
            # `_gradient_text` returns the ink alone, so it sits on the row's
            # baseline by being offset the way the glyphs would have been.
            letters = _gradient_text("AI", ai_font)
            card.paste(letters, (x + lead_w, baseline + ai_font.getbbox("AI")[1]), letters)
        else:
            draw.text((x, baseline), kind, font=row_font, fill=dim)
        x += width
        if i < len(items) - 1:
            # The same 1px rule the launcher separates things with.
            draw.line([(x + rule // 2, mid - 7), (x + rule // 2, mid + 7)], fill=(68, 66, 78), width=1)
            x += rule

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
