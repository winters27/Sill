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
 * ## Why the window is named, and why the check is only here
 *
 * A Tauri event reaches every window. `emit` is "to all targets", and a page
 * listening with the default target receives events aimed at another window as
 * well, so the sender cannot narrow it and the receiver has to. Both events
 * therefore carry the label of the window they are about.
 *
 * Every window Sill has goes through `sleep_soon`, including the tray menu and
 * the windows that are built on first use, so before the label existed
 * **dismissing the tray menu told the launcher it had been hidden** and the
 * readings stopped while the launcher was still on screen. Nothing looked
 * wrong: a gauge that has stopped updating and a machine that is doing the
 * same thing as a moment ago are the same picture.
 *
 * The comparison lives here and nowhere else. Every caller subscribes through
 * this module rather than listening for itself, so the rule cannot be
 * remembered in three places and forgotten in the fourth.
 *
 * ## Why it takes a reading on the way back
 *
 * A gauge that comes back showing what the machine was doing before it was
 * hidden is worse than one that is blank for a moment: it is wrong, and it
 * looks right. So becoming visible reads immediately rather than waiting out
 * the interval.
 */
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

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

/** And the moment it goes away. */
const leaving = new Set<() => void>();

let watching = false;

function watch() {
  if (watching) return;
  watching = true;

  // Asked once. The label of the window a page is drawn in cannot change, and
  // reading it per event would be a call into Rust's metadata on every summon
  // of every window.
  const mine = getCurrentWindow().label;

  void listen<string>("sill://hidden", ({ payload }) => {
    if (payload !== mine) return;

    onScreen = false;
    for (const run of leaving) run();
  });

  void listen<string>("sill://shown", ({ payload }) => {
    if (payload !== mine) return;

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

/** And each time it is put away. */
export function whenHidden(run: () => void): () => void {
  watch();
  leaving.add(run);

  return () => {
    leaving.delete(run);
  };
}

export function pollWhileVisible(take: () => void, every: number): () => void {
  let timer: ReturnType<typeof setInterval> | undefined;

  const start = () => {
    // Already running.
    if (timer !== undefined) return;

    take();
    timer = setInterval(take, every);
  };

  const stop = () => {
    if (timer === undefined) return;

    clearInterval(timer);
    timer = undefined;
  };

  start();

  // Through the pair above rather than two `listen` calls of its own.
  //
  // Those resolved to their unlisten functions after this had already
  // returned, so tearing down meant unsubscribing from promises and a summon
  // in that gap could restart a poller whose component was gone. It carried a
  // flag to refuse that. A registration that is a set entry is undone the
  // moment it is asked for, so the gap does not exist and neither does the
  // flag.
  const off = [whenVisible(start), whenHidden(stop)];

  return () => {
    stop();

    for (const undo of off) undo();
  };
}
