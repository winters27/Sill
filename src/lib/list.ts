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

/**
 * What each kind of row is filed under.
 *
 * ## Why this is a table and not a switch
 *
 * It was a `switch` whose default returned "Applications", so a mode nobody
 * had thought about read as an application. Four did: a saved window
 * arrangement, a running process, a captured piece of text and a clipboard
 * entry were all filed under Applications, which is not true of any of them.
 *
 * That shape has cost this project a session more than once. A match over
 * modes with a default is a bug waiting: it makes forgetting silent, and the
 * thing forgotten looks like something else rather than looking wrong.
 *
 * So it is a table with no default, and `verify:source` refuses any `mode:`
 * that Rust can produce and this does not name. Adding a source without a
 * heading now fails the build instead of quietly reading "Applications".
 */
const HEADINGS: Record<string, string> = {
  // Applications, and the one mode the old default was actually right about.
  app: "Applications",

  answer: "Answer",
  /*
   * Where you were, rather than what exists.
   *
   * The only heading in the list that is about the past. It holds one row, it
   * sorts above everything because of the floor in the ranker, and it
   * disappears on its own within ten minutes.
   */
  conversation: "Continue",
  /* Every conversation, in the list that lets you reopen or forget one. */
  "past-conversation": "Conversations",
  /*
   * One extension as the store lists it.
   *
   * Never actually drawn here: the store keeps its own view and its own rows,
   * and a listing is not something the root search offers. It is named anyway
   * because the rule is about every mode Rust can produce having an answer,
   * and a mode with no heading that later does reach this list would be filed
   * under whichever extension the row happened to name.
   */
  "store-listing": "Extension Store",
  "sill-setting": "Sill Settings",
  /*
   * Terminal profiles and WSL distributions, under one heading.
   *
   * Not "Windows Terminal": on a machine without it, the rows are the WSL
   * distributions the registry knows about and Terminal has nothing to do
   * with them. What the two have in common is that pressing one opens a
   * terminal.
   */
  "terminal-profile": "Terminals",
  view: "Commands",
  "no-view": "Commands",
  builtin: "Sill",
  // Windows' own switches, under a heading that says so. Filed with Sill's
  // commands they read as Sill features, which is the opposite of true.
  system: "System Controls",
  /*
   * Named for whose settings they are.
   *
   * "Settings" beside "Sill Settings" reads as though one of them is the
   * general case and the other a special one, when they are simply two
   * different programs' settings. Saying which is which costs a word.
   */
  setting: "Windows Settings",
  file: "Files",
  /*
   * Folders, apart from files.
   *
   * Only a selection produces this today: a search returns a folder under
   * `file`, because there it is one answer among many to "where is the thing
   * called X" and splitting the list in two would put a heading over one row.
   * A selection is a list somebody made by hand, and telling them apart there
   * is the difference between "Open Terminal Here" being offered and not.
   */
  folder: "Folders",
  "file-setup": "Files",
  window: "Open Windows",
  /*
   * Saved and visited under one heading.
   *
   * They are the same kind of answer to the same question, and splitting them
   * puts two headings on a handful of rows. Which one a row is still shows: a
   * saved page ranks above a visited one of equal strength, so the ordering
   * carries it without a label.
   */
  url: "Browser",
  /*
   * Open tabs, under their own heading rather than with the pages.
   *
   * They answer a different question. "Browser" is somewhere you have been;
   * this is somewhere you are, in a window that is on this screen right now,
   * and choosing one takes you there rather than opening it again. Filed
   * together, the row that reopens a page and the row that goes to the copy
   * already open would be indistinguishable until you pressed Enter.
   */
  "browser-tab": "Open Tabs",
  websearch: "Web Search",
  emoji: "Emoji",
  "audio-session": "Playing Now",
  /*
   * What is playing, which is one row and never a list.
   *
   * A heading of its own rather than "Playing Now", which is the App Volume
   * list's. The two are different questions: one is every program making a
   * noise and how loud each is, this is the track and the three keys. They are
   * never on screen together, so the two headings never sit beside each other
   * to be confused, but a row filed under the other one would say the wrong
   * thing about what it is.
   */
  media: "Media",
  destination: "Folders",
  // What they are rather than who uses them: plenty of these ship with
  // Windows and have nothing to do with development.
  exe: "Command Line",

  // The four the default was silently wrong about.
  //
  // A saved arrangement of windows, a running process, a piece of text a
  // shortcut captured, and an entry from the clipboard history. None of them
  // is an application and every one of them used to say it was.
  workspace: "Arrangements",
  process: "Running",
  text: "Text",
  clipboard: "Clipboard History",
};

/**
 * A row's heading, or the extension it came from.
 *
 * The fallback is the extension's own title rather than a guess at a category,
 * because an extension command that named no mode Sill knows is still that
 * extension's, and saying so is true where "Applications" was not. A snippet
 * takes this path on purpose: Rust puts its collection in that field, because
 * both answer the same question.
 */
/**
 * One row per id, because two with the same id lose one of them.
 *
 * The list is keyed by `command.id`, and a repeated key does not throw and
 * does not draw twice: **it draws one row and says nothing**. Measured
 * against Svelte 5.56 in `RootList.svelte.test.ts`, on the first render and
 * on an update. So the failure is a result somebody searched for quietly
 * not being in the list, which is the kind of thing nobody reports because
 * it looks like the thing not existing.
 *
 * Every list on screen is built here by concatenating what Rust ranked with
 * files and browser pages that arrive later from another command, and Rust's
 * own `one_per_id` cannot see across that seam. This is the seam.
 *
 * The first of a repeated id wins, because the earlier list is the ranked
 * one and the later ones are appended below it deliberately. The original
 * array is handed back untouched when nothing repeated, which is every
 * keystroke on an ordinary machine.
 */
export function onePerId(rows: RankedCommand[]): RankedCommand[] {
  const seen = new Set<string>();
  const kept: RankedCommand[] = [];

  for (const row of rows) {
    if (seen.has(row.id)) continue;
    seen.add(row.id);
    kept.push(row);
  }

  return kept.length === rows.length ? rows : kept;
}

export function groupOf(command: RankedCommand): string {
  return HEADINGS[command.mode] ?? command.extensionTitle;
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
