/**
 * The launcher's own commands, as opposed to the extension API.
 *
 * Thin wrappers so components call typed functions rather than stringly-typed
 * `invoke` calls scattered through markup.
 */

import { invoke } from "@tauri-apps/api/core";

export interface RankedCommand {
  id: string;
  extension: string;
  extensionTitle: string;
  title: string;
  subtitle: string;
  /**
   * Where the entry came from.
   *
   * "app" is an installed application, "exe" a bare executable found on PATH,
   * and the other two are extension commands.
   */
  mode:
    | "view"
    | "no-view"
    | "app"
    | "exe"
    | "setting"
    | "file"
    | "builtin"
    | "answer"
    | "snippet"
    | "sill-setting"
    /** A quicklink that opens straight away. */
    | "quicklink"
    /** A quicklink with `{query}` in it, which takes over the field first. */
    | "quicklink-arg";
  entrypoint: string;
  /** A file to take an icon from, when it differs from the launch target. */
  icon?: string | null;
  /**
   * The settings panel this belongs to, for anything Sill owns.
   *
   * Set for Sill's own commands and for individual settings, so both arrive
   * in the launcher wearing the mark they wear in settings. Rust decides it,
   * because the answer is a fact about the command rather than a rendering
   * choice, and a copy of the mapping here would drift.
   */
  panel?: string | null;
  /** Indices into `title` that matched the query, for highlighting. */
  matched: number[];
}

export interface LaunchedCommand {
  session: string;
  title: string;
  extensionTitle: string;
  /** "no-view" runs and exits; "app" and "exe" are launched by the shell. */
  mode:
    | "view"
    | "no-view"
    | "app"
    | "exe"
    | "setting"
    | "file"
    | "builtin"
    | "answer"
    | "snippet"
    | "sill-setting"
    /** A quicklink that opens straight away. */
    | "quicklink"
    /** A quicklink with `{query}` in it, which takes over the field first. */
    | "quicklink-arg";
}

export function searchCommands(query: string): Promise<RankedCommand[]> {
  return invoke<RankedCommand[]>("search_commands", { query });
}

export function launchCommand(id: string): Promise<LaunchedCommand> {
  return invoke<LaunchedCommand>("launch_command", { id });
}

export function unloadExtension(session: string): Promise<boolean> {
  return invoke<boolean>("unload_extension", { session });
}

export function activateHandler(
  session: string,
  handler: string,
  args: unknown[] = [],
): Promise<unknown> {
  return invoke("activate_handler", { session, handler, args });
}

/** Runs an action Raycast implements itself, e.g. Action.CopyToClipboard. */
export function performBuiltin(tag: string, props: Record<string, unknown>): Promise<string> {
  return invoke<string>("perform_builtin", { tag, props });
}

/**
 * The icon for a launchable, as a data URI.
 *
 * Cached here as well as in Rust: a row re-renders on every keystroke while
 * filtering, and an await per row per frame would be a lot of IPC for an
 * answer that cannot change.
 */
const iconCache = new Map<string, Promise<string | null>>();

export function appIcon(path: string): Promise<string | null> {
  let pending = iconCache.get(path);
  if (!pending) {
    pending = invoke<string | null>("app_icon", { path }).catch(() => null);
    iconCache.set(path, pending);
  }
  return pending;
}

export interface FileHit {
  name: string;
  path: string;
  isDir: boolean;
}

export function searchFiles(query: string): Promise<FileHit[]> {
  return invoke<FileHit[]>("search_files", { query }).catch(() => []);
}

export function openPath(path: string): Promise<void> {
  return invoke("open_path", { path });
}

/**
 * Presents a file as a row.
 *
 * Files reuse the command row rather than getting a list of their own, so
 * selection, windowing and the keyboard all work unchanged. Everything has
 * already ranked them, so `score` is 0 and they simply follow the commands.
 */
export function fileAsCommand(hit: FileHit): RankedCommand {
  return {
    id: `file:${hit.path}`,
    extension: "file",
    extensionTitle: hit.isDir ? "Folder" : "File",
    title: hit.name,
    subtitle: hit.path,
    mode: "file",
    entrypoint: hit.path,
    icon: hit.path,
    matched: [],
  };
}

export function dismiss(): Promise<void> {
  return invoke("dismiss");
}

/** One thing that can be done to a result, as the registry describes it. */
export interface ActionInfo {
  id: string;
  title: string;
  /** What Enter does. Exactly one per kind. */
  primary: boolean;
}

/** How to reverse an action that said it could be reversed. */
export type UndoToken = { kind: "restoreClipboard"; text: string };

export interface ActionOutcome {
  message: string;
  undo?: UndoToken;
  session?: string;
}

/**
 * What can be done to a result of this kind.
 *
 * Asked by mode rather than by id, because the answer depends only on what
 * kind of thing it is, and a file result was never in an index to look up.
 */
export function actionsFor(mode: string): Promise<ActionInfo[]> {
  return invoke<ActionInfo[]>("actions_for", { mode });
}

/**
 * Runs one action against one result.
 *
 * The result's own fields go back as they arrived. Nothing here decides what
 * an action means or whether it applies; Rust owns both, and rejects a pairing
 * the window got wrong.
 */
export function runAction(action: string, command: RankedCommand): Promise<ActionOutcome> {
  return invoke<ActionOutcome>("run_action", {
    action,
    object: {
      id: command.id,
      mode: command.mode,
      target: command.entrypoint,
      title: command.title,
    },
  });
}

export function undoAction(undo: UndoToken): Promise<string> {
  return invoke<string>("undo_action", { undo });
}
