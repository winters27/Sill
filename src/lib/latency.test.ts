/**
 * The bookkeeping behind keystroke-to-paint.
 *
 * Two things worth testing without a browser: that a long typing session
 * cannot grow this without end, and that "answered" and "presented" land one
 * frame apart rather than in the same one. The second is the whole
 * measurement: one frame of difference is sixteen milliseconds on an ordinary
 * display, and the budget is sixteen milliseconds.
 */
import { expect, test } from "vitest";

import { Latency, MOST_KEPT, aroundPaint } from "$lib/latency";

test("readings are kept per kind and handed over together", () => {
  const seen = new Latency();

  seen.record("keystrokeAnswered", 1_200);
  seen.record("keystrokePresented", 17_000);
  seen.record("keystrokeAnswered", 900);

  expect(seen.flush()).toEqual([
    { what: "keystrokeAnswered", tookUs: [1_200, 900] },
    { what: "keystrokePresented", tookUs: [17_000] },
  ]);
});

test("flushing forgets, so the next visit reports its own typing", () => {
  const seen = new Latency();

  seen.record("keystrokeAnswered", 1_200);
  seen.flush();

  expect(seen.flush()).toEqual([]);
  expect(seen.pending("keystrokeAnswered")).toEqual([]);
});

test("a kind nothing was measured for is not reported as nothing", () => {
  // "Not measured" and "measured as zero" are different answers, and Rust
  // should not have to tell them apart from an empty list.
  const seen = new Latency();
  seen.record("keystrokeAnswered", 0);

  expect(seen.flush()).toEqual([{ what: "keystrokeAnswered", tookUs: [0] }]);
});

test("typing for a long time cannot grow the list without end", () => {
  const seen = new Latency();

  for (let at = 0; at < MOST_KEPT + 20; at++) seen.record("keystrokeAnswered", at);

  const kept = seen.pending("keystrokeAnswered");
  expect(kept).toHaveLength(MOST_KEPT);
  // The oldest went, not the newest: the interesting reading is the one
  // nearest whatever somebody just noticed.
  expect(kept[0]).toBe(20);
  expect(kept[kept.length - 1]).toBe(MOST_KEPT + 19);
});

test("a clock that went backwards is recorded as nothing rather than as less than nothing", () => {
  const seen = new Latency();
  seen.record("keystrokeAnswered", -4);

  expect(seen.pending("keystrokeAnswered")).toEqual([0]);
});

/**
 * The one that matters.
 *
 * `answered` has to run in the frame that draws the rows and `presented` in
 * the one after it. Collapsing them into a single frame would report the same
 * number twice and quietly delete the paint from a paint measurement.
 */
test("answered is the frame that draws and presented is the frame after", () => {
  const frames: (() => void)[] = [];
  const run = (at: number) => {
    const due = frames.splice(0, frames.length);
    for (const one of due) one();
    return at;
  };

  const order: string[] = [];
  aroundPaint(
    (fn) => frames.push(fn),
    () => order.push("answered"),
    () => order.push("presented"),
  );

  expect(order).toEqual([]);
  run(1);
  expect(order).toEqual(["answered"]);
  run(2);
  expect(order).toEqual(["answered", "presented"]);
});
