/**
 * The shapes a marked-up picture is made of.
 *
 * Kept as a list rather than painted straight onto the picture. Undo is then
 * dropping the last one, changing a colour is editing one, and the picture
 * underneath is never touched until it is exported. Painting each stroke into
 * the image as it is drawn would make undo impossible without keeping a copy
 * of every intermediate picture, which for a full-screen capture is tens of
 * megabytes per stroke.
 */

export type Tool =
  | "arrow"
  | "box"
  | "ellipse"
  | "pen"
  | "highlight"
  | "hide"
  | "text"
  /** A numbered badge, for walking somebody through a picture in order. */
  | "step"
  /** Trims the picture, rather than drawing on it. */
  | "crop";

export interface Point {
  x: number;
  y: number;
}

export interface Shape {
  tool: Tool;
  colour: string;
  /** Stroke width, in the picture's own pixels. */
  weight: number;
  /** Two points for the shapes that have corners, many for a pen stroke. */
  points: Point[];
  /** Only for `text`. */
  text?: string;
  /**
   * Only for `step`: which number the badge shows.
   *
   * Carried rather than worked out from the shape's position in the list,
   * because the list holds every kind of mark and a badge's number has to
   * survive a box being drawn between two of them.
   */
  number?: number;
}

/** How coarse the blocks are when hiding something, relative to the stroke. */
export const HIDE_BLOCK = 6;

/**
 * The rectangle two points describe, whichever way round they are.
 *
 * Dragging up and to the left is as ordinary as dragging down and to the
 * right, and every shape here has to survive it.
 */
export function boxOf(a: Point, b: Point): { x: number; y: number; w: number; h: number } {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    w: Math.abs(b.x - a.x),
    h: Math.abs(b.y - a.y),
  };
}

/**
 * Where an arrow's head sits, as the two points behind its tip.
 *
 * Worked out here rather than in the drawing so it can be tested: an arrow
 * whose head does not turn with the line is the classic version of this bug,
 * and it only shows at certain angles.
 */
export function arrowHead(from: Point, to: Point, size: number): [Point, Point] {
  const angle = Math.atan2(to.y - from.y, to.x - from.x);
  // Wide enough to read as an arrow at a glance, narrow enough not to look
  // like a delta.
  const spread = Math.PI / 7;

  return [
    {
      x: to.x - size * Math.cos(angle - spread),
      y: to.y - size * Math.sin(angle - spread),
    },
    {
      x: to.x - size * Math.cos(angle + spread),
      y: to.y - size * Math.sin(angle + spread),
    },
  ];
}

/**
 * Whether a shape has anything to draw.
 *
 * A click with no drag makes a shape of no size, and leaving those in the list
 * means undo appears to do nothing: it removes something invisible.
 */
export function worthKeeping(shape: Shape): boolean {
  if (shape.tool === "text") return (shape.text ?? "").trim().length > 0;
  if (shape.tool === "pen") return shape.points.length > 1;

  const [from, to] = shape.points;
  if (!from || !to) return false;

  const box = boxOf(from, to);
  // A couple of pixels either way is a click, not a shape.
  return box.w > 2 || box.h > 2;
}

/** The colours offered, which are the ones that show up on a screenshot. */
export const COLOURS = [
  { name: "Red", value: "#ff3b30" },
  { name: "Yellow", value: "#ffcc00" },
  { name: "Green", value: "#34c759" },
  { name: "Blue", value: "#0a84ff" },
  { name: "Black", value: "#000000" },
  { name: "White", value: "#ffffff" },
];

/**
 * The size a picture should be shown at inside a given space.
 *
 * Fits it to the space and keeps its shape, scaling **up** as well as down. A
 * `max-width` alone only ever shrinks, which leaves a small capture sitting
 * tiny in the middle of a large window with nothing wrong that anyone can
 * point at.
 *
 * Never larger than the picture's own pixels: enlarging a screenshot past
 * one-to-one only makes it blurry, and it stops the marks lining up with what
 * they are marking.
 */
export function fitted(
  picture: { width: number; height: number },
  space: { width: number; height: number },
): { width: number; height: number } {
  if (picture.width <= 0 || picture.height <= 0) return { width: 0, height: 0 };
  if (space.width <= 0 || space.height <= 0) return { width: 0, height: 0 };

  const scale = Math.min(space.width / picture.width, space.height / picture.height, 1);

  return {
    width: Math.round(picture.width * scale),
    height: Math.round(picture.height * scale),
  };
}

/**
 * Whether a point is on a shape, for picking one up again.
 *
 * Generous on purpose: `slack` is how far away still counts, because a one
 * pixel line is not something anybody can click on reliably. It is the stroke
 * width plus a margin, so a thick line is easier to hit than a thin one, which
 * is what somebody would expect.
 */
