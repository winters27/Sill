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

/** The id of the result list, which the search field points at. */
export const LISTBOX = "sill-results";

/** The id of one result row, which is what `aria-activedescendant` names. */
export function optionId(index: number): string {
  return `sill-result-${index}`;
}
