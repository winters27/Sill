import { describe, expect, it, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";

import ExtIcon from "./ExtIcon.svelte";
import type { ExtIcon as Icon } from "$lib/exthost/present";

/**
 * What is drawn when the picture does not arrive.
 *
 * The comment in the component said a failed image "falls back to the letter
 * tile" and it reached the tile with no letter in it, so a row whose icon
 * failed drew an empty grey square: the one outcome that reads as a decision
 * rather than as a failure. Nothing catches that by looking, because an empty
 * tile is exactly what a deliberate blank would look like.
 */

let mounted: Record<string, unknown> | null = null;

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  document.body.innerHTML = "";
});

/** Mounts it and fails the picture, the way a browser would. */
function draw(props: { icon: Icon; small?: boolean; label?: string }, fail = true): void {
  mounted = mount(ExtIcon, { target: document.body, props });
  flushSync();

  if (!fail) return;
  const img = document.querySelector("img");
  img?.dispatchEvent(new Event("error"));
  flushSync();
}

describe("a picture that would not load", () => {
  it("falls back to the letter of what it was an icon of", () => {
    draw({ icon: { kind: "image", src: "https://example.test/x.png" }, label: "the moral panic" });

    expect(document.querySelector("img"), "the broken picture is not still drawn").toBeNull();
    expect(document.querySelector(".initial")?.textContent).toBe("T");
  });

  /*
   * An accessory pill has no label worth lettering: an "A" beside a number,
   * standing in for an arrow that would not load, is worse than the arrow's
   * absence. So it is drawn as nothing at all, which is what a row with no
   * icon already looks like.
   */
  it("is drawn as nothing at all when there is no label to letter", () => {
    draw({ icon: { kind: "image", src: "https://example.test/x.png" }, small: true });

    expect(document.querySelector(".ext-icon"), "an empty tile was left behind").toBeNull();
    expect(document.body.textContent?.trim()).toBe("");
  });

  it("is left alone while it is still loading", () => {
    draw({ icon: { kind: "image", src: "https://example.test/x.png" }, label: "a row" }, false);

    expect(document.querySelector("img")).not.toBeNull();
    expect(document.querySelector(".initial")).toBeNull();
  });
});

describe("a name with no mark", () => {
  /*
   * The behaviour that was already here, held so the change above kept it.
   *
   * This used to be `Windsock`, which was then one of the four hundred names
   * with no drawing. Every name Raycast publishes has one now, so the only
   * thing that still reaches the tile is what always really did: a relative
   * path into an extension's own assets, which arrives as a name and is not
   * one. Testing it with an invented name would have tested nothing.
   */
  it("still letters itself, and needs no label to do it", () => {
    draw({ icon: { kind: "mark", name: "wifi-signal.svg" } }, false);

    expect(document.querySelector(".initial")?.textContent).toBe("W");
  });

  it("draws its mark when it has one", () => {
    draw({ icon: { kind: "mark", name: "Star" } }, false);

    expect(document.querySelector("svg"), "Star is a mark Sill draws").not.toBeNull();
    expect(document.querySelector(".initial")).toBeNull();
  });
});

/**
 * The three ways a mark gets drawn, and the reason to check all three.
 *
 * A name resolving to a mark and a mark reaching a drawing are two different
 * questions, and `verify:source` only answers the first. A mark can be in the
 * table, be named by Rust, and still land on nothing, because the markup picks
 * between an outline, a set of characters and an arm in that order and a
 * mistake in that chain is invisible: every wrong answer is an empty box.
 */
describe("the three kinds of mark", () => {
  const svg = () => document.querySelector("svg");

  it("draws a name Phosphor covers as an outline", () => {
    draw({ icon: { kind: "mark", name: "Windsock" } }, false);

    expect(svg()?.getAttribute("viewBox"), "an outline is on Phosphor's box").toBe("0 0 256 256");
    expect(svg()?.querySelector("path")?.getAttribute("d")).toBeTruthy();
  });

  /* `Icon.Number42` is a picture of the number, so the number is drawn. */
  it("prints the digits of a numeral", () => {
    draw({ icon: { kind: "mark", name: "Number42" } }, false);

    expect(svg()?.querySelector("text")?.textContent).toBe("42");
    expect(document.querySelector(".initial"), "a numeral is not a lettered tile").toBeNull();
  });

  it("prints a letter pair for the case marks", () => {
    draw({ icon: { kind: "mark", name: "Uppercase" } }, false);
    expect(svg()?.querySelector("text")?.textContent).toBe("AA");
  });

  /*
   * A graded family is drawn by an arm, and the arms are what a generation
   * cannot produce. One of each family, because each is a separate arm and a
   * missing one falls through the chain to an empty `<svg>`.
   */
  it("draws the graded families rather than leaving an empty box", () => {
    for (const name of ["StackedBars3", "CircleProgress75", "Exclamationmark3", "ChessPiece"]) {
      draw({ icon: { kind: "mark", name } }, false);

      expect(svg()?.getAttribute("viewBox"), `${name} is drawn on the house box`).toBe("0 0 24 24");
      expect(svg()?.children.length, `${name} drew nothing`).toBeGreaterThan(0);

      unmount(mounted!);
      mounted = null;
      document.body.innerHTML = "";
    }
  });

  /*
   * The one name that must not become a plain star. `StarDisabled` asking for
   * a star would tell a row it is favourited when the extension said it is
   * not, which is worse than no picture at all.
   */
  it("strikes the star through rather than drawing a star", () => {
    draw({ icon: { kind: "mark", name: "StarDisabled" } }, false);

    expect(svg()?.querySelectorAll("path").length, "a star and the line through it").toBe(2);
  });
});
