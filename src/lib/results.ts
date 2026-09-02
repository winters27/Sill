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

import type { RankedCommand } from "$lib/exthost/commands";

/** The id of the result list, which the search field points at. */
export const LISTBOX = "sill-results";

/** The id of one result row, which is what `aria-activedescendant` names. */
export function optionId(index: number): string {
  return `sill-result-${index}`;
}

/**
 * Whether the search field is filtering a list that is on screen and walkable.
 *
 * Only then is it a combobox. The same field is a plain one everywhere else:
 * in the modes that show something other than the root list there is no
 * listbox to point at, and naming a row that is not rendered leaves a screen
 * reader announcing nothing at all, which is the state this was meant to fix.
 * Naming a collection or an alias is typing a name rather than filtering, so
 * there is nothing to arrow through either.
 */
// Which modes those are lives in `modes.ts`, with everything else that is
// decided per mode. It was four hand-written lists in three files, and they
// had already drifted.
export { isListMode } from "./modes";
import { isListMode } from "./modes";

export function isBrowsing(mode: string, results: number): boolean {
  return isListMode(mode) && results > 0;
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
