/**
 * Polling that stops when nobody is looking.
 *
 * ## Why this exists
 *
 * A widget pinned to the chin kept polling into a window that had been hidden
 * for hours. The machine readout enumerated every process on the system once a
 * second, forever, because `setInterval` in `onMount` runs until the component
 * is destroyed and hiding a window destroys nothing.
 *
 * Nothing told the page it had been put away, either. Rust now emits
 * `sill://hidden` from the one place every hide path goes through, and this is
 * the other half: one helper, so a widget that wants a reading every so often
 * cannot accidentally want one while invisible.
 *
 * ## Why it takes a reading on the way back
 *
 * A gauge that comes back showing what the machine was doing before it was
 * hidden is worse than one that is blank for a moment: it is wrong, and it
 * looks right. So becoming visible reads immediately rather than waiting out
 * the interval.
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Whether a window of Sill's is on screen right now.
 *
 * One subscription for the whole page rather than one per caller: every
 * component asking the same question of the same two events would be a dozen
 * listeners saying the same thing. Assumed true until told otherwise, because
 * the page is only ever created inside a window that is about to be shown.
 */
let onScreen = true;

/** Callers who want to know the moment it comes back. */
const returning = new Set<() => void>();

let watching = false;

function watch() {
  if (watching) return;
  watching = true;

  void listen("sill://hidden", () => {
    onScreen = false;
  });

  void listen("sill://shown", () => {
    onScreen = true;
    for (const run of returning) run();
  });
}

/** Whether anything is looking at this window. */
export function visible(): boolean {
  watch();
  return onScreen;
}

/**
 * Runs something each time the window comes back.
 *
 * For work that is skipped while hidden and has to be caught up on: a list
 * that ignored the changes it was told about is a list that is wrong when it
 * reappears, which is worse than one that was briefly out of date.
 */
export function whenVisible(run: () => void): () => void {
  watch();
  returning.add(run);

  return () => {
    returning.delete(run);
  };
}

export function pollWhileVisible(take: () => void, every: number): () => void {
  let timer: ReturnType<typeof setInterval> | undefined;
  let torn = false;

  const start = () => {
    // Already running, or the component is gone and a late event arrived.
    if (timer !== undefined || torn) return;

    take();
    timer = setInterval(take, every);
  };

  const stop = () => {
    if (timer === undefined) return;

    clearInterval(timer);
    timer = undefined;
  };

  start();

  const listeners: Promise<UnlistenFn>[] = [
    listen("sill://hidden", stop),
    listen("sill://shown", start),
  ];

  return () => {
    torn = true;
    stop();

    // The listeners may still be being registered. Whatever they resolve to
    // is undone when they arrive, which is why this is a `then` rather than
    // an await nobody can wait for.
    for (const pending of listeners) void pending.then((off) => off());
  };
}
