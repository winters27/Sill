/**
 * What the launcher is showing, and what each of those things behaves like.
 *
 * ## Why this exists
 *
 * The window is one page with sixteen faces. Which of them draws the ordinary
 * result list, which counts its own rows, which answers Escape itself and
 * which has an action panel were four separate hand-written lists, in three
 * files, each maintained by whoever last added a face and remembered.
 *
 * They had already drifted. `output` was in neither the list of views that
 * draw themselves nor the list of views that are ordinary lists, so the script
 * output block rendered **and the stale result list rendered underneath it**,
 * with the arrow keys dead because the count for that mode was zero.
 *
 * This is that guarantee written once. A mode with no entry here does not
 * compile, which is the only version of it that does not rely on somebody
 * remembering.
 *
 * ## Why the fields are not one boolean
 *
 * "Is this a list?" was doing two unrelated jobs: deciding what the arrow keys
 * walk, and deciding whether a change to the query re-runs the index search.
 * The clipboard and the conversation list are the first without being the
 * second, because their filter is a substring test over rows already in hand.
 * The count in `+page.svelte` carried a comment complaining about exactly
 * that. Splitting them is what the comment was asking for.
 */

/** Every face the launcher has. */
export const MODES = [
  "root",
  "command",
  "clipboard",
  "argument",
  "switcher",
  "collection",
  "alias",
  "emoji",
  "appVolume",
  "processes",
  "widgets",
  "namingWorkspace",
  "output",
  "store",
  "ai",
  "conversations",
  "destination",
] as const;

export type Mode = (typeof MODES)[number];

/** Where the number of rows to arrow through comes from. */
export type Rows =
  /** The ranked results the index search returned. */
  | "commands"
  /** The view keeps its own count: it filters rows it already holds. */
  | "own"
  /** An extension's rendered tree. */
  | "items"
  /** The field is a name rather than a filter, so there is nothing to walk. */
  | "none";

/** What a mode puts on screen. */
export type Shows =
  /** A view of its own, instead of the result list. */
  | "own"
  /** The ordinary result list. */
  | "results"
  /**
   * Nothing of its own, with the result list left showing underneath.
   *
   * On purpose: the field holds a name for the row being named, and the row
   * has to stay visible while it is typed.
   */
  | "behind";

export interface Behaviour {
  rows: Rows;
  /**
   * Whether a change to the query re-runs the search against the index.
   *
   * Separate from `rows` on purpose. A mode can be a list of rows and not want
   * this: the clipboard and the conversation list narrow what they already
   * have, and re-running the index search for them would answer a question
   * nobody asked.
   */
  searches: boolean;
  /**
   * What is on screen.
   *
   * Three answers rather than a boolean, because there really are three. The
   * third is the one that kept getting lost: a mode can draw nothing itself
   * and deliberately leave the result list showing, which is what naming a row
   * does, and "draws nothing" and "leaves the list up on purpose" look
   * identical from a boolean.
   */
  shows: Shows;
  /** Whether Escape means something here before it means "go back". */
  escape: boolean;
  /** Whether the action panel takes its actions from the selected row. */
  actions: boolean;
}

