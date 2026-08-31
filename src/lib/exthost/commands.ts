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
     * A web address a browser remembers. Never from the index either.
     *
     * Read out of a browser's own database when the query was typed, and gone
     * again afterwards, so like a window it is opened through the action
     * registry rather than launched by id.
     */
    | "url"
    /**
     * Words to look up on the web.
     *
     * Not an address yet. Which engine turns them into one is a setting read
     * when the row is chosen, so the window carries only what was typed.
     */
    | "websearch"
    /**
     * A switch belonging to Windows: the volume, the theme, the lock screen.
     *
     * Its own kind so it groups apart and wears Windows' own icon. A row that
     * changes the machine should not look like one of Sill's own commands.
     */
    | "system"
    /**
     * The row standing in for files that could not be searched.
     *
     * Not a thing to launch. Choosing it fixes what it names, and it only
     * exists while there is something to fix.
     */
    | "file-setup"
    /** One emoji. Its own corpus, reached through its own command. */
    | "emoji"
    /** One program's own volume, while it is playing something. */
    | "audio-session";
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
  /**
   * Whether this row is a switch, and which way it is set.
   *
   * Absent for everything that is not one, which is nearly everything. A row
   * that carries it draws as a control: pressing it flips the thing and leaves
   * the launcher where it is, so the state can be watched changing.
   */
  toggle?: boolean;
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
    /**
     * A web address a browser remembers. Never from the index either.
     *
     * Read out of a browser's own database when the query was typed, and gone
     * again afterwards, so like a window it is opened through the action
     * registry rather than launched by id.
     */
    | "url"
    /**
     * Words to look up on the web.
     *
     * Not an address yet. Which engine turns them into one is a setting read
     * when the row is chosen, so the window carries only what was typed.
     */
    | "websearch"
    /** One emoji. Its own corpus, reached through its own command. */
    | "emoji"
    /** One program's own volume, while it is playing something. */
    | "audio-session"
    /**
     * A switch belonging to Windows.
     *
     * Missing here for as long as these have existed, which made a Windows
     * switch a thing that could be shown and, as far as the types knew, never
     * launched. The window read that as "there is more to show" and put up an
     * extension screen with no extension in it, named after the switch.
     *
     * This union is a second copy of the one above it and they drifted. It is
     * still two lists, and the next thing added to one has to be added to the
     * other.
     */
    | "system";
  /** What the action said it did, in one line. */
  message: string;
  /**
   * Where a switch ended up, when the thing run was one.
   *
   * A row carrying this stays on screen showing its new state instead of the
   * launcher closing, so the thing being switched can be watched changing.
   */
  toggle?: boolean;
}

export function searchCommands(query: string): Promise<RankedCommand[]> {
  return invoke<RankedCommand[]>("search_commands", { query });
}

/**
 * Where the given switches are set, right now.
 *
 * Asked after one has been pressed, because pressing one can move another:
 * the audio outputs are a single choice spread across several rows. Answers in
 * the order it was asked, with `null` for anything that is not a switch.
 */
/**
 * Every program playing something right now, with its own volume.
 *
 * Its own call rather than part of the root search, because enumerating the
 * audio sessions costs about three milliseconds and the root list runs on
 * every keystroke whether or not anything about sound was typed.
 */
export function searchAppVolume(query: string): Promise<RankedCommand[]> {
  return invoke<RankedCommand[]>("search_app_volume", { query });
}

