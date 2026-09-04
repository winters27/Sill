import { describe, expect, it } from "vitest";

import { hasRowActions, searchesOnType } from "./modes";
import { selectionRows, type SelectedObject } from "./selection";

const file: SelectedObject = {
  kind: "file",
  id: "file:C:\\work\\notes.md",
  target: "C:\\work\\notes.md",
  title: "notes.md",
  mode: "file",
};

const folder: SelectedObject = {
  kind: "folder",
  id: "file:C:\\work\\archive",
  target: "C:\\work\\archive",
  title: "archive",
  mode: "folder",
};

const text: SelectedObject = {
  kind: "text",
  id: "selection",
  target: "The first line\nand the second one",
  title: "The first line",
  mode: "text",
};

describe("rows for whatever was selected", () => {
  /**
   * The mode is what the action panel asks Rust about, so it has to survive.
   *
   * A folder flattened into a file here would ask for the file actions and get
   * them: no terminal in that folder, and an offer to hash it. The two kinds
   * exist in Rust precisely so this can be different.
   */
  it("keeps a folder a folder and a file a file", () => {
    const [one, two] = selectionRows([file, folder]);

    expect(one.mode).toBe("file");
    expect(two.mode).toBe("folder");
    expect(one.extensionTitle).toBe("File");
    expect(two.extensionTitle).toBe("Folder");
  });

  /** A path is its own icon, exactly as a searched file's is. */
  it("takes a file's icon from the file", () => {
    const [row] = selectionRows([file]);

    expect(row.icon).toBe("C:\\work\\notes.md");
    expect(row.subtitle).toBe("C:\\work\\notes.md");
    expect(row.entrypoint).toBe("C:\\work\\notes.md");
  });

  /**
   * Text has no file to take an icon from.
   *
   * Passing the text as the icon path asks the icon loader to extract one from
   * a paragraph, which is a failed shell call per row for a picture that could
   * never exist.
   */
  it("asks for no icon for a piece of text", () => {
    const [row] = selectionRows([text]);

    expect(row.icon).toBeNull();
  });

  /**
   * The subtitle is one line, whatever the selection was.
   *
   * A row is one line tall. A subtitle carrying real newlines either overflows
   * it or is silently cut at the first one, and the first line is already the
   * title, so the row would say the same thing twice.
   */
  it("flattens a multi-line selection into one line", () => {
    const [row] = selectionRows([text]);

    expect(row.subtitle).not.toContain("\n");
    expect(row.subtitle).toBe("The first line and the second one");
  });

  it("shortens a long selection rather than putting a paragraph on a row", () => {
    const [row] = selectionRows([{ ...text, target: "word ".repeat(200) }]);

    expect(row.subtitle.length).toBeLessThanOrEqual(81);
    expect(row.subtitle.endsWith("…")).toBe(true);
  });

  /** Explorer's order is the order on screen, so it is the order here. */
  it("keeps the order it was given", () => {
    const rows = selectionRows([folder, file, text]);

    expect(rows.map((row) => row.title)).toEqual(["archive", "notes.md", "The first line"]);
  });
});

describe("what the selection view behaves like", () => {
  /**
   * The action panel is the entire point of the mode.
   *
   * Without this the key summons a list of files with Ctrl+K answering "no
   * actions here", which is the exact complaint `P1-09` existed to fix,
   * arriving on a new view.
   */
  it("takes its actions from the row under the cursor", () => {
    expect(hasRowActions("selection")).toBe(true);
  });

  /**
   * Typing must not replace the answer with a different one.
   *
   * The list is what was selected. Re-running the index search on a keystroke
   * would push unrelated results in beside the three files somebody
   * highlighted, and the panel would then be acting on whichever of the two
   * kinds of row happened to be under the cursor.
   */
  it("does not re-search the index while it is up", () => {
    expect(searchesOnType("selection")).toBe(false);
  });
});
