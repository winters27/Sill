/**
 * The arithmetic behind the result list.
 *
 * Pulled out of the component because it is the part that can be wrong without
 * looking wrong. A list that renders a screen of nothing is arithmetic, not
 * markup, and arithmetic sitting inside a `.svelte` file is arithmetic nobody
 * can write a test for.
 */
import type { RankedCommand } from "$lib/exthost/commands";

/** One drawn line: a group heading, or a result. */
export type Line =
  | { kind: "header"; label: string }
  | { kind: "row"; command: RankedCommand; index: number };

/** What each kind of row is filed under. */
export function groupOf(command: RankedCommand): string {
  switch (command.mode) {
    case "answer":
      return "Answer";
    case "snippet":
      return "Snippets";
    case "sill-setting":
      return "Sill Settings";
    case "view":
    case "no-view":
      return "Commands";
    case "builtin":
      return "Sill";
    // Windows' own switches, under a heading that says so. Filed with Sill's
    // commands they read as Sill features, which is the opposite of true.
    case "system":
      return "System Controls";
    /*
     * Named for whose settings they are.
     *
     * "Settings" beside "Sill Settings" reads as though one of them is the
     * general case and the other a special one, when they are simply two
     * different programs' settings. Saying which is which costs a word.
     */
    case "setting":
      return "Windows Settings";
    case "file":
    case "file-setup":
      return "Files";
    case "window":
      return "Open Windows";
    /*
     * Saved and visited under one heading.
     *
     * They are the same kind of answer to the same question, and splitting
     * them puts two headings on a handful of rows. Which one a row is still
     * shows: a saved page ranks above a visited one of equal strength, so the
     * ordering carries it without a label.
     */
    case "url":
      return "Browser";
    case "websearch":
      return "Web Search";
    case "emoji":
      return "Emoji";
    // What they are rather than who uses them: plenty of these ship with
    // Windows and have nothing to do with development.
    case "exe":
      return "Command Line";
    default:
      return "Applications";
  }
}

/**
 * The list as drawn: group labels interleaved with their rows.
 *
 * Groups are ordered by their best-scoring member, not alphabetically, so the
 * ranker still decides what is seen first. Grouping that fought the ranking
 * would put the answer below a heading nobody was looking at.
 *
 * One group is not a grouping: a lone heading over the whole list is noise
 * rather than structure, so it is left off.
 */
export function linesOf(commands: RankedCommand[]): Line[] {
  const order: string[] = [];
  const groups = new Map<string, Line[]>();
  const rows: Line[] = [];
  const seen = new Set<string>();

  commands.forEach((command, index) => {
    /*
     * A repeated id is drawn once, because the alternative is drawing nothing.
     *
     * The rows are rendered by a keyed loop, and a repeated key is a hard error
     * there rather than a duplicated row: the whole block throws and the list
     * comes up empty. Four Windows settings shared one id, and the launcher
     * opened on a blank list until somebody thought to look for that.
     *
     * That is fixed at the source, and a test holds it fixed. This is here
     * because the consequence is out of all proportion to the cause: every
     * source of results feeds this one function, and none of them should be
     * able to blank the screen by getting an id wrong. Losing the second copy
     * of a row is a thing a person can miss. Losing all of them is not.
     */
    if (seen.has(command.id)) return;
    seen.add(command.id);

    const row: Line = { kind: "row", command, index };
    rows.push(row);

    const label = groupOf(command);
    let bucket = groups.get(label);
    if (!bucket) {
      bucket = [];
      groups.set(label, bucket);
      order.push(label);
    }
    bucket.push(row);
  });

  // One group is not a grouping, so it goes unlabelled.
  if (order.length < 2) return rows;

  return order.flatMap((label) => [
    { kind: "header", label } as Line,
    ...(groups.get(label) ?? []),
  ]);
}

/**
 * Where to put the scroll so a row is fully in view.
 *
 * Takes measurements rather than making them: every number here comes from the
 * element, so nothing can disagree with what the browser actually laid out.
 * That is the whole point. The list used to work out row positions from an
 * assumed row height, and twice the assumption stopped matching reality and
 * the difference came out as a screen of blank space.
 *
 * At the ends the container is taken all the way, so a group heading above the
 * first row and the padding below the last one both stay visible. Anywhere
 * else the row is nudged just far enough to clear the edge, because moving
 * more than that makes arrowing feel like the list is jumping.
 */
export function scrollFor(at: {
  /** Where the container is scrolled now. */
  scrollTop: number;
  /** The container's visible height. */
  viewport: number;
  /** How far the container can scroll, which the browser already knows. */
  scrollHeight: number;
  /** The row's top, relative to the top of the scrollable content. */
  rowTop: number;
  rowHeight: number;
  /** Clearance to leave between the row and the edge it is nearest. */
  gap: number;
  first: boolean;
  last: boolean;
}): number {
  if (at.first) return 0;
  // Past the end on purpose: the browser clamps it, and it knows about the
  // padding below the rows in a way nothing here should have to.
  if (at.last) return at.scrollHeight;

  const above = at.rowTop - at.gap;
  const below = at.rowTop + at.rowHeight + at.gap - at.viewport;

  if (at.scrollTop > above) return Math.max(0, above);
  if (at.scrollTop < below) return below;

  // Already in view. Not moving is the right answer more often than not.
  return at.scrollTop;
}
