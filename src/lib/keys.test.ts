import { describe, expect, it } from "vitest";
import { chordFor, keyOf, keysOf, modifiersOf } from "./keys";

function press(
  key: string,
  held: { ctrl?: boolean; alt?: boolean; shift?: boolean; meta?: boolean } = {},
) {
  return {
    key,
    ctrlKey: held.ctrl ?? false,
    altKey: held.alt ?? false,
    shiftKey: held.shift ?? false,
    metaKey: held.meta ?? false,
  };
}

describe("drawing a chord as keys", () => {
  it("splits a chord into one cap per key, in the order pressed", () => {
    expect(keysOf("Ctrl+Shift+Up")).toEqual(["Ctrl", "Shift", "↑"]);
    expect(keysOf("Alt+Space")).toEqual(["Alt", "Space"]);
  });

  it("names the Windows key Win, which is what is printed on it", () => {
    expect(keysOf("Super+K")).toEqual(["Win", "K"]);
  });

  it("draws nothing for an empty chord rather than one empty cap", () => {
    expect(keysOf("")).toEqual([]);
    expect(keysOf(" + ")).toEqual([]);
  });

  it("tells the modifiers from the key", () => {
    expect(modifiersOf("Ctrl+Alt+Delete")).toEqual(["Ctrl", "Alt"]);
    expect(keyOf("Ctrl+Alt+Delete")).toBe("Del");
    expect(keyOf("Ctrl")).toBe("");
  });
});

describe("what a recorder accepts", () => {
  it("shows the modifiers held so far and commits nothing", () => {
    expect(chordFor("hotkey", press("Control", { ctrl: true }))).toEqual({ held: ["Ctrl"] });
    expect(chordFor("action", press("Alt", { ctrl: true, alt: true }))).toEqual({
      held: ["Ctrl", "Alt"],
    });
  });

  it("takes a key on its own as a global key, and a combination too", () => {
    expect(chordFor("hotkey", press("F12"))).toEqual({ chord: "F12" });
    expect(chordFor("binding", press("k", { ctrl: true, alt: true }))).toEqual({
      chord: "Ctrl+Alt+K",
    });
  });

  it("allows a bare letter as a global key with a caution, because it takes that key from every program", () => {
    expect(chordFor("hotkey", press("k"))).toEqual({
      chord: "K",
      caution: "K on its own is taken from every program while Sill runs.",
    });
  });

  it("lets a navigation key be anything, because Down means Down", () => {
    expect(chordFor("navigation", press("ArrowDown"))).toEqual({ chord: "Down" });
    expect(chordFor("navigation", press("j", { ctrl: true }))).toEqual({ chord: "Ctrl+J" });
  });

  it("refuses the Windows key and a bare letter for an action key, and takes a lone key that types nothing", () => {
    expect(chordFor("action", press("k", { meta: true }))).toEqual({
      refused: "The Windows key cannot run an action",
    });
    expect(chordFor("action", press("k"))).toEqual({
      refused: "On its own that key would be typed into the search field. Add Ctrl or Alt, or use a key that types nothing.",
    });
    expect(chordFor("action", press("F5"))).toEqual({ chord: "F5" });
    expect(chordFor("action", press("k", { ctrl: true, shift: true }))).toEqual({
      chord: "Ctrl+Shift+K",
    });
  });

  it("names Space rather than writing a blank, so the chord can register", () => {
    expect(chordFor("hotkey", press(" ", { alt: true }))).toEqual({ chord: "Alt+Space" });
  });
});
