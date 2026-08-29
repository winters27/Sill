"""Cuts the app icon out of its render and writes the master PNG.

Run this, then `npx tauri icon` to produce the whole set:

    python scripts/make-icons.py <render.png> src-tauri/icons/master.png
    npx tauri icon src-tauri/icons/master.png

The art is a window frame, a sill and the S. Two different blacks have to be
transparent: the surround, or every taskbar draws a square around the icon,
and the four window panes, or it is a black slab with a letter on it instead
of a window you can see through.

There are two ways to get there and the script takes whichever the render
offers. If it arrives with a real alpha channel, that is used, because a
channel authored against the art beats anything inferred from pixels. If it
arrives opaque, the fallback threshold works only because such renders have
been true cutouts, where the surround and the panes are both exactly 0 and
the frame averages 0.18 even where it looks black. That is a property of the
file rather than a law, so it is checked instead of assumed.
"""

import sys

import numpy as np
from PIL import Image

# Below this the pixel is background; above it, art. Only used when the
# render carries no alpha of its own.
DARK = 0.0015
LIT = 0.010

# The surround plus four panes. Well under this and the render is not a
# cutout, so thresholding it would eat the frame.
EXPECTED_EMPTY = 0.35

# An alpha channel this sparse is a leftover, not a cutout.
REAL_ALPHA = 0.02

# Background removal tools often land a hair short of opaque. Anything at or
# above this was meant to be solid.
NEARLY_SOLID = 250


def from_alpha(a: np.ndarray) -> np.ndarray:
    """The render's own alpha, stretched so its solid parts are truly solid.

    Measured as the median of the near-solid pixels, not their maximum. A
    background remover can leave the whole body a step short of opaque while
    a handful of stray pixels still reach 255, and taking the maximum reads
    those strays as proof the icon is already solid.
    """
    solid = a[a >= NEARLY_SOLID]
    intended = float(np.median(solid)) if solid.size else 255.0
    if intended < 255:
        print(f"body alpha is {intended:.0f}, stretching to 255")
    return np.clip(a * (255.0 / intended), 0, 255) / 255.0


def from_brightness(value: np.ndarray) -> np.ndarray:
    empty = float((value <= DARK).mean())
    if empty < EXPECTED_EMPTY:
        raise SystemExit(
            f"this render has no alpha and only {empty:.1%} of it is empty, "
            f"against {EXPECTED_EMPTY:.0%} expected. It looks composited on "
            f"black rather than cut out, so thresholding would eat the frame. "
            f"Re-export it with transparency."
        )
    print(f"no alpha; cutting on brightness, {empty:.1%} empty")
    alpha = np.clip((value - DARK) / (LIT - DARK), 0, 1)
    # Smoothstep, so the edge ramps rather than banding.
    return alpha * alpha * (3 - 2 * alpha)


def cut(render: Image.Image) -> Image.Image:
    rgba = render.convert("RGBA")
    a = np.asarray(rgba).astype(np.float32)
    rgb = Image.merge("RGB", rgba.split()[:3])

    carries_alpha = float((a[..., 3] == 0).mean()) > REAL_ALPHA
    if carries_alpha:
        print("using the render's own alpha")
        alpha = from_alpha(a[..., 3])
    else:
        alpha = from_brightness(a[..., :3].max(2) / 255)

    out = Image.merge("RGBA", (*rgb.split(), Image.fromarray((alpha * 255).astype(np.uint8))))

    # Trim to the art, then square it around its centre. An icon carrying its
    # own padding is drawn smaller than everything beside it, and a master a
    # few pixels off square gets stretched by every downstream resize.
    ys, xs = np.nonzero(alpha > 0.5)
    side = max(xs.max() - xs.min(), ys.max() - ys.min()) + 1
    cx, cy = (xs.min() + xs.max()) / 2, (ys.min() + ys.max()) / 2
    left, top = round(cx - side / 2), round(cy - side / 2)
    print(f"art {xs.max()-xs.min()+1}x{ys.max()-ys.min()+1}, squared to {side}")

    square = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    square.paste(out.crop((left, top, left + side, top + side)), (0, 0))
    return square


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2

    icon = cut(Image.open(sys.argv[1]))
    icon.save(sys.argv[2])
    print(f"{sys.argv[2]}: {icon.width}x{icon.height}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
