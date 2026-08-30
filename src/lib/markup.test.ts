/**
 * The arithmetic behind marking up a picture.
 *
 * This is the part that can be wrong without looking wrong: an arrow head that
 * only misaligns at certain angles, a rectangle that vanishes when dragged the
 * other way, an undo that appears to do nothing because it removed a shape of
 * no size.
 */
import { describe, expect, test } from "vitest";
import {
  arrowHead,
  boxOf,
  croppedTo,
  fitted,
  nextNumber,
  renumbered,
  moved,
  roomFor,
  pickedAt,
  touches,
  windowUnder,
  worthKeeping,
  type Shape,
} from "$lib/markup";

function shape(over: Partial<Shape>): Shape {
  return {
    tool: "box",
    colour: "#ff0000",
    weight: 4,
    points: [
      { x: 0, y: 0 },
      { x: 50, y: 40 },
    ],
    ...over,
  };
}

describe("the rectangle two points describe", () => {
  test("is the same dragged in any direction", () => {
    const down = boxOf({ x: 10, y: 20 }, { x: 60, y: 80 });
    const up = boxOf({ x: 60, y: 80 }, { x: 10, y: 20 });

    expect(down).toEqual({ x: 10, y: 20, w: 50, h: 60 });
    expect(up).toEqual(down);
  });

  test("has no negative size, whichever corner came first", () => {
    const box = boxOf({ x: 100, y: 100 }, { x: 0, y: 0 });

    expect(box.w).toBeGreaterThan(0);
    expect(box.h).toBeGreaterThan(0);
  });
});

describe("an arrow head", () => {
  /**
   * The head has to turn with the line. Fixing the angles gives an arrow that
   * looks right pointing one way and broken pointing any other, which is easy
   * to miss because the first one you draw is usually to the right.
   */
  test("sits behind the tip whichever way the arrow points", () => {
    const tip = { x: 100, y: 100 };

    for (const from of [
      { x: 0, y: 100 },
      { x: 200, y: 100 },
      { x: 100, y: 0 },
      { x: 100, y: 200 },
      { x: 0, y: 0 },
    ]) {
      const [left, right] = arrowHead(from, tip, 20);

      // Both corners are nearer the start of the line than the tip is.
      const reach = Math.hypot(tip.x - from.x, tip.y - from.y);
      for (const corner of [left, right]) {
        const back = Math.hypot(corner.x - from.x, corner.y - from.y);
        expect(back).toBeLessThan(reach);
      }
    }
  });

  test("is symmetrical about the line", () => {
    const [left, right] = arrowHead({ x: 0, y: 0 }, { x: 100, y: 0 }, 20);

    // Pointing straight right, the two corners mirror in y and share an x.
    expect(left.x).toBeCloseTo(right.x, 6);
    expect(left.y).toBeCloseTo(-right.y, 6);
  });

  test("grows with the size it is given", () => {
    const small = arrowHead({ x: 0, y: 0 }, { x: 100, y: 0 }, 10)[0];
    const large = arrowHead({ x: 0, y: 0 }, { x: 100, y: 0 }, 30)[0];

    expect(100 - large.x).toBeGreaterThan(100 - small.x);
  });
});

describe("what is worth keeping", () => {
  /**
   * A click with no drag makes a shape of no size. Kept, it makes undo look
   * broken: the first press removes something nobody can see.
   */
  test("a click that never became a drag is not a shape", () => {
    const click = shape({
      points: [
        { x: 10, y: 10 },
        { x: 11, y: 11 },
      ],
    });

    expect(worthKeeping(click)).toBe(false);
  });

  test("a real drag is", () => {
    expect(worthKeeping(shape({}))).toBe(true);
  });

  test("a line with no width still counts if it has height", () => {
    const thin = shape({
      points: [
        { x: 10, y: 10 },
        { x: 10, y: 90 },
      ],
    });

    expect(worthKeeping(thin)).toBe(true);
  });

  test("an empty label is not a shape", () => {
    expect(worthKeeping(shape({ tool: "text", text: "   " }))).toBe(false);
    expect(worthKeeping(shape({ tool: "text", text: "note" }))).toBe(true);
  });

  test("a pen stroke needs more than the point it started on", () => {
    expect(worthKeeping(shape({ tool: "pen", points: [{ x: 5, y: 5 }] }))).toBe(false);
    expect(
      worthKeeping(
        shape({
          tool: "pen",
          points: [
            { x: 5, y: 5 },
            { x: 6, y: 7 },
          ],
        }),
      ),
    ).toBe(true);
  });
});

