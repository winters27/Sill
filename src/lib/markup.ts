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
  /** A plain segment, for underlining and for pointing without a head. */
  | "line"
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

/** Which tools a fill means anything for. */
export const CAN_FILL: Tool[] = ["box", "ellipse"];

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
  /**
   * Only for `arrow`: which head it was drawn with.
   *
   * Carried per mark for the same reason `colour` and `weight` are: changing
   * the picker is a choice about the next arrow, not about the ones already on
   * the picture. Optional, so a mark built without one still draws, with the
   * default head.
   */
  tip?: Tip;
  /**
   * Only for the tools in [`CAN_FILL`]: solid rather than an outline.
   *
   * Optional rather than defaulted to false, so a mark saved before this
   * existed reads as an outline, which is what it was drawn as.
   */
  fill?: boolean;
}

/**
 * One thing that can be taken back, and put back again.
 *
 * A crop is undoable and is not a shape, so a stack of shapes alone loses the
 * ordering between the two: undoing a crop and then a box has to redo the box
 * and then the crop, and a list that only holds shapes cannot say where the
 * crop went.
 */
export type Step =
  | { did: "mark"; shape: Shape }
  | { did: "crop"; crop: { x: number; y: number; w: number; h: number } | null };

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
 * How far a head opens either side of the line it points along.
 *
 * Wide enough to read as an arrow at a glance, narrow enough not to look like
 * a delta. The default the solid heads are proportioned around; a head is free
 * to state its own.
 */
const HEAD_SPREAD = Math.PI / 7;

/**
 * Where an arrow's head sits, as the two points behind its tip.
 *
 * Worked out here rather than in the drawing so it can be tested: an arrow
 * whose head does not turn with the line is the classic version of this bug,
 * and it only shows at certain angles.
 */
