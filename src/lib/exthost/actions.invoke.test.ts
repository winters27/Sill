/**
 * The answer an action had to be asked for reaches Rust.
 *
 * Renaming and moving were Tauri commands of their own that did the work
 * themselves, which made them the two actions only this window could run: no
 * key could be bound to either and the model could not reach them. They are
 * ordinary registry actions now, and the new name or the chosen folder travels
 * as `run_action`'s `argument`.
 *
 * This is the window's half of that. It checks the wire and nothing else,
 * because the wire is where the answer used to be lost: `run_action` ignoring
 * a third parameter would leave every rename saying "renaming needs a new
 * name" about a name somebody had just typed.
 */
import { beforeEach, describe, expect, test, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { runAction, asTarget } = await import("$lib/exthost/commands");
const { storeUninstall } = await import("$lib/store");

/** A file result, in the shape the launcher holds one. */
function file(path: string, title: string) {
  return asTarget({
    id: `file:${path}`,
    extension: "sill",
    extensionTitle: "Files",
    title,
    subtitle: path,
    mode: "file" as const,
    entrypoint: path,
    panel: null,
    matched: [],
  });
}

/** What `run_action` was told, or null if it was never called. */
function asked(): Record<string, unknown> | null {
  const call = invoke.mock.calls.find(([command]) => command === "run_action");
  return call ? (call[1] as Record<string, unknown>) : null;
}

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue({ message: "done" });
});

describe("an action that had to ask something first", () => {
  test("renaming sends the new name as the action's argument", async () => {
    await runAction("sill.file.rename", file("C:/work/notes.md", "notes.md"), "todo.md");

    expect(asked()).toEqual({
      action: "sill.file.rename",
      object: {
        id: "file:C:/work/notes.md",
        mode: "file",
        target: "C:/work/notes.md",
        title: "notes.md",
      },
      argument: "todo.md",
    });
  });

  /**
   * The folder is the answer, and the whole path of it.
   *
   * The destination picker borrows the result list rather than the field, so
   * what arrives here is a row's entrypoint rather than what was typed. Typing
   * only narrowed which folder.
   */
  test("moving sends the chosen folder as the action's argument", async () => {
    await runAction(
      "sill.file.move",
      file("C:/work/notes.md", "notes.md"),
      "C:/Users/me/Archive",
    );

    expect(asked()?.argument).toBe("C:/Users/me/Archive");
  });
});

describe("an action that asks nothing", () => {
  /**
   * Which is nearly all of them, and they send no answer at all.
   *
   * `undefined` rather than `""`: Rust reads a blank answer as no answer, so
   * either would work, and sending an empty string on every copy would put a
   * field in the payload that means nothing.
   */
  test("copying a path sends no argument", async () => {
    await runAction("sill.copyPath", file("C:/work/notes.md", "notes.md"));

    expect(asked()?.argument).toBeUndefined();
  });
});

describe("removing an extension", () => {
  /**
   * Answers with what Rust said rather than with a boolean.
   *
   * It goes through the action registry now, which answers in sentences: an
   * extension that was already gone is the end state somebody asked for rather
   * than a failure, and the sentence is what says which of the two happened.
   */
  test("hands back the sentence the registry produced", async () => {
    invoke.mockResolvedValue("Removed Spotify Controls");

    await expect(storeUninstall("spotify-controls")).resolves.toBe("Removed Spotify Controls");
    expect(invoke).toHaveBeenCalledWith("store_uninstall", { extension: "spotify-controls" });
  });
});