describe("fitting a picture to the window", () => {
  /**
   * A small picture is shown at its own size, not stretched to fill.
   *
   * The editor did look broken with a five hundred pixel capture adrift in an
   * eight hundred pixel window, and the fix is not to blow the picture up:
   * enlarging a screenshot past one to one only makes it blurry and stops the
   * marks lining up with what they mark. The window is sized to the picture
   * instead, which is `roomFor`.
   */
  test("a small picture is shown at its own size rather than stretched", () => {
    const size = fitted({ width: 500, height: 350 }, { width: 1000, height: 700 });

    expect(size).toEqual({ width: 500, height: 350 });
  });

  test("a large picture is scaled down to fit", () => {
    const size = fitted({ width: 3840, height: 2160 }, { width: 1000, height: 700 });

    expect(size.width).toBeLessThanOrEqual(1000);
    expect(size.height).toBeLessThanOrEqual(700);
  });

  test("the shape of the picture is kept", () => {
    const size = fitted({ width: 1600, height: 900 }, { width: 800, height: 800 });

    expect(size.width / size.height).toBeCloseTo(1600 / 900, 2);
  });

  /** Enlarging past one to one only makes a screenshot blurry. */
  test("it never grows past the picture's own pixels", () => {
    const size = fitted({ width: 200, height: 100 }, { width: 4000, height: 4000 });

    expect(size).toEqual({ width: 200, height: 100 });
  });

  test("a space with no room gives nothing rather than a negative size", () => {
    expect(fitted({ width: 100, height: 100 }, { width: 0, height: 500 })).toEqual({
      width: 0,
      height: 0,
    });
    expect(fitted({ width: 100, height: 100 }, { width: -40, height: -40 })).toEqual({
      width: 0,
      height: 0,
    });
  });
});

describe("the window the picture needs", () => {
  const CHROME = { width: 0, height: 140 };

  test("is the picture plus the panels around it", () => {
    const room = roomFor({ width: 600, height: 400 }, { width: 4000, height: 4000 });

    expect(room.width).toBeGreaterThanOrEqual(600);
    expect(room.height).toBeGreaterThanOrEqual(400 + CHROME.height);
  });

  /** A full-screen capture must not open a window bigger than the screen. */
  test("never asks for more room than the screen has", () => {
    const room = roomFor({ width: 3840, height: 2160 }, { width: 1920, height: 1080 });

    expect(room.width).toBeLessThanOrEqual(1920);
    expect(room.height).toBeLessThanOrEqual(1080);
  });

  /** Below a point the toolbars stop fitting and the window is unusable. */
  test("never asks for less than the toolbars need", () => {
    const room = roomFor({ width: 20, height: 20 }, { width: 4000, height: 4000 });

    expect(room.width).toBeGreaterThanOrEqual(720);
    expect(room.height).toBeGreaterThanOrEqual(520);
  });

  test("a screen smaller than the minimum still gives the screen", () => {
    const room = roomFor({ width: 1000, height: 1000 }, { width: 640, height: 480 });

    expect(room).toEqual({ width: 640, height: 480 });
  });
});

