import { describe, expect, it, afterEach } from "vitest";
import { mount, unmount } from "svelte";
import Footer from "./Footer.svelte";
import { chinLine } from "$lib/update";

/**
 * The chin, rendered.
 *
 * `update.test.ts` proves `chinLine` returns the right words for each state.
 * It cannot prove they reach the screen, and the question that produced this
 * file was exactly that: an update was known about and the launcher appeared
 * to say nothing.
 *
 * What it says nothing about is which state the application is actually in.
 * A build that is already the newest draws nothing here and is right to.
 */

let mounted: Record<string, unknown> | null = null;

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  document.body.innerHTML = "";
});

function draw(over: Partial<Record<string, unknown>> = {}) {
  const target = document.createElement("div");
  document.body.append(target);

  mounted = mount(Footer, {
    target,
    props: {
      mode: "root",
      toast: null,
      status: "",
      update: null,
      prefs: null,
      viewTag: undefined,
      hasActions: false,
      onbuiltin: () => {},
      onrun: () => {},
      onactions: () => {},
      ontoastaction: () => {},
      onupdate: () => {},
      ...over,
    },
  });

  return target;
}

describe("the chin says there is a newer Sill", () => {
  it("draws the words and the button an available update carries", () => {
    const target = draw({
      update: chinLine({ kind: "available", version: "0.3.0", notes: null }),
    });

    expect(target.textContent).toContain("Sill 0.3.0 is available");
    const button = [...target.querySelectorAll("button")].find(
      (b) => b.textContent?.trim() === "Update and restart",
    );
    expect(button, "the press that installs it").toBeTruthy();
  });

  it("offers a restart once it is downloaded", () => {
    const target = draw({ update: chinLine({ kind: "ready", version: "0.3.0" }) });

    expect(target.textContent).toContain("Sill 0.3.0 is ready");
    expect(target.textContent).toContain("Restart now");
  });

  /**
   * A second press mid-download would start a second download, so the words
   * arrive without anything to press.
   */
  it("shows the download moving with no button on it", () => {
    const target = draw({
      update: chinLine({ kind: "downloading", version: "0.3.0", percent: 42 }),
    });

    expect(target.textContent).toContain("Downloading Sill 0.3.0, 42%");
    const buttons = [...target.querySelectorAll("button")].map((b) => b.textContent?.trim());
    expect(buttons).not.toContain("Update and restart");
  });

  /**
   * The order the chin resolves in, which is the one way an update that is
   * genuinely known about can still be invisible.
   */
  it("gives the line to the launcher's own status first", () => {
    const target = draw({
      status: "Alpha stopped: the worker exited",
      update: chinLine({ kind: "available", version: "0.3.0", notes: null }),
    });

    expect(target.textContent).toContain("Alpha stopped");
    expect(target.textContent).not.toContain("Sill 0.3.0 is available");
  });

  it("says nothing at all when there is nothing newer", () => {
    const target = draw({ update: chinLine({ kind: "upToDate" }) });

    expect(target.textContent).not.toContain("available");
    expect(target.textContent).not.toContain("Sill 0.");
  });
});
