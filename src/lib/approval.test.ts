import { describe, expect, it } from "vitest";
import { SETTLES_MS, answersCard, shownAt } from "./approval";

function key(key: string, extra: Partial<KeyboardEvent> = {}) {
  return { key, shiftKey: false, repeat: false, isComposing: false, ...extra };
}

describe("answering an approval card", () => {
  it("does not let an Enter already on its way allow a card that just arrived", () => {
    const card = {};
    shownAt(card, 1000);

    expect(answersCard(key("Enter"), card, 1000 + SETTLES_MS - 1)).toBe(null);
  });

  it("lets Enter allow a card that has been read", () => {
    const card = {};
    shownAt(card, 1000);

    expect(answersCard(key("Enter"), card, 1000 + SETTLES_MS)).toBe("allow");
  });

  it("counts from the first time the card was seen, not the key", () => {
    const card = {};

    // The key handler can be the first to ask, when the card has not been
    // drawn yet. That is the moment it arrived, so it is not settled.
    expect(answersCard(key("Enter"), card, 5000)).toBe(null);
    expect(answersCard(key("Enter"), card, 5000 + SETTLES_MS)).toBe("allow");
  });

  it("never allows from Shift+Enter, a held key, or an unfinished composition", () => {
    const card = {};
    shownAt(card, 0);
    const later = SETTLES_MS * 10;

    expect(answersCard(key("Enter", { shiftKey: true }), card, later)).toBe(null);
    expect(answersCard(key("Enter", { repeat: true }), card, later)).toBe(null);
    expect(answersCard(key("Enter", { isComposing: true }), card, later)).toBe(null);
  });

  it("refuses on Escape at once", () => {
    const card = {};
    shownAt(card, 1000);

    expect(answersCard(key("Escape"), card, 1000)).toBe("refuse");
  });

  it("ignores every other key", () => {
    const card = {};
    shownAt(card, 0);

    expect(answersCard(key("a"), card, SETTLES_MS * 10)).toBe(null);
  });
});
