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
 * Puts a second search's results into the first, by how well each matched.
 *
 * Appending was wrong and measuring showed how wrong: typing "tada" matched
 * eighty-four things in the index, every one a coincidence of spelling, and
 * the emoji somebody had plainly named landed eighty-fifth. Groups are ordered
 * by their best member, so where the first one lands decides where the whole
 * group reads.
 *
 * So: above everything the index only half-recognised, below everything it
 * knew by name. Neither list is reordered within itself.
 */
export function merged(into: RankedCommand[], extra: RankedCommand[]): RankedCommand[] {
  let at = 0;
  while (at < into.length && into[at].strong) at += 1;

  return [...into.slice(0, at), ...extra, ...into.slice(at)];
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
/**
 * The modes that are a list of rows you arrow through and filter by typing.
 *
 * One list because this test was written out four times, and the fourth copy
 * is where a new mode gets forgotten: the arrow keys stop working, or the
 * search stops re-running, in one place and not the others. Two hand-kept
 * copies of the mode union have already drifted in this codebase.
 */
export const LIST_MODES = [
  "root",
  "switcher",
  "emoji",
  "appVolume",
  "destination",
] as const;

/** Whether this mode draws a list of rows at all. */
export function isListMode(mode: string): boolean {
  return (LIST_MODES as readonly string[]).includes(mode);
}

export function isBrowsing(mode: string, results: number): boolean {
  return isListMode(mode) && results > 0;
}