describe("picking a mark back up", () => {
  const box = shape({
    points: [
      { x: 100, y: 100 },
      { x: 200, y: 160 },
    ],
  });

  test("a click inside it finds it", () => {
    expect(touches(box, { x: 150, y: 130 }, 8)).toBe(true);
  });

  test("a click well away from it does not", () => {
    expect(touches(box, { x: 400, y: 400 }, 8)).toBe(false);
  });

  /**
   * A one pixel line is not something anybody can click on reliably, so near
   * enough counts. A thick mark is easier to hit than a thin one, which is
   * what somebody would expect.
   */
  test("just outside it still counts, by the slack given", () => {
    expect(touches(box, { x: 205, y: 130 }, 8)).toBe(true);
    expect(touches(box, { x: 260, y: 130 }, 8)).toBe(false);
  });

  test("the topmost mark wins where they overlap", () => {
    const under = shape({ colour: "#111111" });
    const over = shape({ colour: "#222222" });

    expect(pickedAt([under, over], { x: 25, y: 20 }, 8)).toBe(1);
  });

  test("nothing under the pointer is nothing, not the first mark", () => {
    expect(pickedAt([box], { x: 900, y: 900 }, 8)).toBe(-1);
    expect(pickedAt([], { x: 10, y: 10 }, 8)).toBe(-1);
  });

  test("a pen stroke is found near any part of it", () => {
    const scribble = shape({
      tool: "pen",
      points: [
        { x: 10, y: 10 },
        { x: 200, y: 10 },
        { x: 200, y: 200 },
      ],
    });

    expect(touches(scribble, { x: 200, y: 195 }, 8)).toBe(true);
    expect(touches(scribble, { x: 100, y: 150 }, 8)).toBe(false);
  });
});

describe("moving a mark", () => {
  test("every point shifts by the same amount", () => {
    const before = shape({
      points: [
        { x: 10, y: 20 },
        { x: 60, y: 80 },
      ],
    });

    const after = moved(before, 15, -5);

    expect(after.points).toEqual([
      { x: 25, y: 15 },
      { x: 75, y: 75 },
    ]);
  });

  test("the original is left alone", () => {
    const before = shape({});
    const points = structuredClone(before.points);

    moved(before, 100, 100);

    expect(before.points).toEqual(points);
  });

  test("what it is stays what it is", () => {
    const before = shape({ tool: "arrow", colour: "#abcdef", weight: 9 });
    const after = moved(before, 1, 1);

    expect(after.tool).toBe("arrow");
    expect(after.colour).toBe("#abcdef");
    expect(after.weight).toBe(9);
  });
});

describe("the window under the pointer", () => {
  const targets = [
    { id: 1, left: 100, top: 100, width: 400, height: 300 },
    { id: 2, left: 0, top: 0, width: 1920, height: 1080 },
  ];

  /**
   * The list arrives in Z-order, so the first match is the one in front. The
   * other plausible rule, smallest wins, is wrong: a dialog over its parent is
   * in front whether or not it is the smaller of the two.
   */
  test("is the topmost one, not the smallest", () => {
    expect(windowUnder(targets, { x: 200, y: 200 })?.id).toBe(1);
    // Outside the small one, still inside the big one.
    expect(windowUnder(targets, { x: 900, y: 900 })?.id).toBe(2);
  });

  test("is nothing where no window is", () => {
    expect(windowUnder(targets, { x: 5000, y: 5000 })).toBeNull();
    expect(windowUnder([], { x: 10, y: 10 })).toBeNull();
  });

  /** The far edge is outside, or two windows sharing an edge both match. */
  test("counts the near edge and not the far one", () => {
    const one = [{ id: 1, left: 100, top: 100, width: 100, height: 100 }];

    expect(windowUnder(one, { x: 100, y: 100 })?.id).toBe(1);
    expect(windowUnder(one, { x: 199, y: 199 })?.id).toBe(1);
    expect(windowUnder(one, { x: 200, y: 150 })).toBeNull();
  });

  /** A window at a negative origin is ordinary on a multi-display desk. */
  test("works where the screen starts at a negative coordinate", () => {
    const left = [{ id: 9, left: -1080, top: -801, width: 500, height: 400 }];

    expect(windowUnder(left, { x: -900, y: -700 })?.id).toBe(9);
    expect(windowUnder(left, { x: 100, y: 100 })).toBeNull();
  });
});

