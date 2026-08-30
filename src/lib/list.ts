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
      return "System";
    case "setting":
      return "Settings";
    case "file":
    case "file-setup":
      return "Files";
    case "window":
      return "Open Windows";
    case "emoji":
      return "Emoji";
    case "exe":
      return "Developer";
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

  commands.forEach((command, index) => {
    const label = groupOf(command);
    let bucket = groups.get(label);
    if (!bucket) {
      bucket = [];
      groups.set(label, bucket);
      order.push(label);
    }
    bucket.push({ kind: "row", command, index });
  });

  if (order.length < 2) {
    return commands.map((command, index) => ({ kind: "row", command, index }));
  }

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
