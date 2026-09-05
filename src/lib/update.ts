/**
 * Whether there is a newer Sill, as the launcher and the settings window both
 * read it.
 *
 * Rust owns the answer, for the same reason `status.ts` does: two windows that
 * each decide for themselves when to ask will disagree about it, and one of
 * them will be wrong in front of somebody. This module is the door.
 *
 * Every call is wrapped the way the codebase requires. A window that cannot
 * reach Rust draws "unknown" rather than "up to date", because claiming to be
 * current when nothing was checked is exactly the untrue-interface failure the
 * status surface exists to prevent.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { orElse, silently, type Surface } from "$lib/status";

/**
 * Where the check has got to.
 *
 * A discriminated union rather than a bag of flags, mirroring the Rust enum
 * exactly, so that a `switch` over it can be made exhaustive and the compiler
 * catches a state nobody drew. `RootList` had to be corrected into this shape
 * after eleven kinds fell through a `default:`; this starts there.
 */
export type Progress =
  | { kind: "unknown" }
  | { kind: "upToDate" }
  | { kind: "available"; version: string; notes: string | null }
  | { kind: "downloading"; version: string; percent: number | null }
  | { kind: "ready"; version: string }
  | { kind: "failed"; why: string };

export interface UpdateState {
  progress: Progress;
  /** The running build. */
  current: string;
  /** Whether the answer is fresh enough that summoning will not re-ask. */
  checkedRecently: boolean;
}

/** What a window draws before Rust has answered, and if it never does. */
export const NOTHING_KNOWN: UpdateState = {
  progress: { kind: "unknown" },
  current: "",
  checkedRecently: false,
};

/**
 * The state, without asking for a new one.
 *
 * `orElse` rather than `silently`: a window that cannot read this draws the
 * About page's version as blank and its update row as unknown, which is a
 * pane saying something untrue about the application, and that is the exact
 * test `status.ts` sets for choosing between the two.
 */
export function updateState(surface: Surface): Promise<UpdateState> {
  return invoke<UpdateState>("update_state")
    .then(asState)
    .catch(
      orElse(surface, "Sill could not read whether there is an update", NOTHING_KNOWN, "about"),
    );
}

/**
 * What came back, if it is the shape this claims to return.
 *
 * `orElse` catches a rejection and nothing else, so a call that resolves to
 * something unexpected walks straight past it and the chin reads `.kind` off
 * `undefined`. That is not hypothetical: it took the launcher's whole footer
 * down under test the first time this shipped, because a command with no
 * answer resolves rather than throws.
 *
 * A denied command does the same thing in production. Tauri refuses a command
 * missing from `capabilities/` **silently**, which is how the tray menu once
 * shipped completely dead, so the shape coming back is not something to trust
 * on the strength of a type parameter.
 */
export function asState(answer: unknown): UpdateState {
  const said = answer as Partial<UpdateState> | null | undefined;
  if (!said || typeof said.progress?.kind !== "string") return NOTHING_KNOWN;
  return {
    progress: said.progress,
    current: said.current ?? "",
    checkedRecently: said.checkedRecently ?? false,
  };
}

/**
 * Asks whether there is a newer Sill.
 *
 * Rust decides whether that costs anything: without `force` it does nothing at
 * all unless a day has passed, so the summon path can call this every single
 * time the window opens without opening a socket every time.
 *
 * `silently` here, and only here. The answer arrives through
 * `sill://update-changed` and a failure becomes a `failed` progress that
 * settings shows in words. Reporting it twice would put a launcher-wide
 * trouble on screen for something the update row already says.
 */
export function checkForUpdate(force = false): Promise<void> {
  return invoke<void>("check_for_update", { force }).catch(silently(undefined));
}

/**
 * Downloads the newer Sill and runs its installer.
 *
 * Rejects with the reason, for a caller that wants to say so. The state is
 * announced either way, so a surface watching the event does not need to.
 */
export function installUpdate(): Promise<void> {
  return invoke<void>("install_update");
}

/**
 * Closes Sill and starts it again, for an update already installed.
 *
 * Never resolves when it works, because the process it would resolve into is
 * gone. Callers treat it as fire and forget.
 */
export function restartForUpdate(): Promise<void> {
  return invoke<void>("restart_for_update");
}

/**
 * Calls back whenever the state changes, and returns the way to stop.
 *
 * A window that forgets to unlisten keeps a handler alive over a component
 * that is gone, which in this codebase has meant a settings pane redrawing
 * after it was closed.
 */
export function whenUpdateChanges(run: (progress: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>("sill://update-changed", (event) => run(event.payload));
}

/**
 * The one line the chin shows, or nothing at all.
 *
 * Returns `null` for every state the launcher has no business interrupting
 * somebody about. That is most of them: being up to date is not news, a failed
 * check belongs in settings, and "unknown" is what the first second of every
 * launch looks like. The chin speaks only when there is something to press.
 *
 * Written here rather than in the component so it can be tested without
 * rendering anything, and so the settings window can use the same words.
 */
export function chinLine(progress: Progress): { words: string; button: string | null } | null {
  switch (progress.kind) {
    case "available":
      return {
        words: `Sill ${progress.version} is available`,
        button: "Update and restart",
      };
    case "downloading":
      return {
        words:
          progress.percent === null
            ? `Downloading Sill ${progress.version}`
            : `Downloading Sill ${progress.version}, ${progress.percent}%`,
        // No button while it is arriving. A second press would start a second
        // download, and there is nothing else useful to offer mid-flight.
        button: null,
      };
    case "ready":
      return { words: `Sill ${progress.version} is ready`, button: "Restart now" };
    case "unknown":
    case "upToDate":
    case "failed":
      return null;
  }
}

/**
 * The full sentence, for the settings row that has room for one.
 *
 * Says something for every state, which is the difference between this and
 * `chinLine`. Settings is where somebody went to ask, so "up to date" and a
 * failed check are both answers there, where in the chin they would be an
 * interruption with nothing to press.
 *
 * Exhaustive over the union: a state added later will not compile until it has
 * words here, which is the guard `RootList` gained after eleven kinds fell
 * through a `default:`.
 */
export function updateWords(progress: Progress): string {
  switch (progress.kind) {
    case "unknown":
      return "Not checked yet.";
    case "upToDate":
      return "This is the newest version.";
    case "available":
      return `Sill ${progress.version} is available.`;
    case "downloading":
      return progress.percent === null
        ? `Downloading Sill ${progress.version}.`
        : `Downloading Sill ${progress.version}, ${progress.percent}% done.`;
    case "ready":
      return `Sill ${progress.version} is downloaded and will finish installing on restart.`;
    case "failed":
      return `The last check did not work: ${progress.why}`;
  }
}