const behaviour: Record<Mode, Behaviour> = {
  root: { rows: "commands", searches: true, shows: "results", escape: false, actions: true },
  /**
   * Window management on the row under the cursor.
   *
   * The switcher had no action panel at all, which is the one view where the
   * whole point is a window you have already picked out: Ctrl+K now offers the
   * twenty-odd things the registry can do to it, halves and thirds and close
   * among them. Enter still means "switch to it", because the panel's primary
   * goes back through `openSelected`, which is where the switcher's own
   * dismissal lives.
   */
  switcher: { rows: "commands", searches: true, shows: "results", escape: true, actions: true },
  // Copy as well as paste, which the registry already offers on an emoji and
  // the picker had no way to reach.
  emoji: { rows: "commands", searches: true, shows: "results", escape: false, actions: true },
  appVolume: { rows: "commands", searches: true, shows: "results", escape: false, actions: true },
  /**
   * What is running, with the action panel on the row under the cursor.
   *
   * The panel is the whole point of this view rather than a nicety. Enter asks
   * a program to close, and the one action that ends it without asking lives
   * behind Ctrl+K, below the one that does. Taking the panel away here would
   * leave no way to reach it at all.
   */
  processes: { rows: "commands", searches: true, shows: "results", escape: false, actions: true },
  /**
   * Deliberately no action panel.
   *
   * The rows are folders, so the registry would offer everything it offers a
   * folder: reveal it, compress it, put it in the recycle bin. This view was
   * opened to answer one question, "which folder", and offering to delete one
   * of the answers is not a feature.
   */
  destination: { rows: "commands", searches: true, shows: "results", escape: false, actions: false },

  // Filters rows already in hand, so it counts its own and does not re-search.
  clipboard: { rows: "own", searches: false, shows: "own", escape: true, actions: false },
  /**
   * Actions on the conversation under the cursor.
   *
   * Ctrl+K here said "no actions here", and Delete was the only way to remove
   * one, wired straight into the window. An action only the page can reach is
   * one a hotkey cannot bind and the model cannot run.
   */
  conversations: { rows: "own", searches: false, shows: "own", escape: true, actions: true },
  /**
   * The extension store, with actions on the listing under the cursor.
   *
   * The query does go to Rust, but it answers with a page already narrowed and
   * capped, so what is arrowed through is whatever came back and the count is
   * the view's own.
   *
   * Ctrl+K here said "no actions here" on a shelf of code somebody is deciding
   * whether to run, and removing one was a chord wired straight into the page.
   * Enter still installs, because installing is two screens and the second one
   * is what says what the code appears to be able to do.
   */
  store: { rows: "own", searches: false, shows: "own", escape: true, actions: true },

  // An extension's own tree.
  command: { rows: "items", searches: false, shows: "own", escape: true, actions: false },
  argument: { rows: "items", searches: false, shows: "own", escape: false, actions: false },

  // The field is a name being typed, and the list underneath is what is being
  // named, kept on screen on purpose.
  alias: { rows: "none", searches: false, shows: "behind", escape: true, actions: false },
  collection: { rows: "none", searches: false, shows: "own", escape: true, actions: false },
  namingWorkspace: { rows: "none", searches: false, shows: "behind", escape: false, actions: false },

  widgets: { rows: "items", searches: false, shows: "own", escape: false, actions: false },
  ai: { rows: "items", searches: false, shows: "own", escape: false, actions: false },

  /**
   * What a script printed.
   *
   * `own` because it draws the output block. It was in neither list before, so
   * the block rendered and the previous result list rendered under it, with
   * dead arrow keys. That is the bug this whole table exists to make
   * impossible.
   */
  output: { rows: "items", searches: false, shows: "own", escape: false, actions: false },
};

export function behaviourOf(mode: string): Behaviour | undefined {
  return behaviour[mode as Mode];
}

/**
 * Whether this mode walks the ranked results.
 *
 * Kept as its own function because it is asked in three places, including the
 * combobox wiring that decides whether a screen reader announces the
 * highlighted row.
 */
export function isListMode(mode: string): boolean {
  return behaviourOf(mode)?.rows === "commands";
}

/** Whether a change to the query should re-run the index search. */
export function searchesOnType(mode: string): boolean {
  return behaviourOf(mode)?.searches ?? false;
}

/** Whether this draws its own view rather than the ordinary result list. */
export function drawsItsOwn(mode: string): boolean {
  return behaviourOf(mode)?.shows === "own";
}

/** Whether Escape means something here before it means "go back". */
export function handlesItsOwnEscape(mode: string): boolean {
  return behaviourOf(mode)?.escape ?? false;
}

/** Whether the action panel takes its actions from the selected row. */
export function hasRowActions(mode: string): boolean {
  return behaviourOf(mode)?.actions ?? false;
}

/**
 * Whether what is on screen is a listbox somebody can arrow through.
 *
 * The mode alone cannot answer it. Five components draw a `role="listbox"`
 * between them, and one of those is an extension's own tree: the same
 * `command` mode is a list, a grid, a form or a page of prose depending on
 * what the extension rendered, and only the first two are something to arrow
 * through. `tree` is that tag, and it is ignored for every mode whose rows do
 * not come from an extension.
 *
 * This is the question the search field asks before it calls itself a
 * combobox. It used to be `rows === "commands"`, which is the root list and
 * nothing else, so the clipboard, the store, the conversation list and every
 * extension list left a screen reader silent while somebody walked them.
 */
export function showsAListbox(mode: string, tree?: string): boolean {
  switch (behaviourOf(mode)?.rows) {
    // The root list, under whichever mode is filling it.
    case "commands":
      return true;

    // A view that counts its own rows: the clipboard, the store, the
    // conversation list. Each draws a listbox of its own.
    case "own":
      return true;

    // An extension's tree, which is a listbox only when it rendered one.
    case "items":
      return tree === "List" || tree === "Grid";

    // The field holds a name rather than a filter, or the mode is not one.
    default:
      return false;
  }
}
