/**
 * Whether anything installed from the store is behind, as both windows read it.
 *
 * Rust owns the answer for the reason `update.ts` next door does: two windows
 * that each decide when to ask will disagree, and one of them will be wrong in
 * front of somebody. This module is the door.
 *
 * Nothing here decides anything. Which extensions are out of date, whether one
 * can be applied without asking, and when it is worth asking the store at all
 * are all in `src-tauri/src/store/updates.rs`. The window draws a row.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { silently } from "$lib/status";

/** One extension with something newer published. */
export interface Behind {
  /** The directory it is installed in, which is what actions name it by. */
  extension: string;
  /** The store's name for it. Not always the same string. */
  listing: string;
  author: string;
  folder: string;
  title: string;
  /** The revision installed here. */
  from: string;
  /** The revision the store publishes. */
  to: string;
}

/**
 * What the launcher is doing about it.
 *
 * A discriminated union mirroring the Rust enum exactly, so a `switch` can be
 * exhaustive and the compiler catches a state nobody drew. That is the shape
 * `RootList` had to be corrected into after eleven kinds fell through a
 * `default:`; this starts there.
 */
export type Doing =
  | { kind: "nothing" }
  | { kind: "checking" }
  | { kind: "applying"; title: string; done: number; total: number };

export interface Standing {
  behind: Behind[];
  doing: Doing;
  /** Titles that need somebody to look, because the new version reaches more. */
  asking: string[];
  /** Titles that did not work. */
  failed: string[];
  /** When the store was last asked, in seconds, or zero for never. */
  checkedAt: number;
}

/** What a window draws before Rust has answered, and if it never does. */
export const NOTHING_BEHIND: Standing = {
  behind: [],
  doing: { kind: "nothing" },
  asking: [],
  failed: [],
  checkedAt: 0,
};

/**
 * What came back, if it is the shape this claims to return.
 *
 * Validated rather than trusted, and the reason is not defensiveness about
 * Rust. **Tauri refuses a command missing from `capabilities/` silently**, and
 * a denied command *resolves* rather than throwing, so a `.catch` does not
 * help and neither does the type parameter. A launcher that read `.length` off
 * `undefined` here would take the whole root list down.
 */
export function asStanding(answer: unknown): Standing {
  const said = answer as Partial<Standing> | null | undefined;
  if (!said || !Array.isArray(said.behind)) return NOTHING_BEHIND;

  return {
    behind: said.behind,
    doing: typeof said.doing?.kind === "string" ? said.doing : { kind: "nothing" },
    asking: Array.isArray(said.asking) ? said.asking : [],
    failed: Array.isArray(said.failed) ? said.failed : [],
    checkedAt: said.checkedAt ?? 0,
  };
}

/**
 * What is behind right now, without asking the store anything.
 *
 * Reads a file Rust already has open, or a list it is already holding. Safe on
 * a window opening, which is where it is called.
 */
export function extensionUpdates(): Promise<Standing> {
  return invoke<Standing>("extension_updates")
    .then(asStanding)
    .catch(silently(NOTHING_BEHIND));
}

/**
 * Asks the store whether anything is behind.
 *
 * Rust decides whether that costs anything. Without `force` it returns having
 * done nothing unless six hours have passed, and does nothing at all on a
 * machine with nothing installed from the store, so the summon path can call
 * this every single time the window opens.
 *
 * `silently`, because the answer arrives through the event and a check that
 * could not reach the network is not something to put a launcher-wide trouble
 * on screen for.
 */
export function checkExtensionUpdates(force = false): Promise<void> {
  return invoke<void>("check_extension_updates", { force }).catch(silently(undefined));
}

/**
 * Applies the updates that ask for nothing new.
 *
 * `extension` names one, or nothing for all of them. Resolves as soon as the
 * work is handed off: npm and a bundler are tens of seconds per extension and
 * the launcher will be long gone. Watch the event for what happened.
 */
export function applyExtensionUpdates(extension?: string): Promise<void> {
  return invoke<void>("apply_extension_updates", {
    extension: extension ?? null,
  }).catch(silently(undefined));
}

/**
 * Calls back whenever the standing changes, and returns the way to stop.
 *
 * A window that forgets to unlisten keeps a handler alive over a component that
 * is gone, which in this codebase has meant a settings pane redrawing after it
 * was closed.
 */
export function whenExtensionUpdatesChange(
  run: (standing: Standing) => void,
): Promise<UnlistenFn> {
  return listen<Standing>("sill://extension-updates", (event) =>
    run(asStanding(event.payload)),
  );
}

/**
 * The sentence for what just happened, or nothing worth saying.
 *
 * Returns `null` for the states a launcher has no business interrupting
 * somebody about: nothing happening, and a check in flight. Applying is worth a
 * line because somebody pressed a key and is waiting to see that it took.
 *
 * Exhaustive over the union, so a state added later will not compile until it
 * has words here.
 *
 * Written here rather than in the component so it can be tested without
 * rendering anything.
 */
export function updatingWords(standing: Standing): string | null {
  switch (standing.doing.kind) {
    case "applying": {
      const { title, done, total } = standing.doing;
      return total === 1
        ? `Updating ${title}`
        : `Updating ${title}, ${done} of ${total}`;
    }
    case "checking":
    case "nothing":
      break;
  }

  // Said after the batch rather than during it, and only once. These are the
  // two outcomes somebody has to act on; the ones that simply worked are
  // reported by the row going away.
  if (standing.failed.length) {
    return `${said(standing.failed)} could not be updated`;
  }

  if (standing.asking.length) {
    return `${said(standing.asking)} asks for more than before, so it is waiting for you`;
  }

  return null;
}

/** A list of titles as a person would say it. */
function said(titles: string[]): string {
  if (titles.length === 1) return titles[0];
  if (titles.length === 2) return `${titles[0]} and ${titles[1]}`;
  return `${titles.slice(0, -1).join(", ")} and ${titles[titles.length - 1]}`;
}
