import { describe, expect, it } from "vitest";

import {
  MODES,
  behaviourOf,
  drawsItsOwn,
  handlesItsOwnEscape,
  hasRowActions,
  isListMode,
  searchesOnType,
} from "./modes";

describe("what each mode behaves like", () => {
  it("has an answer for every mode there is", () => {
    for (const mode of MODES) {
      expect(behaviourOf(mode), `${mode} has no entry`).toBeDefined();
    }
  });

  it("says nothing about a mode that does not exist", () => {
    // The helpers are called with a plain string, because `mode` crosses from
    // Rust as one. A typo has to read as "no special behaviour" rather than as
    // a crash, and it must not accidentally match another mode's entry.
    expect(behaviourOf("modeThatIsNotReal")).toBeUndefined();
    expect(isListMode("modeThatIsNotReal")).toBe(false);
    expect(drawsItsOwn("modeThatIsNotReal")).toBe(false);
  });

  /**
   * The bug this table was written for.
   *
   * `output` was in neither the list of views that draw themselves nor the
   * list of ordinary lists, so the script output block rendered and the
   * previous result list rendered underneath it, with the arrow keys dead
   * because its count was zero. A mode has to be one or the other.
   */
  it("says what every mode puts on screen", () => {
    for (const mode of MODES) {
      const how = behaviourOf(mode)!;

      expect(
        ["own", "results", "behind"],
        `${mode} does not say what it shows`,
      ).toContain(how.shows);
    }
  });

  /**
   * A mode that shows the result list has to walk it.
   *
   * The one that did not was `output`: it drew the script output block and,
   * because it was in neither hand-written list, the previous result list as
   * well, with the arrow keys dead because its count was zero. Leaving the
   * list up is fine while a name is typed; leaving it up under a view of its
   * own is the bug.
   */
  it("walks the results wherever it shows them", () => {
    for (const mode of MODES) {
      const how = behaviourOf(mode)!;
      if (how.shows !== "results") continue;

      expect(
        how.rows,
        `${mode} shows the result list and does not walk it`,
      ).toBe("commands");
    }
  });

  it("only re-runs the index search for modes that walk its results", () => {
    for (const mode of MODES) {
      if (!searchesOnType(mode)) continue;

      expect(
        isListMode(mode),
        `${mode} re-runs the index search but does not show its results`,
      ).toBe(true);
    }
  });

  /**
   * The rule is about having rows, not about where they came from.
   *
   * It used to say "rows from the index", which was true of every mode with a
   * panel at the time and stopped being the point: the conversation list
   * counts through its own rows and has real actions on them. What has to
   * hold is that the panel has something to act on at all, which is any mode
   * whose arrow keys walk something.
   */
  it("offers row actions only where there are rows to act on", () => {
    for (const mode of MODES) {
      if (!hasRowActions(mode)) continue;

      const how = behaviourOf(mode)!;

      expect(
        how.rows,
        `${mode} takes actions from a selected row and has no rows`,
      ).not.toBe("none");
    }
  });

  /**
   * Every list of Sill's own rows has a panel, and the exceptions are named.
   *
   * `P1-09`: five views answered Ctrl+K with "no actions here" on rows that
   * plainly had some. Written as a list of exceptions rather than a list of
   * modes that have one, because that is the difference between forgetting a
   * new view and being told about it: the same inversion `ObjectKind::plainly`
   * makes in Rust, and the same one `groupOf` needed after its default filed
   * four kinds of thing as applications.
   *
   * An extension's own tree is not in scope. Those rows carry actions the
   * extension declared, and the registry has never known anything about them.
   */
  it("gives every list of Sill's own rows an action panel, or says why not", () => {
    /** A mode with rows of Sill's own and no panel, and the reason. */
    const without: Record<string, string> = {
      // The rows are folders, so the registry would offer everything it
      // offers a folder: reveal it, compress it, put it in the recycle bin.
      // This view was opened to answer "which folder", and offering to delete
      // one of the answers is not a feature.
      destination: "the rows are the answers to one question",
      // It has a panel; it does not take it from the selected row. Every row
      // in the history is the same kind of thing, so the answer cannot differ
      // between them and it is fetched once instead of per selection.
      clipboard: "one fetch for the whole list, because every row is alike",
      // The rows are not things, they are five offers Sill made. There is no
      // object behind "choose a key that is free" for the registry to describe
      // and nothing it could do to one, so Ctrl+K here would open a panel
      // about nothing on somebody's first minute with the application.
      welcome: "the rows are offers rather than objects",
      /*
       * There is exactly one thing to do to a button, and Enter already does
       * it. A panel here would be a menu of one entry, and naming a control is
       * refused too, because its id holds a window handle and one provider's
       * identifier for a button that stops existing when the window redraws.
       *
       * "Exactly one" is not left as an opinion. `tests/actions.rs` asserts
       * that the registry offers a screen control one action and no more, so
       * adding a second fails there and points back at this line.
       */
      controls: "one action, which Enter already runs",
    };

    for (const mode of MODES) {
      const how = behaviourOf(mode)!;

      // Sill's own rows: the ranked results, or a view counting rows it holds.
      if (how.rows !== "commands" && how.rows !== "own") continue;
      if (mode in without) {
        expect(hasRowActions(mode), `${mode} is excused and has a panel anyway`).toBe(false);
        continue;
      }

      expect(
        hasRowActions(mode),
        `${mode} answers Ctrl+K with "no actions here" on a row that has some`,
      ).toBe(true);
    }
  });

  it("keeps the modes that were known to answer Escape themselves", () => {
    // Named rather than derived: this is the behaviour the table replaced, and
    // a table that quietly changed it would be a refactor that broke Escape.
    for (const mode of ["conversations", "clipboard", "alias", "collection", "switcher", "command", "store"]) {
      expect(handlesItsOwnEscape(mode), `${mode} stopped answering Escape`).toBe(true);
    }

    expect(handlesItsOwnEscape("root")).toBe(false);
  });

  it("keeps the modes that were known to draw their own view", () => {
    // Named rather than derived, for the same reason as Escape above: this is
    // the behaviour the table replaced. Writing it out caught the table
    // dropping `collection`, which would have put the result list back under
    // the collection picker.
    for (const mode of [
      "command",
      "widgets",
      "clipboard",
      "argument",
      "collection",
      "ai",
      "conversations",
      "store",
    ]) {
      expect(drawsItsOwn(mode), `${mode} stopped drawing its own view`).toBe(true);
    }

    // And the ones that deliberately leave the list showing must not start.
    for (const mode of ["alias", "namingWorkspace", "root"]) {
      expect(drawsItsOwn(mode), `${mode} started drawing a view of its own`).toBe(false);
    }
  });
});