export function touches(shape: Shape, point: Point, slack: number): boolean {
  const reach = Math.max(slack, shape.weight * 2);

  if (shape.tool === "pen") {
    return shape.points.some((at) => Math.hypot(at.x - point.x, at.y - point.y) <= reach);
  }

  if (shape.tool === "text") {
    const size = Math.max(12, shape.weight * 6);
    const wide = (shape.text ?? "").length * size * 0.6;

    return (
      point.x >= shape.points[0].x - reach &&
      point.x <= shape.points[0].x + wide + reach &&
      point.y >= shape.points[0].y - reach &&
      point.y <= shape.points[0].y + size + reach
    );
  }

  const [from, to] = shape.points;
  if (!from || !to) return false;

  const box = boxOf(from, to);

  // Filled shapes are hit anywhere inside; outlines only near their edge would
  // be truer, but "click the thing you can see" is what people expect and a
  // box is mostly its own inside.
  return (
    point.x >= box.x - reach &&
    point.x <= box.x + box.w + reach &&
    point.y >= box.y - reach &&
    point.y <= box.y + box.h + reach
  );
}

/** Moves a shape by an offset, leaving the original alone. */
export function moved(shape: Shape, dx: number, dy: number): Shape {
  return {
    ...shape,
    points: shape.points.map((at) => ({ x: at.x + dx, y: at.y + dy })),
  };
}

/** The topmost shape under a point, which is the last one drawn there. */
export function pickedAt(shapes: Shape[], point: Point, slack: number): number {
  for (let at = shapes.length - 1; at >= 0; at--) {
    if (touches(shapes[at], point, slack)) return at;
  }

  return -1;
}

/** How much of the window the panels above and below the picture take. */
export const CHROME = { width: 48, height: 140 };

/** The smallest window the toolbars still fit in. */
export const LEAST = { width: 720, height: 520 };

/**
 * The window a picture wants to be shown in.
 *
 * The window is sized to the picture rather than the picture stretched to the
 * window. Enlarging a screenshot past one to one only makes it blurry and
 * stops the marks lining up with what they are marking, so a small capture
 * gets a small window and looks right in it.
 *
 * Bounded both ways: never bigger than the screen, because a full-screen
 * capture would ask for a window larger than the display it came from, and
 * never smaller than the toolbars need. Where the screen is smaller than that
 * minimum the screen wins, since a window that does not fit is worse than a
 * cramped one.
 */
export function roomFor(
  picture: { width: number; height: number },
  screen: { width: number; height: number },
): { width: number; height: number } {
  const wanted = {
    width: picture.width + CHROME.width,
    height: picture.height + CHROME.height,
  };

  return {
    width: Math.min(Math.max(wanted.width, LEAST.width), screen.width),
    height: Math.min(Math.max(wanted.height, LEAST.height), screen.height),
  };
}

/**
 * The window under a point, or nothing.
 *
 * Topmost first, because the list arrives in Z-order and the window in front
 * is the one somebody is pointing at. The smallest match would be the other
 * reasonable rule and it is wrong: a dialog sitting over its parent is in
 * front, whether or not it is smaller.
 */
export function windowUnder<T extends { left: number; top: number; width: number; height: number }>(
  targets: T[],
  point: { x: number; y: number },
): T | null {
  for (const target of targets) {
    if (
      point.x >= target.left &&
      point.x < target.left + target.width &&
      point.y >= target.top &&
      point.y < target.top + target.height
    ) {
      return target;
    }
  }

  return null;
}

/**
 * The number the next badge should show.
 *
 * One past the highest so far rather than a count of them. After deleting the
 * third of five, a count would hand out 5 again and there would be two.
 */
export function nextNumber(shapes: Shape[], from: number): number {
  const highest = shapes
    .filter((shape) => shape.tool === "step")
    .reduce((most, shape) => Math.max(most, shape.number ?? 0), from - 1);

  return highest + 1;
}

/**
 * Puts the badges back in order after one is removed.
 *
 * Deleting the second of four should leave one, two, three, not one, three,
 * four. The order is the order they were placed in, which is their order in
 * the list, so nothing has to be sorted.
 */
export function renumbered(shapes: Shape[], from: number): Shape[] {
  let next = from;

  return shapes.map((shape) =>
    shape.tool === "step" ? { ...shape, number: next++ } : shape,
  );
}

/**
 * A crop rectangle, clamped to the picture it is cropping.
 *
 * A drag that runs off the edge is ordinary, and the part outside the picture
 * is not something that can be kept. Returns nothing where the overlap is too
 * small to be worth cropping to, so a stray click does not reduce a screenshot
 * to four pixels.
 */
export function croppedTo(
  drag: { x: number; y: number; w: number; h: number },
  picture: { width: number; height: number },
): { x: number; y: number; w: number; h: number } | null {
  const left = Math.max(0, Math.round(drag.x));
  const top = Math.max(0, Math.round(drag.y));
  const right = Math.min(picture.width, Math.round(drag.x + drag.w));
  const bottom = Math.min(picture.height, Math.round(drag.y + drag.h));

  const w = right - left;
  const h = bottom - top;

  // Small enough that it was a click or a slip rather than a crop.
  if (w < 16 || h < 16) return null;

  return { x: left, y: top, w, h };
}