export function arrowHead(
  from: Point,
  to: Point,
  size: number,
  spread: number = HEAD_SPREAD,
): [Point, Point] {
  const angle = Math.atan2(to.y - from.y, to.x - from.x);

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
 * A point `by` back from the tip along the arrow's own line.
 *
 * Clamped at the arrow's start, so an arrow shorter than its own head stops
 * there rather than reaching back out of its tail.
 */
function back(from: Point, to: Point, by: number): Point {
  const span = Math.hypot(to.x - from.x, to.y - from.y);
  if (span === 0) return { x: to.x, y: to.y };

  const along = Math.min(by, span);

  return {
    x: to.x - (along * (to.x - from.x)) / span,
    y: to.y - (along * (to.y - from.y)) / span,
  };
}

/** How an arrow's head is drawn. */
export type Tip = "barbed" | "dart" | "chevron" | "triangle";

/** The head an arrow gets when nothing says otherwise. */
export const DEFAULT_TIP: Tip = "barbed";

/** Every head, in the order the picker offers them. */
export const TIPS: { id: Tip; hint: string }[] = [
  { id: "barbed", hint: "Swept head, notched into the shaft" },
  { id: "dart", hint: "A long, narrow head" },
  { id: "chevron", hint: "Two strokes, at the shaft's own weight" },
  { id: "triangle", hint: "A plain filled head" },
];

/**
 * How each head is proportioned, against the stroke it is drawn with.
 *
 * Everything is a multiple of the stroke rather than a pixel count, which is
 * what keeps a head looking the same at weight 2 and at weight 20. A fixed
 * size is the version of this that looks right at exactly one weight.
 *
 * `notch` is how far the back edge is pulled in toward the tip, as a fraction
 * of `reach`: zero leaves the back edge straight, and more sweeps it.
 */
const SHAPE: Record<Tip, { reach: number; spread: number; notch: number; solid: boolean }> = {
  barbed: { reach: 6, spread: HEAD_SPREAD, notch: 0.58, solid: true },
  dart: { reach: 7, spread: Math.PI / 11, notch: 0, solid: true },
  // Wider and shorter than the solid heads: an open head has no area to read
  // by, so it needs the angle to say which way it points.
  chevron: { reach: 5, spread: Math.PI / 6, notch: 0, solid: false },
  triangle: { reach: 6, spread: HEAD_SPREAD, notch: 0, solid: true },
};

/** An arrow's head, as something the drawing can paint without deciding anything. */
export interface Head {
  /** Where the shaft stops, so no part of the stroke shows past the head. */
  shaftEnd: Point;
  /** The head's corners, in the order they are joined. */
  points: Point[];
  /** Filled when true, stroked at the shaft's own weight when not. */
  solid: boolean;
}

/**
 * An arrow's head, whichever kind it was drawn with.
 *
 * One function rather than one per head, because every caller wants the same
 * three things and the difference between the heads is four numbers. A
 * `switch` in the drawing would put the shape of an arrow in the one file
 * that cannot be tested.
 *
 * ## Why the shaft stops short
 *
 * A solid head comes to a point, so for the last stroke width before the tip
 * it is narrower than the shaft drawn into it: a shaft taken the whole way
 * shows either side of the point, and a round cap puts another half a stroke
 * beyond it. Ending at the back edge, or at the notch where there is one,
 * buries the end of the shaft in the widest part of the head.
 *
 * An open head is the exception and its shaft does run to the tip. There is no
 * area to hide an end inside, the two barbs meet the shaft there, and a round
 * join closes the corner. Stopping short would open a gap instead.
 *
 * An unknown head falls back to the default rather than throwing: this is
 * reached from a stored preference, and a settings file holding a name from a
 * later version should draw an arrow rather than take the editor down.
 */
export function arrowTip(from: Point, to: Point, weight: number, tip: Tip = DEFAULT_TIP): Head {
  const { reach, spread, notch, solid } = SHAPE[tip] ?? SHAPE[DEFAULT_TIP];
  const size = weight * reach;
  const [left, right] = arrowHead(from, to, size, spread);
  const point = { x: to.x, y: to.y };

  if (!solid) {
    return { shaftEnd: point, points: [left, point, right], solid };
  }

  const base = back(from, to, notch > 0 ? size * notch : size * Math.cos(spread));

  return {
    shaftEnd: base,
    points: notch > 0 ? [point, left, base, right] : [point, left, right],
    solid,
  };
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

/**
 * The key that reaches each tool, so a hand on the mouse can change tool.
 *
 * One letter each, chosen from the tool's own name where the letter was free.
 * `x` for hide, because `h` is highlight and both start the same way; `v` for
 * select, which is what every editor uses for the arrow.
 *
 * Exported so a test can hold that every tool has one and no two share.
 */
export const TOOL_KEYS: Record<string, Tool | "select"> = {
  v: "select",
  b: "box",
  l: "line",
  a: "arrow",
  e: "ellipse",
  p: "pen",
  h: "highlight",
  x: "hide",
  t: "text",
  s: "step",
  c: "crop",
};

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

  // The rectangle a line describes is nearly all of it empty: a diagonal from
  // one corner of the picture to the other would be pickable anywhere on the
  // screen. So a line is hit near the line, not near its bounding box.
  if (shape.tool === "line" || shape.tool === "arrow") {
    const [from, to] = shape.points;
    if (!from || !to) return false;

    return nearSegment(from, to, point) <= reach;
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

/**
 * How far a point is from a segment, not from the infinite line through it.
 *
 * The difference is the whole point: a click a long way past the end of a short
 * arrow is close to that arrow's line and nowhere near the arrow.
 *
 * A segment of no length is a point, and the projection would divide by zero,
 * so that case answers with the distance to the point it is.
 */
export function nearSegment(from: Point, to: Point, point: Point): number {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const length = dx * dx + dy * dy;

  if (length === 0) return Math.hypot(point.x - from.x, point.y - from.y);

  // Where the point lands along the segment, clamped to its two ends.
  const along = Math.max(
    0,
    Math.min(1, ((point.x - from.x) * dx + (point.y - from.y) * dy) / length),
  );

  return Math.hypot(point.x - (from.x + along * dx), point.y - (from.y + along * dy));
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
