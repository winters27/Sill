/**
 * The notes window's side of the boundary, and the one piece of it worth
 * testing.
 *
 * A note is saved while somebody types, which is three decisions rather than
 * one: when to write, what to do when a write fails, and what has to happen
 * before the window goes away. All three have been got wrong here before in
 * other projects, and the way they fail is that the last thing somebody typed
 * is not on disk and nothing said so.
 *
 * So the saving is a small object with no Svelte in it and no Tauri in it,
 * driven by a clock and a writer that are both handed in. Every case below is
 * a test rather than a thing somebody has to reproduce by typing fast.
 */
import { invoke } from "@tauri-apps/api/core";

/** One note, as Rust keeps it. */
export interface Note {
  id: string;
  text: string;
  created: number;
  updated: number;
}

export async function readNote(id: string): Promise<Note | null> {
  return await invoke<Note | null>("note_read", { id });
}

export async function writeNote(id: string, text: string): Promise<Note> {
  return await invoke<Note>("note_write", { id, text });
}

export async function forgetNote(id: string): Promise<boolean> {
  return await invoke<boolean>("note_forget", { id });
}

/** How long after the last keystroke a note is written. */
export const AFTER_TYPING = 700;

/** What the window shows about whether the note is safe. */
export type Standing = "saved" | "saving" | "unsaved" | "failed";

/** The clock and the writer, both handed in so a test can hold them. */
export interface Wiring {
  /** Writes the note, and answers with what Rust now holds. */
  write: (text: string) => Promise<Note>;
  /** `setTimeout`, or something a test can drive. */
  later?: (run: () => void, ms: number) => number;
  /** `clearTimeout`, matching. */
  cancel?: (handle: number) => void;
  /** How long to wait after the last keystroke. */
  wait?: number;
}

/**
 * Saves a note while somebody types, and says where it stands.
 *
 * ## Why a flush exists at all
 *
 * A debounce on its own loses the last thing typed. Somebody writes a line and
 * closes the window inside the wait, and the timer is cancelled with the
 * window that owned it. So the window flushes before it goes, and the flush
 * has to be the same code path as the timer or it is a second saver that can
 * disagree with the first.
 *
 * ## Why a failure keeps the text
 *
 * The pending text is only forgotten once a write has actually come back. A
 * write that fails leaves it exactly where it was, so the next keystroke or
 * the next flush tries again with the whole of what somebody wrote rather than
 * with whatever they typed after the failure.
 */
export function saver(wiring: Wiring) {
  const later = wiring.later ?? ((run, ms) => setTimeout(run, ms) as unknown as number);
  const cancel = wiring.cancel ?? ((handle) => clearTimeout(handle));
  const wait = wiring.wait ?? AFTER_TYPING;

  /** What has been typed and not yet written. `null` when there is nothing. */
  let pending: string | null = null;
  /** What the last successful write put on disk. */
  let written: string | null = null;
  let timer: number | null = null;
  let inFlight: Promise<void> | null = null;
  let standing: Standing = "saved";

  function stop(): void {
    if (timer !== null) {
      cancel(timer);
      timer = null;
    }
  }

  async function put(): Promise<void> {
    stop();

    // Nothing new since the last write. Saying so rather than writing an
    // identical file is what stops a flush on close rewriting a note somebody
    // only opened and read.
    if (pending === null || pending === written) {
      pending = null;
      standing = "saved";
      return;
    }

    const going = pending;
    standing = "saving";

    try {
      const saved = await wiring.write(going);
      written = saved.text;

      // Only the text that actually went is cleared. Somebody who kept typing
      // during the write still has unsaved work, and the timer they started
      // is what writes it.
      if (pending === going) {
        pending = null;
        standing = "saved";
      }

      return;
    } catch {
      // Left exactly where it was, so the next attempt carries the whole note
      // rather than the part typed after the failure.
      standing = "failed";
    }
  }

  /**
   * Writes whatever is outstanding, now.
   *
   * Awaits a write already in flight rather than starting a second one, so
   * closing the window during a save does not produce two writes of the same
   * note racing to be last.
   */
  async function flush(): Promise<void> {
    if (inFlight) await inFlight;

    inFlight = put();
    try {
      await inFlight;
    } finally {
      inFlight = null;
    }
  }

  return {
    /** Whether the note on disk is the note on screen. */
    get standing(): Standing {
      return standing;
    },

    /** Somebody typed. */
    changed(text: string): void {
      pending = text;
      standing = text === written ? "saved" : "unsaved";
      stop();
      timer = later(() => void flush(), wait);
    },

    flush,

    /** Tells the saver which text is already on disk.
     *
     *  Called once when the window opens a note, so that closing it again
     *  without typing writes nothing. Without this the first flush would
     *  rewrite an untouched note and move its place in the list. */
    opened(text: string): void {
      written = text;
      pending = null;
      standing = "saved";
    },

    /** Drops the pending timer without writing.
     *
     *  Never wanted on its own: it is here so a window that has flushed can
     *  make sure nothing fires afterwards into a page that has gone. */
    stop,
  };
}
