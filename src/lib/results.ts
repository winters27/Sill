/**
 * How the search field and the result list refer to each other.
 *
 * A screen reader is told which result is highlighted by being handed the id
 * of that row, so the row needs an id and the field needs to know which one to
 * name. Both sides have to agree on the spelling, and two copies of a string
 * that must agree is how they stop agreeing.
 *
 * Fixed rather than generated: there is one field and one list in the window,
 * and a generated id would have to be threaded between them to be useful.
 */

import type { FileSearchMissing, RankedCommand } from "$lib/exthost/commands";

/**
 * The id of the list on screen, which the search field points at.
 *
 * One name for five components. The root list, the clipboard, the store and
 * an extension's List and Grid are never on screen together: the window shows
 * exactly one of them at a time, and the field above them is the same field.
 * Giving each its own id would mean the field had to know which view it was
 * looking at to name it, which is the knowledge this constant exists to
 * remove.
 */
export const LISTBOX = "sill-results";

/** The id of one row, which is what `aria-activedescendant` names. */
export function optionId(index: number): string {
  return `sill-result-${index}`;
}

/**
 * The id of one item in a popover menu.
 *
 * Separate from `optionId` because a menu can be open OVER a list, so the two
 * sets of ids exist in the document at the same time and must not collide.
 * `which` names the menu: there are three, and two of them can be open at
 * once only in the sense that neither knows about the other.
 */
export function itemId(which: string, index: number): string {
  return `sill-${which}-item-${index}`;
}

/**
 * Whether the search field is filtering a list that is on screen and walkable.
 *
 * Only then is it a combobox. The same field is a plain one everywhere else:
 * where nothing on screen is a listbox there is nothing to point at, and
 * naming a row that is not rendered leaves a screen reader announcing nothing
 * at all, which is the state this was meant to fix. Naming a collection or an
 * alias is typing a name rather than filtering, so there is nothing to arrow
 * through either.
 *
 * `rows` is how many rows the arrow keys walk, not how many results the index
 * returned. Those are the same number at the root and nowhere else: the
 * clipboard, the store and an extension's list each count their own.
 *
 * `tree` is the tag an extension rendered, for the modes where the mode alone
 * does not say whether there is a list. See `showsAListbox`.
 */
// Which modes those are lives in `modes.ts`, with everything else that is
// decided per mode. It was four hand-written lists in three files, and they
// had already drifted.
export { isListMode, showsAListbox } from "./modes";
import { showsAListbox } from "./modes";

export function isBrowsing(mode: string, rows: number, tree?: string): boolean {
  return showsAListbox(mode, tree) && rows > 0;
}

/**
 * Where the highlight goes when the results change underneath it.
 *
 * ## Why the selection cannot just be a number
 *
 * It was one, and a number means nothing once the list it counted into has
 * been replaced. Two things went wrong with that.
 *
 * Typing another character kept row five selected, which is now a different
 * row: the highlight appeared to stay still while what it pointed at changed,
 * so Enter opened whatever had moved into that position.
 *
 * And the list is built in stages. Commands arrive, then files and browser
 * pages a moment later. Anything appended below the highlighted row is
 * harmless, but the moment the list is rebuilt the number is pointing into a
 * different array.
 *
 * ## The two rules
 *
 * A new query starts at the top, which is what every launcher does and what
 * somebody typing expects. The same query keeps the row it was on, by id, so
 * a late page of files cannot move it. If that row is gone, the top.
 */
export function selectionAfter(
  held: { id: string | undefined; index: number },
  rows: { id: string }[],
  sameQuery: boolean,
): number {
  if (!sameQuery) return 0;

  if (held.id !== undefined) {
    const at = rows.findIndex((row) => row.id === held.id);
    if (at >= 0) return at;
  }

  // The row is gone: a source answered differently, or the list shrank. The
  // top rather than the same number, which would be an arbitrary row.
  return held.index < rows.length ? held.index : 0;
}

/**
 * The row that appears where files would have been.
 *
 * A row rather than a message under the field, because a message is something
 * to read and this is something to do. It sits with the files it is standing
 * in for, and Enter fixes the thing it names.
 *
 * Here rather than in the window because there really are three answers and
 * each says something different about what has gone wrong: an index still
 * being built is not the same as no folders chosen, and neither is the same as
 * a program that is installed and not running. One wording for all three would
 * tell somebody to turn on a thing that is already on.
 */
export function fileSearchRow(why: FileSearchMissing): RankedCommand {
  const said = {
    indexing: {
      title: "Reading your files",
      subtitle: "Sill is going through your folders for the first time. This takes a moment and happens once.",
    },
    absent: {
      title: "Turn on file search",
      subtitle: "Sill is not indexing any folders. Choose this to start, and it will read the ones you work in.",
    },
    asleep: {
      title: "Start file search",
      subtitle: "Everything is installed but not running, so there is nothing to search. Choose this to start it.",
    },
  }[why];

  return {
    id: "sill:file-search",
    extension: "sill",
    extensionTitle: "Files",
    title: said.title,
    subtitle: said.subtitle,
    mode: "file-setup",
    entrypoint: "",
    matched: [],
  };
}
