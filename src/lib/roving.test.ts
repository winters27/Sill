/**
 * Walking a group of choices with the arrow keys.
 *
 * Five groups in the settings window declared `role="radiogroup"` and not one
 * of them answered an arrow key, so a screen reader announced "radio button, 2
 * of 6" and then the keys that are supposed to answer that went to the page.
 * This is the part they all share.
 */
import { describe, expect, test } from "vitest";
import { rovingTab, rovingTo } from "$lib/roving";

describe("where an arrow key goes", () => {
  test("a row moves left and right", () => {
    expect(rovingTo("ArrowRight", 0, 3)).toBe(1);
    expect(rovingTo("ArrowLeft", 2, 3)).toBe(1);
  });

  test("a row ignores up and down, so the page can still scroll", () => {
    expect(rovingTo("ArrowDown", 0, 3)).toBeNull();
    expect(rovingTo("ArrowUp", 0, 3)).toBeNull();
  });

  test("a column moves up and down and ignores left and right", () => {
    expect(rovingTo("ArrowDown", 0, 3, "column")).toBe(1);
    expect(rovingTo("ArrowUp", 2, 3, "column")).toBe(1);
    expect(rovingTo("ArrowRight", 0, 3, "column")).toBeNull();
  });

  test("a grid answers both axes", () => {
    expect(rovingTo("ArrowDown", 0, 6, "both")).toBe(1);
    expect(rovingTo("ArrowRight", 0, 6, "both")).toBe(1);
    expect(rovingTo("ArrowUp", 3, 6, "both")).toBe(2);
    expect(rovingTo("ArrowLeft", 3, 6, "both")).toBe(2);
  });

  /*
   * Wrapping is the radio-group contract, and it is also the only way to get
   * back to the first option without counting: hold one key.
   */
  test("it wraps at both ends", () => {
    expect(rovingTo("ArrowRight", 2, 3)).toBe(0);
    expect(rovingTo("ArrowLeft", 0, 3)).toBe(2);
  });

  test("Home and End are absolute", () => {
    expect(rovingTo("Home", 2, 3)).toBe(0);
    expect(rovingTo("End", 0, 3)).toBe(2);
  });

  /*
   * `null` rather than "stay where you are", so a caller can tell a key it
   * does not handle from a key that moved nothing. Swallowing the first would
   * take Tab, Enter and Escape with it.
   */
  test("anything else is not this group's key", () => {
    for (const key of ["Enter", "Escape", "Tab", " ", "a", "PageDown"]) {
      expect(rovingTo(key, 0, 3)).toBeNull();
    }
  });

  test("an empty group has nowhere to go", () => {
    expect(rovingTo("ArrowRight", 0, 0)).toBeNull();
    expect(rovingTo("Home", 0, 0)).toBeNull();
  });

  test("a group of one stays on the one", () => {
    expect(rovingTo("ArrowRight", 0, 1)).toBe(0);
    expect(rovingTo("ArrowLeft", 0, 1)).toBe(0);
  });
});

describe("which option Tab lands on", () => {
  test("the chosen one, and only that one", () => {
    expect(rovingTab(0, 1)).toBe(-1);
    expect(rovingTab(1, 1)).toBe(0);
    expect(rovingTab(2, 1)).toBe(-1);
  });

  /*
   * The failure that would make a group unreachable rather than merely wrong.
   *
   * A value matching none of the options gives -1, and without a fallback
   * every option would be at tabindex -1 and Tab would skip the group
   * entirely. `Segmented` renders before its options arrive often enough for
   * this to be the normal first paint, not an edge case.
   */
  test("the first one when nothing is chosen", () => {
    expect(rovingTab(0, -1)).toBe(0);
    expect(rovingTab(1, -1)).toBe(-1);
  });
});
