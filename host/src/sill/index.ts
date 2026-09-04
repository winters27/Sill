/**
 * The module an extension gets when it requires `@sill/api`.
 *
 * Everything else the host hands out is Raycast's, reimplemented. This is
 * Sill's own, and it is deliberately tiny: three ideas, each of which is
 * something Sill already has a strong type for in Rust and the extension had
 * no way to see.
 *
 * - **Objects.** What the command was run on, when it was run on something.
 * - **Actions.** A command that declares `sill.actionOn` in its manifest
 *   becomes a row in the action panel of every object of those kinds, and
 *   [`actionTarget`] is what it was run on.
 * - **Capabilities.** What this extension has been allowed to reach, in the
 *   same words the install card used and Settings uses.
 *
 * ## Plain functions, never methods
 *
 * Nothing here is a class and nothing here is a method, on purpose. The
 * published `@raycast/utils` hands `cache.subscribe` to
 * `useSyncExternalStore` unbound, React calls it with no `this`, and every
 * extension using `useCachedState` died on its first render with an error that
 * named nothing of Sill's. An extension is free to write
 * `const { actionTarget } = require("@sill/api")` and pass it anywhere, and a
 * free function survives that by construction.
 *
 * ## What is not here
 *
 * No way to ask for a permission, and no way to reach anything the Raycast API
 * does not already reach. Everything below is a **reading** of what the worker
 * was already told at launch. The module gate in `worker/patch-require.ts` is
 * unaffected: `@sill/api` is not a Node built-in, so it is neither free nor
 * gated but an override, and what it hands back is four strings and a list of
 * words.
 */

import { getBridge } from "../api/bridge";

/**
 * What kind of thing an object is.
 *
 * The same words `ObjectKind` serialises as in Rust, and the same words a
 * manifest writes under `sill.actionOn`. `npm run verify:source` compares this
 * union to `ObjectKind::name` and fails in both directions, because a kind
 * spelled two ways is an action that is silently never offered.
 */
export type SillObjectKind =
  | "application"
  | "file"
  | "folder"
  | "extensionCommand"
  | "systemSetting"
  | "setting"
  | "builtin"
  | "systemControl"
  | "snippet"
  | "quicklink"
  | "terminalProfile"
  | "script"
  | "answer"
  | "clipboardEntry"
  | "text"
  | "emoji"
  | "window"
  | "browserTab"
  | "search"
  | "url"
  | "audioSession"
  | "nowPlaying"
  | "process"
  | "workspace"
  | "conversation"
  | "storeListing";

/**
 * One thing in Sill, and enough to act on it.
 *
 * Flat, because Sill's own `Object` is: every kind carries exactly one
 * meaningful string. `target` is the part to act on (a path for a file, a
 * window handle for a window, the text itself for a clipboard row) and `title`
 * is what to call it in front of somebody.
 */
export interface SillObject {
  kind: SillObjectKind;
  /** Stable identity, the same string Sill ranks and remembers it by. */
  id: string;
  /** What to act on: a path, a value, a handle. */
  target: string;
  /** What to call it. */
  title: string;
  /**
   * How Sill found it, when it came out of the index.
   *
   * Two modes can share a kind: `app` and `exe` are both an application, and
   * `quicklink` and `quicklink-arg` are both a saved link. Read `kind` unless
   * you specifically need the difference.
   */
  mode: string;
}

/**
 * What an extension can be allowed to reach.
 *
 * The same words Rust's `Capability` serialises as, so a name here is the name
 * on the install card and in Settings. `verify:source` holds the two together.
 */
export type SillCapability =
  | "clipboardRead"
  | "clipboardWrite"
  | "fileRead"
  | "fileWrite"
  | "processLaunch"
  | "inputInjection"
  | "network"
  | "ui"
  | "launcherDismiss"
  | "systemControl"
  | "shellExecution"
  | "selectionRead"
  | "windowControl";

/**
 * The thing this command was run on, or `undefined`.
 *
 * `undefined` is the ordinary case and is not an error: somebody picked the
 * command off the root list, so there is nothing it was run on. It is only
 * ever a value when the command was reached through the action panel of an
 * object whose kind the manifest named under `sill.actionOn`.
 *
 * So this is also how a command tells the two apart, and a command that can be
 * both should branch on it rather than assuming.
 */
export function actionTarget(): SillObject | undefined {
  // The bridge types `kind` as a bare string, because the bridge is where the
  // wire arrives and the wire carries whatever Rust sent. The narrowing
  // happens here, once, and `verify:source` is what makes it true: the union
  // above is checked against `ObjectKind::name` in both directions.
  return getBridge().on as SillObject | undefined;
}

/**
 * Everything this extension has been allowed to reach.
 *
 * Read at the moment it is asked rather than captured at launch, because
 * somebody can take a permission back in Settings while a command is running
 * and the answer has to change with them.
 *
 * This is a **reading, not a request**: nothing here grants anything, and an
 * absent capability is still refused by the gate whether or not the code
 * checked first. It exists so an extension can say "this needs the clipboard,
 * which you have not allowed" in its own words instead of throwing.
 */
export function capabilities(): SillCapability[] {
  return [...getBridge().capabilities] as SillCapability[];
}

/** Whether this extension holds one particular capability. */
export function holds(capability: SillCapability): boolean {
  return getBridge().capabilities.includes(capability);
}

/**
 * The version of this API the host implements.
 *
 * A single number rather than a semantic version, because there is one
 * publisher and the only question an extension can usefully ask is "is what I
 * was written against here". It goes up when something is added or changed.
 */
export const apiVersion = 1;
