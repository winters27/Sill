import { describe, expect, it } from "vitest";

import { deleteMeansTheRow, isTyping, typedInto } from "./typing";

const press = (key: string, held: Partial<Record<"ctrlKey" | "altKey" | "metaKey", boolean>> = {}) => ({
  key,
  ctrlKey: false,
  altKey: false,
  metaKey: false,
  ...held,
});

describe("deciding what a keystroke is", () => {
  it("treats a character as text", () => {
    for (const key of ["a", "Z", "7", " ", "-", "é", "😀"]) {
      expect(isTyping(press(key)), `${key} is not being typed`).toBe(true);
    }
  });

  it("treats a named key as a command", () => {
    for (const key of ["Escape", "Enter", "ArrowDown", "Tab", "Backspace", "Delete", "F5", "PageUp"]) {
      expect(isTyping(press(key)), `${key} would be typed into the field`).toBe(false);
    }
  });

  /**
   * Every navigation preset is a Ctrl chord, on purpose: the field has focus
   * the whole time, so a bare j is the letter j. Catching held modifiers here
   * is what keeps this from swallowing them.
   */
  it("leaves a chord alone whatever key it is on", () => {
    expect(isTyping(press("j", { ctrlKey: true }))).toBe(false);
    expect(isTyping(press("k", { altKey: true }))).toBe(false);
    expect(isTyping(press("n", { metaKey: true }))).toBe(false);
  });
});

describe("putting a character into the field", () => {
  it("appends when the caret is at the end", () => {
    expect(typedInto({ value: "fire", start: 4, end: 4 }, "f")).toEqual({
      value: "firef",
      caret: 5,
    });
  });

  it("inserts where the caret is", () => {
    expect(typedInto({ value: "fox", start: 1, end: 1 }, "i")).toEqual({
      value: "fiox",
      caret: 2,
    });
  });

  /**
   * The case the summon makes ordinary.
   *
   * `selectQueryOnSummon` selects the last query so the next character
   * replaces it. A keystroke that arrived before focus landed has to do the
   * same thing, or the fast typist gets the old query with a letter glued on.
   */
  it("replaces the selection, which is what a summon leaves behind", () => {
    expect(typedInto({ value: "yesterday", start: 0, end: 9 }, "n")).toEqual({
      value: "n",
      caret: 1,
    });
  });

  it("orders the ends, because a backwards drag reports them backwards", () => {
    expect(typedInto({ value: "abcdef", start: 5, end: 2 }, "X")).toEqual({
      value: "abXf",
      caret: 3,
    });
  });
});

describe("deciding what Delete destroys", () => {
  it("removes the row when nothing is typed", () => {
    expect(deleteMeansTheRow(press("Delete"), "")).toBe(true);
  });

  /**
   * The bug. Filtering the clipboard for "invoice" and pressing Delete to fix
   * a typo removed the entry under the cursor, with no confirmation.
   */
  it("edits the text when something is typed", () => {
    expect(deleteMeansTheRow(press("Delete"), "invoice")).toBe(false);
  });

  it("removes the row on Ctrl+Delete whatever is typed", () => {
    expect(deleteMeansTheRow(press("Delete", { ctrlKey: true }), "invoice")).toBe(true);
    expect(deleteMeansTheRow(press("Delete", { metaKey: true }), "invoice")).toBe(true);
  });

  it("is not any other key", () => {
    expect(deleteMeansTheRow(press("Backspace"), "")).toBe(false);
    expect(deleteMeansTheRow(press("d", { ctrlKey: true }), "")).toBe(false);
  });
});
