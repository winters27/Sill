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

/** Where each line starts, and where the list ends. One entry longer than the list. */
export function offsetsOf(lines: Line[], rowHeight: number, headerHeight: number): number[] {
  const out = new Array<number>(lines.length + 1);
  let y = 0;

  for (let i = 0; i < lines.length; i++) {
    out[i] = y;
    y += lines[i].kind === "header" ? headerHeight : rowHeight;
  }

  out[lines.length] = y;
  return out;
}

/** Index of the last line starting at or before `y`. */
export function lineAt(offsets: number[], count: number, y: number): number {
  let low = 0;
  let high = count - 1;

  while (low < high) {
    const mid = (low + high + 1) >> 1;
    if (offsets[mid] <= y) low = mid;
    else high = mid - 1;
  }

  return Math.max(0, low);
}

/**
 * Which slice of the list to put in the DOM, and where it starts.
 *
 * `at` is where the list actually is rather than where it was last heard to
 * be. The remembered scroll position is only refreshed by a scroll event, and
 * the browser clamps the real one on its own whenever the content gets
 * shorter. Between the two, a search returning fewer results than the last one
 * sliced the list from a position past its end, and **every row rendered below
 * the viewport**: a screen of blank space that only came right if you scrolled
 * and provoked an event.
 */
export function windowOf(
  offsets: number[],
  count: number,
  scrollTop: number,
  height: number,
  overscan: number,
): { at: number; first: number; last: number } {
  const total = offsets[count] ?? 0;
  const at = Math.max(0, Math.min(scrollTop, total - height));

  return {
    at,
    first: Math.max(0, lineAt(offsets, count, at) - overscan),
    last: Math.min(count, lineAt(offsets, count, at + height) + 1 + overscan),
  };
}