describe("numbered step badges", () => {
  function step(n: number): Shape {
    return shape({ tool: "step", number: n, points: [{ x: n * 10, y: 10 }] });
  }

  test("the first one starts where the setting says", () => {
    expect(nextNumber([], 1)).toBe(1);
    expect(nextNumber([], 0)).toBe(0);
    expect(nextNumber([], 5)).toBe(5);
  });

  test("each one is the next number", () => {
    expect(nextNumber([step(1)], 1)).toBe(2);
    expect(nextNumber([step(1), step(2), step(3)], 1)).toBe(4);
  });

  /**
   * One past the highest, not a count. After deleting the third of five, a
   * count would hand out 5 again and there would be two of them.
   */
  test("it is one past the highest, not a count", () => {
    expect(nextNumber([step(1), step(2), step(4), step(5)], 1)).toBe(6);
  });

  test("marks that are not badges are ignored", () => {
    expect(nextNumber([shape({}), step(1), shape({ tool: "arrow" })], 1)).toBe(2);
  });

  test("renumbering closes the gap a delete left", () => {
    const after = renumbered([step(1), step(3), step(4)], 1);

    expect(after.map((s) => s.number)).toEqual([1, 2, 3]);
  });

  test("renumbering follows the order they were placed in", () => {
    const after = renumbered([step(9), step(2), step(5)], 1);

    expect(after.map((s) => s.number)).toEqual([1, 2, 3]);
  });

  test("renumbering leaves every other mark exactly as it was", () => {
    const box = shape({ colour: "#123456" });
    const after = renumbered([box, step(4)], 1);

    expect(after[0]).toEqual(box);
    expect(after[1].number).toBe(1);
  });

  test("it can start from a number somebody chose", () => {
    expect(renumbered([step(1), step(2)], 7).map((s) => s.number)).toEqual([7, 8]);
  });
});

describe("cropping", () => {
  const picture = { width: 800, height: 600 };

  test("a drag inside the picture is kept as it is", () => {
    expect(croppedTo({ x: 100, y: 50, w: 300, h: 200 }, picture)).toEqual({
      x: 100,
      y: 50,
      w: 300,
      h: 200,
    });
  });

  /** A drag that runs off the edge is ordinary, and the outside is not real. */
  test("a drag off the edge is clamped to the picture", () => {
    expect(croppedTo({ x: -50, y: -50, w: 300, h: 200 }, picture)).toEqual({
      x: 0,
      y: 0,
      w: 250,
      h: 150,
    });

    expect(croppedTo({ x: 700, y: 500, w: 400, h: 400 }, picture)).toEqual({
      x: 700,
      y: 500,
      w: 100,
      h: 100,
    });
  });

  /** A slip should not reduce a screenshot to four pixels. */
  test("something too small to be a crop is refused", () => {
    expect(croppedTo({ x: 10, y: 10, w: 4, h: 200 }, picture)).toBeNull();
    expect(croppedTo({ x: 10, y: 10, w: 200, h: 4 }, picture)).toBeNull();
  });

  test("a drag entirely outside the picture is refused", () => {
    expect(croppedTo({ x: 2000, y: 2000, w: 300, h: 300 }, picture)).toBeNull();
  });

  test("the whole picture is a valid crop", () => {
    expect(croppedTo({ x: 0, y: 0, w: 800, h: 600 }, picture)).toEqual({
      x: 0,
      y: 0,
      w: 800,
      h: 600,
    });
  });
});
