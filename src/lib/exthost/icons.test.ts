/**
 * The icon a row can draw without waiting.
 *
 * New rows flashed a lettered tile before the picture swapped in, once per row
 * per keystroke. The cause was not that the shell is slow: it is that the
 * answer was held only as a promise, and a promise cannot be read. A row for a
 * path this session had already resolved still had to await it, so it rendered
 * the letter first and corrected itself afterwards.
 *
 * These hold the two properties that remove it rather than hide it: an answer
 * already in hand is readable synchronously, and an answer not yet in hand is
 * distinguishable from an answer of "no icon" so a row can reserve its tile
 * instead of guessing at a letter.
 */
import { beforeEach, describe, expect, test, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { appIcon, hasShellIcon, knownIcon } = await import("$lib/exthost/commands");

/** A fresh path per test, because the cache is the module's and outlives one. */
let counter = 0;
const somewhere = () => `C:\\Apps\\app-${++counter}.exe`;

beforeEach(() => {
  invoke.mockReset();
  vi.spyOn(console, "error").mockImplementation(() => {});
});

describe("what a row can draw before it waits", () => {
  test("a path nobody has asked about is not known", () => {
    expect(knownIcon(somewhere())).toBeUndefined();
  });

  /*
   * The property the whole fix rests on. Without it every row, including the
   * hundredth drawing of the same application, has to go through a promise,
   * and anything that goes through a promise has already rendered once.
   */
  test("a path already resolved answers without waiting", async () => {
    const path = somewhere();
    invoke.mockResolvedValue("data:image/png;base64,AAAA");

    await appIcon(path);

    expect(knownIcon(path)).toEqual({ uri: "data:image/png;base64,AAAA" });
  });

  /*
   * "No icon" and "not asked yet" are different answers and a row draws them
   * differently: the first is the lettered tile, the second is an empty slot.
   * Folding them into one nullable string is what made the letter a
   * placeholder, and a placeholder that turns out to be wrong is the flash.
   */
  test("a file with no icon is a known answer, not an unknown one", async () => {
    const path = somewhere();
    invoke.mockResolvedValue(null);

    await appIcon(path);

    expect(knownIcon(path)).toEqual({ uri: null });
  });

  test("nothing is known while the answer is still in flight", () => {
    const path = somewhere();
    let answer: (uri: string | null) => void = () => {};
    invoke.mockReturnValue(new Promise<string | null>((done) => (answer = done)));

    void appIcon(path);
    expect(knownIcon(path)).toBeUndefined();

    answer("data:image/png;base64,BBBB");
  });

  /*
   * A refusal is forgotten rather than remembered as "no icon", so a file
   * locked for a moment gets another chance. The synchronous read has to agree
   * with that: reporting a failed path as known would make the retry
   * unreachable and turn a transient lock into a permanently lettered row.
   */
  test("a refusal leaves nothing known, so the next row asks again", async () => {
    const path = somewhere();
    invoke.mockRejectedValue(new Error("denied"));

    expect(await appIcon(path)).toBeNull();
    expect(knownIcon(path)).toBeUndefined();

    invoke.mockResolvedValue("data:image/png;base64,CCCC");
    expect(await appIcon(path)).toBe("data:image/png;base64,CCCC");
  });

  test("one ask per path, however many rows draw it", async () => {
    const path = somewhere();
    invoke.mockResolvedValue("data:image/png;base64,DDDD");

    await Promise.all([appIcon(path), appIcon(path), appIcon(path)]);

    expect(invoke).toHaveBeenCalledTimes(1);
  });
});

describe("which paths are worth asking about", () => {
  test("a real file is", () => {
    expect(hasShellIcon("C:\\Windows\\notepad.exe", true)).toBe(true);
  });

  /*
   * Each of these is known to have no icon without a round trip, so the row
   * draws its letter on the first frame instead of reserving a tile for an
   * answer that was never coming.
   */
  test("an entry whose icon would say nothing is not", () => {
    expect(hasShellIcon("C:\\ext\\bundle.js", false)).toBe(false);
  });

  test("a row with no path at all is not", () => {
    expect(hasShellIcon("", true)).toBe(false);
  });

  test("a packaged app, which is an id rather than a file, is not", () => {
    expect(hasShellIcon("shell:AppsFolder\\Microsoft.Todos_8wek!App", true)).toBe(false);
  });
});
