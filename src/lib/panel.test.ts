import { describe, expect, it } from "vitest";

import { clipboardPanel, namable, rowPanel } from "./panel";
import type { ActionInfo, RankedCommand } from "./exthost/commands";

/** A registry answer, which is what Rust hands back for a kind. */
function registry(over: Partial<ActionInfo> = {}): ActionInfo {
  return { id: "sill.file.reveal", title: "Reveal in Explorer", primary: false, ...over };
}

/** A row, with only the fields the panel reads filled in properly. */
function row(over: Partial<RankedCommand> = {}): RankedCommand {
  return {
    id: "app:notepad",
    extension: "sill",
    extensionTitle: "Applications",
    title: "Notepad",
    subtitle: "",
    mode: "app",
    entrypoint: "notepad.exe",
    matched: [],
    ...over,
  } as RankedCommand;
}

/**
 * The rule `verify:source` also checks, asserted from the other side.
 *
 * The check reads the source for `action.shortcut` near the tag; this reads
 * the value that comes out. Both are worth having: the source check catches a
 * new builder that forgets, and this catches one that passes the wrong thing.
 */
describe("a registry action keeps the key Rust resolved for it", () => {
  it("carries the action's own shortcut into the clipboard panel", () => {
    const built = clipboardPanel({
      picked: [],
      rich: false,
      openCollection: null,
      registry: [registry({ id: "sill.text.readAloud", title: "Read Aloud", shortcut: { modifiers: ["ctrl"], key: "r" } })],
    });

    const aloud = built.find((entry) => entry.tag === "Sill.Action:sill.text.readAloud");
    expect(aloud?.shortcut).toEqual({ modifiers: ["ctrl"], key: "r" });
  });

  it("carries it into a row's panel too", () => {
    const built = rowPanel(
      [registry({ shortcut: { modifiers: ["ctrl", "shift"], key: "e" } })],
      row(),
    );

    expect(built[0]?.shortcut).toEqual({ modifiers: ["ctrl", "shift"], key: "e" });
  });

  /*
   * Enter is not the action's own key, and must not be read as one.
   *
   * For the primary action it is the `open` movement, handled by the chord map
   * with everything the launcher does on the way out. An action that declared
   * a shortcut AND is primary still shows Enter, because that is the key that
   * actually runs it from the panel.
   */
  it("shows Enter on the primary one whatever it declared", () => {
    const built = rowPanel(
      [registry({ primary: true, shortcut: { modifiers: ["ctrl"], key: "o" } })],
      row(),
    );

    expect(built[0]?.shortcut).toEqual({ modifiers: [], key: "enter" });
  });
});

describe("what the clipboard offers", () => {
  const bare = { picked: [] as number[], rich: false, openCollection: null, registry: [] };

  it("does not offer merging until there is something to merge", () => {
    const one = clipboardPanel({ ...bare, picked: [7] }).map((e) => e.tag);
    expect(one).not.toContain("Sill.ClipboardMerge");

    const two = clipboardPanel({ ...bare, picked: [7, 8] }).map((e) => e.tag);
    expect(two).toContain("Sill.ClipboardMerge");
    expect(two).toContain("Sill.ClipboardMergeInline");
  });

  // Offering it on a line of terminal output would be offering to do nothing.
  it("only offers plain paste on an entry that kept formatting", () => {
    expect(clipboardPanel(bare).map((e) => e.tag)).not.toContain("Sill.ClipboardPastePlain");
    expect(clipboardPanel({ ...bare, rich: true }).map((e) => e.tag)).toContain(
      "Sill.ClipboardPastePlain",
    );
  });

  // Removing something from a collection is only a thing while looking at one.
  it("only offers leaving a collection while one is open", () => {
    expect(clipboardPanel(bare).map((e) => e.tag)).not.toContain("Sill.ClipboardUncollect");

    const inside = clipboardPanel({ ...bare, openCollection: { id: 3, name: "Receipts" } });
    expect(inside.map((e) => e.tag)).toContain("Sill.ClipboardUncollect");
    expect(inside.find((e) => e.tag === "Sill.ClipboardUncollect")?.title).toBe(
      "Remove from Receipts",
    );
  });

  /*
   * The view already offers a plain Copy above, so the registry's primary for
   * a clipboard row would be the same thing said twice under two keys.
   */
  it("drops the registry's primary, which the view already offers", () => {
    const built = clipboardPanel({
      ...bare,
      registry: [
        registry({ id: "sill.clipboard.copy", title: "Copy", primary: true }),
        registry({ id: "sill.text.upper", title: "Upper Case" }),
      ],
    });

    expect(built.map((e) => e.tag)).not.toContain("Sill.Action:sill.clipboard.copy");
    expect(built.map((e) => e.tag)).toContain("Sill.Action:sill.text.upper");
  });

  // Two entries sharing an id would have the panel run whichever the filter
  // happened to leave at that position.
  it("gives every entry an id of its own", () => {
    const built = clipboardPanel({
      picked: [1, 2],
      rich: true,
      openCollection: { id: 3, name: "Receipts" },
      registry: [registry(), registry({ id: "sill.text.upper" })],
    });

    expect(new Set(built.map((e) => e.id)).size).toBe(built.length);
  });
});

describe("which rows are worth naming", () => {
  /*
   * An alias points at a command id and is matched against the index, so it is
   * only worth offering where the id survives a restart.
   */
  it("refuses the kinds whose id does not outlive the moment", () => {
    for (const mode of [
      "answer",
      "window",
      "audio-session",
      "process",
      "conversation",
      "past-conversation",
      "store-listing",
    ] as const) {
      expect(namable(row({ mode })), `${mode} should not be namable`).toBe(false);
    }
  });

  it("offers it on the kinds that are in the index", () => {
    for (const mode of ["app", "exe", "view", "no-view", "snippet", "quicklink"] as const) {
      expect(namable(row({ mode })), `${mode} should be namable`).toBe(true);
    }
  });

  it("says no when there is no row at all", () => {
    expect(namable(undefined)).toBe(false);
  });

  it("offers to forget a name only where there is one", () => {
    const without = rowPanel([], row()).map((e) => e.tag);
    expect(without).toContain("Sill.SetAlias");
    expect(without).not.toContain("Sill.ClearAlias");

    const withOne = rowPanel([], row({ alias: "np" }));
    expect(withOne.map((e) => e.tag)).toContain("Sill.ClearAlias");
    expect(withOne.find((e) => e.tag === "Sill.SetAlias")?.title).toBe('Rename "np"');
  });

  it("offers nothing about names on a row that cannot hold one", () => {
    const built = rowPanel([registry()], row({ mode: "window" })).map((e) => e.tag);
    expect(built).not.toContain("Sill.SetAlias");
    expect(built).not.toContain("Sill.ClearAlias");
  });
});
