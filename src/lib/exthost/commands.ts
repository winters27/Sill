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
    | "quicklink-arg"
    /** A window that is open right now. Never from the index. */
    | "window"
    /**
     * The row standing in for files that could not be searched.
     *
     * Not a thing to launch. Choosing it fixes what it names, and it only
     * exists while there is something to fix.
     */
    | "file-setup"
    /** One emoji. Its own corpus, reached through its own command. */
    | "emoji";
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
  /**
   * Whether the query named this rather than merely fitting it.
   *
   * Absent on most rows. The root list merges more than one search, and this
   * is how it tells a result somebody typed the name of from one that only
   * happens to contain the same letters in the same order.
   */
  strong?: boolean;
  /** The name the user gave this, when they gave it one. */
  alias?: string;
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
    | "quicklink-arg"
    /** A window that is open right now. Never from the index. */
    | "window"
    /** One emoji. Its own corpus, reached through its own command. */
    | "emoji";
}

export function searchCommands(query: string): Promise<RankedCommand[]> {
  return invoke<RankedCommand[]>("search_commands", { query });
}

/**
 * Runs an indexed command, and tells Sill what was typed to reach it.
 *
 * The query is what makes an abbreviation learnable. Choosing Gmail after
 * typing `ggm` says something the id alone cannot: not that Gmail is popular,
 * but that `ggm` means Gmail.
 */
export function launchCommand(id: string, query?: string): Promise<LaunchedCommand> {
  return invoke<LaunchedCommand>("launch_command", { id, query });
}

/**
 * Counts a use of something the window opened by itself.
 *
 * The clipboard history becomes a view rather than a launch, and a quicklink
 * with a hole in it takes over the field. Neither reaches `launch_command`,
 * so without this neither is visible to ranking at all: `sill:clipboard` had
 * never been recorded once, however often it was opened.
 */
export function recordUse(
  id: string,
  query?: string,
  history = true,
): Promise<void> {
  return invoke<void>("record_use", { id, query, history }).catch(() => undefined);
}

/**
 * What was typed before, most recent first.
 *
 * Only queries that reached something. A shell recalls everything typed
 * including the typos; a launcher offering back the half-finished strings
 * somebody abandoned would mostly be offering them their mistakes.
 */
export function queryHistory(): Promise<string[]> {
  return invoke<string[]>("query_history").catch(() => []);
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

/** Why file search cannot answer, or nothing when it can. */
export type FileSearchMissing = "absent" | "asleep";

/**
 * What is standing between a typed query and a list of files.
 *
 * Asked on summon rather than per keystroke. The answer only changes when a
 * program starts or stops, which is not something typing does.
 */
export function fileSearchMissing(): Promise<FileSearchMissing | null> {
  return invoke<FileSearchMissing | null>("file_search_missing").catch(() => null);
}

/** Does whatever the thing standing in the way needs. */
export function startFileSearch(): Promise<string> {
  return invoke<string>("start_file_search");
}

export function searchFiles(query: string): Promise<FileHit[]> {
  return invoke<FileHit[]>("search_files", { query }).catch(() => []);
}

/**
 * The open windows matching a query.
 *
 * A third corpus beside the index and the filesystem, and the one with the
 * shortest life: it is enumerated fresh on every call, because a window list
 * is wrong the moment anything is opened, closed or renamed. Nothing here is
 * cached for the same reason.
 *
 * Ranked in Rust by the same function as everything else, so these arrive
 * already in order and merge straight into the list.
 */
/**
 * Emoji matching a query.
 *
 * Its own corpus rather than part of the index. Three thousand seven hundred
 * entries would nearly quadruple an index that is ranked on every keystroke,
 * so that typing "smile" could find an emoji as well as an application.
 */
export function searchEmoji(query: string, inline = false): Promise<RankedCommand[]> {
  return invoke<RankedCommand[]>("search_emoji", { query, inline }).catch(() => []);
}

export function searchWindows(query: string): Promise<RankedCommand[]> {
  return invoke<RankedCommand[]>("search_windows", { query }).catch(() => []);
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface MonitorInfo {
  index: number;
  full: Rect;
  work: Rect;
  primary: boolean;
}

export function listMonitors(): Promise<MonitorInfo[]> {
  return invoke<MonitorInfo[]>("list_monitors").catch(() => []);
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

/** The thing an action is being run against. */
export interface ActionTarget {
  id: string;
  /** Which kind of thing it is. Rust maps this to a kind and dispatches. */
  mode: string;
  /** What the action acts on: a path, a panel, a stored id, or the text. */
  target: string;
  title: string;
}

/**
 * Runs one action against one thing.
 *
 * Nothing here decides what an action means or whether it applies; Rust owns
 * both, and rejects a pairing the window got wrong.
 */
export function runAction(action: string, object: ActionTarget): Promise<ActionOutcome> {
  return invoke<ActionOutcome>("run_action", { action, object });
}

/** A search result, in the shape an action wants. */
export function asTarget(command: RankedCommand): ActionTarget {
  return {
    id: command.id,
    mode: command.mode,
    target: command.entrypoint,
    title: command.title,
  };
}

export function undoAction(undo: UndoToken): Promise<string> {
  return invoke<string>("undo_action", { undo });
}
