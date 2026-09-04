import { describe, expect, it, vi } from "vitest";
import { saver, type Note } from "./notes";

/**
 * Saving a note while somebody types.
 *
 * The three ways this fails are all silent, which is why they are worth a test
 * each rather than a look at the window: the last paragraph never reaches
 * disk, a failed write throws away what somebody wrote, or an untouched note
 * is rewritten and moves to the top of the list for having been read.
 *
 * The clock is handed in, so none of these waits for anything.
 */

/** A clock a test drives, standing in for `setTimeout`. */
function clock() {
  const waiting = new Map<number, () => void>();
  let next = 1;

  return {
    later(run: () => void): number {
      const handle = next;
      next += 1;
      waiting.set(handle, run);
      return handle;
    },
    cancel(handle: number): void {
      waiting.delete(handle);
    },
    /** Everything currently waiting, in the order it was queued. */
    tick(): void {
      const due = [...waiting.entries()];
      waiting.clear();
      for (const [, run] of due) run();
    },
    get pending(): number {
      return waiting.size;
    },
  };
}

function note(text: string, id = "note-1"): Note {
  return { id, text, created: 1, updated: 2 };
}

/** A saver with a driven clock and a writer that records what it was given. */
function held(write: (text: string) => Promise<Note>) {
  const time = clock();
  const saving = saver({ write, later: time.later, cancel: time.cancel, wait: 700 });
  return { time, saving };
}

describe("saving while somebody types", () => {
  it("writes once after the typing stops, not once per keystroke", async () => {
    const written: string[] = [];
    const { time, saving } = held(async (text) => {
      written.push(text);
      return note(text);
    });

    for (const text of ["H", "He", "Hel", "Hell", "Hello"]) saving.changed(text);

    expect(written).toEqual([]);
    expect(saving.standing).toBe("unsaved");

    time.tick();
    await vi.waitFor(() => expect(written).toEqual(["Hello"]));
    expect(saving.standing).toBe("saved");
  });

  /**
   * The one the debounce loses on its own.
   *
   * Somebody types a line and closes the window inside the wait. The timer
   * belongs to the page and goes with it, so without a flush the last thing
   * they wrote is gone and nothing said so.
   */
  it("writes what is outstanding when the window is closing", async () => {
    const written: string[] = [];
    const { saving } = held(async (text) => {
      written.push(text);
      return note(text);
    });

    saving.changed("the last line");
    await saving.flush();

    expect(written).toEqual(["the last line"]);
    expect(saving.standing).toBe("saved");
  });

  /**
   * And a note somebody only opened and read is not rewritten.
   *
   * Every note carries an updated time and the list is ordered by it, so a
   * flush that wrote an identical file would move a note to the top for having
   * been looked at.
   */
  it("writes nothing when the note has not been touched", async () => {
    const written: string[] = [];
    const { saving } = held(async (text) => {
      written.push(text);
      return note(text);
    });

    saving.opened("already on disk");
    await saving.flush();

    expect(written).toEqual([]);
    expect(saving.standing).toBe("saved");
  });

  it("writes nothing when the text is typed back to what was already saved", async () => {
    const written: string[] = [];
    const { time, saving } = held(async (text) => {
      written.push(text);
      return note(text);
    });

    saving.opened("hello");
    saving.changed("hell");
    saving.changed("hello");

    time.tick();
    await vi.waitFor(() => expect(saving.standing).toBe("saved"));
    expect(written).toEqual([]);
  });

  /**
   * A write that failed keeps the whole note, not the part typed after it.
   *
   * Forgetting the pending text on failure is the version of this that looks
   * fine: the next keystroke saves, and what it saves is the note minus
   * whatever was outstanding when the disk was full.
   */
  it("keeps what could not be written and sends all of it next time", async () => {
    const written: string[] = [];
    let refuse = true;

    const { saving } = held(async (text) => {
      if (refuse) throw new Error("the disk is full");
      written.push(text);
      return note(text);
    });

    saving.changed("a whole paragraph");
    await saving.flush();

    expect(written).toEqual([]);
    expect(saving.standing).toBe("failed");

    refuse = false;
    await saving.flush();

    expect(written).toEqual(["a whole paragraph"]);
    expect(saving.standing).toBe("saved");
  });

  /** Typing during a write is not lost by the write finishing. */
  it("keeps what was typed while a write was in flight", async () => {
    const written: string[] = [];
    const waiting: Array<() => void> = [];

    const { time, saving } = held(async (text) => {
      await new Promise<void>((resolve) => waiting.push(resolve));
      written.push(text);
      return note(text);
    });

    saving.changed("first");
    time.tick();

    // The write is now waiting on the disk. Somebody carries on typing, which
    // is the moment a saver that cleared its pending text on write would lose
    // the second half of the sentence.
    await vi.waitFor(() => expect(waiting.length).toBe(1));
    saving.changed("first and second");
    expect(saving.standing).toBe("unsaved");

    waiting.shift()?.();
    await vi.waitFor(() => expect(written).toEqual(["first"]));

    // The keystroke armed its own timer while the first write was going.
    time.tick();
    await vi.waitFor(() => expect(waiting.length).toBe(1));
    waiting.shift()?.();

    await vi.waitFor(() => expect(written).toEqual(["first", "first and second"]));
    expect(saving.standing).toBe("saved");
  });

  /** Two flushes do not become two writes racing to be last. */
  it("does not start a second write while one is in flight", async () => {
    let started = 0;
    const waiting: Array<() => void> = [];

    const { saving } = held(async (text) => {
      started += 1;
      await new Promise<void>((resolve) => waiting.push(resolve));
      return note(text);
    });

    saving.changed("something");
    const first = saving.flush();
    const second = saving.flush();

    await vi.waitFor(() => expect(waiting.length).toBe(1));
    waiting.shift()?.();
    await Promise.all([first, second]);

    expect(started).toBe(1);
  });

  /** A timer that has already fired is not left armed for a page that has gone. */
  it("cancels the pending write when told to stop", () => {
    const { time, saving } = held(async (text) => note(text));

    saving.changed("typed");
    expect(time.pending).toBe(1);

    saving.stop();
    expect(time.pending).toBe(0);
  });
});
