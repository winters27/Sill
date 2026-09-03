/**
 * Which window a "you are hidden" event is actually about.
 *
 * A Tauri event reaches every window. `emit` is "to all targets", and a page
 * listening with the default target receives events aimed at another window as
 * well, so the sender cannot narrow it and the receiver has to.
 *
 * Every window Sill has goes through `sleep_soon` on its way out, including
 * the tray menu and the windows built on first use. So dismissing the tray
 * menu told the launcher it had been hidden, and the launcher stopped taking
 * its readings while it was still on screen. Nothing looked wrong: a gauge
 * that has stopped updating and a machine that is doing the same thing as a
 * moment ago are the same picture.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

/** The handler Rust would be calling, one per event name. */
const handlers = new Map<string, (event: { payload: string }) => void>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, run: (payload: { payload: string }) => void) => {
    handlers.set(event, run);
    return Promise.resolve(() => handlers.delete(event));
  },
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: "main" }),
}));

const { pollWhileVisible, visible } = await import("$lib/visible");

/** What Rust emits when a window is put away or brought back. */
function windowEvent(name: "sill://hidden" | "sill://shown", label: string) {
  handlers.get(name)?.({ payload: label });
}

beforeEach(() => {
  vi.useFakeTimers();

  // Subscribes on the first test, and puts the module back on screen on the
  // rest, so no test inherits the state another one left.
  visible();
  windowEvent("sill://shown", "main");
});

afterEach(() => {
  vi.useRealTimers();
});

/** A poller, and a count of how many readings it has taken. */
function polling(every = 1000) {
  let taken = 0;
  const stop = pollWhileVisible(() => (taken += 1), every);

  return { stop, taken: () => taken };
}

test("another window being put away does not stop this one", () => {
  const poll = polling();
  expect(poll.taken()).toBe(1);

  windowEvent("sill://hidden", "traymenu");
  vi.advanceTimersByTime(3000);

  expect(poll.taken()).toBe(4);
  expect(visible()).toBe(true);

  poll.stop();
});

test("this window being put away does", () => {
  const poll = polling();

  windowEvent("sill://hidden", "main");
  vi.advanceTimersByTime(3000);

  expect(poll.taken()).toBe(1);
  expect(visible()).toBe(false);

  poll.stop();
});

/**
 * The other half, and the reason the reading is immediate: a gauge that comes
 * back showing what the machine was doing before it was hidden is worse than
 * one that is blank for a moment, because it is wrong and it looks right.
 */
test("and coming back reads straight away rather than waiting out the interval", () => {
  const poll = polling();

  windowEvent("sill://hidden", "main");
  windowEvent("sill://shown", "main");

  expect(poll.taken()).toBe(2);

  poll.stop();
});

/**
 * A component is destroyed while the window it was in is hidden, and the
 * summon that follows arrives for a poller nobody is drawing any more. A
 * teardown that does not unsubscribe starts an interval with no owner and no
 * way to reach it: the reading-forever this module exists to stop, one layer
 * further down.
 */
test("a poller that has been torn down stays torn down", () => {
  const poll = polling();
  poll.stop();

  windowEvent("sill://shown", "main");
  vi.advanceTimersByTime(3000);

  expect(poll.taken()).toBe(1);
});