export function systemStates(ids: string[]): Promise<(boolean | null)[]> {
  return invoke<(boolean | null)[]>("system_states", { ids });
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
export type FileSearchMissing = "indexing" | "absent" | "asleep";

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

/** A drive that could be indexed. */
export interface Drive {
  root: string;
  label: string;
  kind: "fixed" | "removable" | "network" | "optical";
  indexed: boolean;
}

/** Every mounted drive, and whether Sill reads it. */
export function listDrives(): Promise<Drive[]> {
  return invoke<Drive[]>("list_drives").catch(() => []);
}

/**
 * Starts or stops indexing one folder.
 *
 * Answers with the folders indexed afterwards, so the caller does not have to
 * guess what its own change produced.
 */
export function indexFolder(path: string, wanted: boolean): Promise<string[]> {
  return invoke<string[]>("index_folder", { path, wanted });
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

/** A page a browser remembers. */
export interface BrowserHit {
  title: string;
  url: string;
  /** Which browser it came from, so two copies of a page are tellable apart. */
  browser: string;
  /** Saved rather than merely visited. */
  bookmark: boolean;
  visits: number;
  /** The program behind the browser it came from, for the row's icon. */
  icon: string | null;
}

/**
 * Pages a browser remembers, visited or saved.
 *
 * Behind a debounce like files, and for the same reason: it reads databases
 * that belong to running programs, and one of them on this machine is 31 MB.
 * Ranked in Rust, so these arrive in order and merge straight in.
 */
export function searchBrowsers(query: string): Promise<BrowserHit[]> {
  return invoke<BrowserHit[]>("search_browsers", { query }).catch(() => []);
}

/**
 * A page as a result row.
 *
 * Reuses the command row, exactly as files do, so selection, grouping and the
 * keyboard all work without knowing what a browser is.
 *
 * The address is the subtitle rather than the title because it is what tells
 * two pages of the same name apart, and the title is what somebody is typing
 * at. The browser it came from goes in the group heading, not the row: with
 * one browser installed it would be the same word on every line.
 */
export function browserAsCommand(hit: BrowserHit): RankedCommand {
  return {
    id: `browser:${hit.url}`,
    extension: "browser",
    extensionTitle: hit.bookmark ? "Bookmarks" : "History",
    title: hit.title,
    subtitle: hit.url,
    mode: "url",
    entrypoint: hit.url,
    // The browser it came out of, not Sill. A page from Edge and a page from
    // Zen are told apart at a glance, and neither is dressed as one of Sill's
    // own commands, which is the same rule the Windows switches follow.
    icon: hit.icon ?? undefined,
    matched: [],
  };
}

/**
 * Reads the words out of the last picture copied.
 *
 * Returns what happened, to be shown as it is: how many words were found, or
 * that the picture had none, or why it could not be read.
 */
export function extractTextFromLastImage(): Promise<string> {
  return invoke<string>("extract_text_from_last_image");
}

/**
 * The program that opens a web address on this machine.
 *
 * Asked once, on the way in, rather than per keystroke: the default browser
 * does not change while somebody is typing.
 */
export function defaultBrowser(): Promise<string | null> {
  return invoke<string | null>("default_browser").catch(() => null);
}

/**
 * The row that offers to look up what was typed.
 *
 * Built here rather than asked for, because asking Rust to compose one row per
 * keystroke is exactly the chatter rule 18 is about, and there is nothing to
 * decide until it is chosen: the address is not built until then.
 *
 * It carries the words, not a URL. Which engine turns them into one is a
 * setting, and it can change between this being offered and being picked.
 *
 * `browser` is the program the search will open in, and it is what the row
 * wears. Searching the web is not something Sill does; it is Sill handing the
 * question to that browser, and a row marked with Sill's own gear would say
 * otherwise. It is the same rule the Windows switches follow.
 */
export function webSearchRow(query: string, browser?: string): RankedCommand {
  return {
    id: "websearch:query",
    extension: "websearch",
    extensionTitle: "Web Search",
    title: `Search for ${query}`,
    subtitle: "",
    mode: "websearch",
    entrypoint: query,
    icon: browser,
    matched: [],
  };
}

export function dismiss(): Promise<void> {
  return invoke("dismiss");
}

/**
 * Summons the launcher, optionally with a command to run once it is up.
 *
 * For callers outside the launcher window, which today means the
 * notification-area menu. The launcher hears `sill://run` on arrival and
 * decides what the command looks like; the caller only states the intent.
 */
export function summonWith(command?: string): Promise<void> {
  return invoke("summon_with", { command: command ?? null });
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

/**
 * Renames a file or folder, keeping it where it is.
 *
 * Its own command rather than an action, because the action is handed an
 * object and acts: there is nowhere in that for a question, and renaming is
 * mostly the question.
 */
export function renamePath(path: string, to: string): Promise<string> {
  return invoke<string>("rename_path", { path, to });
}
