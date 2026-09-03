import { describe, expect, it } from "vitest";

import { actionFor, matchesShortcut, type Keystroke } from "./actions";
import type { ActionEntry } from "./actions";

const press = (
  key: string,
  held: Partial<Omit<Keystroke, "key">> = {},
): Keystroke => ({
  key,
  ctrlKey: false,
  altKey: false,
  metaKey: false,
  shiftKey: false,
  ...held,
});

const action = (title: string, modifiers: string[], key: string): ActionEntry =>
  ({
    id: title.length,
    title,
    tag: `Sill.${title}`,
    props: {},
    shortcut: { modifiers, key },
  }) as ActionEntry;

describe("matching a keystroke against an advertised shortcut", () => {
  /**
   * The one the panel has been drawing and never running. A rich clipboard
   * entry offers "Paste as Plain Text  Ctrl Shift ↵", and that chord did
   * nothing at all.
   */
  it("matches the chord the panel draws", () => {
    const plain = { modifiers: ["ctrl", "shift"], key: "enter" };

    expect(
      matchesShortcut(press("Enter", { ctrlKey: true, shiftKey: true }), plain),
    ).toBe(true);
  });

  /**
   * A held modifier nobody asked for is a different chord.
   *
   * Without this, Ctrl+Shift+Enter would also fire an action bound to
   * Ctrl+Enter, and which one ran would depend on list order.
   */
  it("refuses a modifier the shortcut did not ask for", () => {
    const ctrlEnter = { modifiers: ["ctrl"], key: "enter" };

    expect(matchesShortcut(press("Enter", { ctrlKey: true }), ctrlEnter)).toBe(true);
    expect(
      matchesShortcut(press("Enter", { ctrlKey: true, shiftKey: true }), ctrlEnter),
    ).toBe(false);
    expect(
      matchesShortcut(press("Enter", { ctrlKey: true, altKey: true }), ctrlEnter),
    ).toBe(false);
  });

  it("refuses a modifier the shortcut asked for and nobody held", () => {
    expect(
      matchesShortcut(press("Enter"), { modifiers: ["ctrl"], key: "enter" }),
    ).toBe(false);
  });

  /**
   * Extensions are written for macOS, where the modifier is Cmd. The panel
   * already draws `cmd` as Ctrl, so matching has to agree with the drawing or
   * the label and the behaviour disagree.
   */
  it("reads Raycast's mac modifiers as the Windows ones", () => {
    for (const name of ["cmd", "cmdOrCtrl", "ctrl"]) {
      expect(
        matchesShortcut(press("c", { ctrlKey: true }), { modifiers: [name], key: "c" }),
        `${name} did not match Ctrl`,
      ).toBe(true);
    }

    expect(
      matchesShortcut(press("c", { altKey: true }), { modifiers: ["opt"], key: "c" }),
    ).toBe(true);
  });

  /**
   * The DOM and Raycast name the same keys differently.
   */
  it("reconciles the two vocabularies of key names", () => {
    expect(
      matchesShortcut(press("Enter", { ctrlKey: true }), {
        modifiers: ["ctrl"],
        key: "return",
      }),
    ).toBe(true);

    expect(
      matchesShortcut(press("ArrowUp", { altKey: true }), {
        modifiers: ["alt"],
        key: "arrowUp",
      }),
    ).toBe(true);

    expect(
      matchesShortcut(press("Delete", { ctrlKey: true }), {
        modifiers: ["ctrl"],
        key: "delete",
      }),
    ).toBe(true);

    // Shift makes a letter arrive uppercase, and the shortcut is written
    // lowercase whether or not Shift is part of it.
    expect(
      matchesShortcut(press("K", { ctrlKey: true, shiftKey: true }), {
        modifiers: ["ctrl", "shift"],
        key: "k",
      }),
    ).toBe(true);
  });
});

describe("finding the action a keystroke runs", () => {
  const actions = [
    action("Copy", ["ctrl"], "c"),
    action("Paste as Plain Text", ["ctrl", "shift"], "enter"),
    action("Delete", ["ctrl"], "delete"),
  ];

  it("finds the one whose chord was pressed", () => {
    expect(actionFor(press("Enter", { ctrlKey: true, shiftKey: true }), actions)).toBe(1);
    expect(actionFor(press("Delete", { ctrlKey: true }), actions)).toBe(2);
  });

  it("finds nothing for a chord nobody claimed", () => {
    expect(actionFor(press("Enter", { ctrlKey: true }), actions)).toBe(-1);
    expect(actionFor(press("q"), actions)).toBe(-1);
  });

  it("finds nothing among actions with no shortcuts", () => {
    const bare = [{ id: 1, title: "Open", tag: "Sill.Open", props: {} }] as ActionEntry[];

    expect(actionFor(press("Enter", { ctrlKey: true }), bare)).toBe(-1);
  });
});

/**
 * The chords the action registry declares in Rust, as they arrive over IPC.
 *
 * `Modifier` serialises to these exact lower-case names and the key is
 * Raycast's, so this is the shape `matchesShortcut` is handed rather than a
 * shape invented for the test. The list is the one pinned in
 * `src-tauri/tests/actions.rs::DECLARED`; if the two ever disagree, the Rust
 * side is the authority and this is the copy that is wrong.
 */
describe("a shortcut a registry action ships with", () => {
  const declared = [
    { title: "Copy Path", modifiers: ["ctrl", "shift"], key: "c", press: "C" },
    { title: "Copy Name", modifiers: ["ctrl", "shift"], key: "n", press: "N" },
    { title: "Show in Folder", modifiers: ["ctrl", "shift"], key: "e", press: "E" },
    { title: "Open Terminal Here", modifiers: ["ctrl", "shift"], key: "t", press: "T" },
    { title: "Copy Address", modifiers: ["ctrl", "shift"], key: "c", press: "C" },
    { title: "Read Aloud", modifiers: ["ctrl", "shift"], key: "s", press: "S" },
  ];

  /**
   * Every advertised chord fires.
   *
   * Shift makes the letter arrive upper case, which is the case this would
   * quietly fail on: the chord is written lower case in Rust and the DOM
   * hands over `C`.
   */
  it("fires on the key it advertises", () => {
    for (const { title, modifiers, key, press: typed } of declared) {
      const shortcut = { modifiers, key };

      expect(
        matchesShortcut(press(typed, { ctrlKey: true, shiftKey: true }), shortcut),
        `${title} does not fire on the chord it draws`,
      ).toBe(true);
    }
  });

  it("does not fire on the same letter without Shift", () => {
    for (const { title, modifiers, key } of declared) {
      expect(
        matchesShortcut(press(key, { ctrlKey: true }), { modifiers, key }),
        `${title} fired on a chord it does not advertise`,
      ).toBe(false);
    }
  });

  /**
   * A person can set two actions to one key, and Rust reports that on the
   * settings row. What must never happen is both of them running: one thing
   * happening and being told which is recoverable, two is not.
   */
  it("runs one action and not both when two claim the same chord", () => {
    const contested = [
      action("Copy Path", ["ctrl", "shift"], "c"),
      action("Copy Name", ["ctrl", "shift"], "c"),
    ];

    const at = actionFor(press("C", { ctrlKey: true, shiftKey: true }), contested);

    expect(at).toBe(0);
    // `actionFor` returns one index, so the second cannot also run. Asserted
    // rather than assumed, because a matcher that returned a list would be a
    // silent change from "one of them" to "all of them".
    expect(typeof at).toBe("number");
    expect(contested.filter((a) => matchesShortcut(press("C", { ctrlKey: true, shiftKey: true }), a.shortcut!)).length).toBe(2);
  });
});
