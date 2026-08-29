"""Cuts the settings panel icons out of their renders.

    python scripts/make-settings-icons.py <folder> static/settings

Each render is a coloured rounded plaque filling its whole canvas, with only
the corners left black. The corners have to become transparent or every icon
sits in a black-cornered box on the sidebar.

The corner is measured and then redrawn rather than thresholded out. A
threshold sounds simpler and works until you look at the result over a pale
sidebar: the render's corner falls away into its own noise floor, so the cut
lands in the noise and the curve comes out visibly chewed. Measuring gives a
corner as clean at 22px as at 1024.

`NAMES` maps each numbered render to the panel it belongs to. That mapping is
by eye, from what each glyph depicts, and is the one part of this that cannot
be checked by machine.
"""

import sys
from pathlib import Path

import numpy as np
from PIL import Image

# Which render is which panel, read off the glyphs.
NAMES = {
    1: "dictation",    # microphone over a waveform
    2: "clipboard",    # clipboard with a clock
    3: "sources",      # hub wired out to satellites
    4: "appearance",   # a disc half lit, with a sparkle
    5: "general",      # gear
    6: "extensions",   # puzzle piece
    7: "files",        # stacked folders behind a magnifier
    8: "about",        # letter i in a ring
    9: "advanced",     # sliders over circuit traces
}

# Panels that still need art. Named so the gap is visible here rather than
# discovered as a missing icon in the sidebar.
MISSING = ("snippets",)

# One file per size the browser will actually draw, rather than one large file
# it downscales. A single 128px icon shown at 26 CSS px is a 4.9x reduction
# through the browser's cheap filter, and the fine glyph work turns to mush.
#
# These are the **exact** drawn sizes, not round numbers near them: the
# sidebar's 26px and the hero's 38px, each at 1x, 2x and 3x. On a plain
# display that makes it a straight copy with no resampling at all, which is
# the only way a raster icon is as sharp as the vector it replaced.
SIZES = (26, 38, 52, 76, 78, 114)
SOLID = 0.05     # well clear of the render's noise floor


def measure_radius(value: np.ndarray) -> float:
    """The plaque's corner radius, fitted rather than assumed."""
    solid = value > SOLID
    ys, xs = np.nonzero(solid)
    x0, y0, x1, y1 = xs.min(), ys.min(), xs.max(), ys.max()

    insets = []
    for y in range(y0 + 4, y0 + (y1 - y0) // 3):
        row = np.nonzero(solid[y])[0]
        if len(row):
            insets.append((y - y0, row.min() - x0))

    span = min(x1 - x0, y1 - y0)
    best, best_err = span * 0.25, None
    for radius in np.arange(span * 0.05, span * 0.5, 1.0):
        err = 0.0
        for dy, inset in insets:
            predicted = 0.0 if dy >= radius else radius - np.sqrt(max(radius**2 - (radius - dy) ** 2, 0))
            err += (predicted - inset) ** 2
        if best_err is None or err < best_err:
            best, best_err = radius, err
    return float(best)


def rounded_alpha(side: int, radius: float) -> np.ndarray:
    """An antialiased rounded square, as a signed distance field."""
    half = side / 2
    yy, xx = np.mgrid[0:side, 0:side].astype(np.float32) + 0.5
    qx = np.abs(xx - half) - half + radius
    qy = np.abs(yy - half) - half + radius
    outside = np.sqrt(np.maximum(qx, 0) ** 2 + np.maximum(qy, 0) ** 2)
    distance = np.minimum(np.maximum(qx, qy), 0) + outside - radius
    return np.clip(0.5 - distance, 0, 1)


def convert(source: Path, out_dir: Path, name: str) -> None:
    render = Image.open(source).convert("RGB")
    value = np.asarray(render).astype(np.float32).mean(2) / 255
    radius = measure_radius(value)

    for size in SIZES:
        # Downsampled first, then the corner is drawn at the final size.
        # Cutting at full resolution and shrinking would resample the corner's
        # alpha against the black behind it and leave a dark rim.
        small = render.resize((size, size), Image.LANCZOS)
        alpha = rounded_alpha(size, radius * size / render.width)
        out = Image.merge("RGBA", (*small.split(), Image.fromarray((alpha * 255).astype(np.uint8))))
        out.save(out_dir / f"{name}-{size}.png")

    print(f"  {source.name} -> {name}-{{{','.join(str(s) for s in SIZES)}}}.png  radius {radius:.0f}")


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2

    folder, out_dir = Path(sys.argv[1]), Path(sys.argv[2])
    out_dir.mkdir(parents=True, exist_ok=True)

    for number, name in sorted(NAMES.items()):
        source = folder / f"{number}.png"
        if not source.exists():
            print(f"  {source} is missing, skipped")
            continue
        convert(source, out_dir, name)

    for name in MISSING:
        print(f"  no art yet for {name}, it keeps its drawn glyph")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
