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
