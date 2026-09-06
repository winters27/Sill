/**
 * The extension store, from the window's side.
 *
 * Types mirrored from `src-tauri/src/store/`, and nothing more. **No filtering,
 * ranking or counting happens here**: a browse sends a query and receives rows
 * that are already narrowed, ordered, capped and joined against what is
 * installed. The catalogue itself is three thousand listings and two megabytes,
 * and the point of the split is that this side never sees it.
 */
import { invoke } from "@tauri-apps/api/core";
import { orElse } from "$lib/status";

/** One command an extension contributes, as the store describes it. */
export interface StoreCommand {
  name: string;
  title: string;
  description: string;
  /** `view`, `no-view` or `menu-bar`, as its manifest wrote it. */
  mode: string;
  /** Whether Sill has any way to run it. Decided in Rust, never here. */
  runnable: boolean;
}

/** What is known about an extension that is already installed. */
export interface InstalledState {
  revision: string;
  /** `store` or `folder`. */
  source: string;
  outdated: boolean;
}

/** One extension, ready to draw. */
export interface StoreRow {
  name: string;
  folder: string;
  title: string;
  description: string;
  author: string;
  categories: string[];
  platforms: string[];
  downloads: number;
  revision: string;
  /** Where the extension's icon is, or empty when it has none. */
  icon: string;
  commands: StoreCommand[];
  installed: InstalledState | null;
  /** Why it is not offered by default, when it is not. */
  blocked: string | null;
  /** Empty when there is nowhere to send somebody to read the source. */
  sourceUrl: string;
  /**
   * Here because it is installed rather than because the index carries it.
   *
   * An extension built from a folder, or one the store has withdrawn since it
   * was installed. Neither has a download count, an author or a description to
   * draw, so the row says where it came from instead of saying "0 installs"
   * about something nobody could have installed from a store it is not in.
   */
  native: boolean;
}

export interface StoreCategory {
  name: string;
  count: number;
}

export interface Browse {
  rows: StoreRow[];
  categories: StoreCategory[];
  matched: number;
  total: number;
  hidden: number;
  updates: number;
  /** Seconds since the epoch. */
  fetchedAt: number;
}

export interface StoreQuery {
  text: string;
  category: string | null;
  installedOnly: boolean;
  updatesOnly: boolean;
  hideBlocked: boolean;
}

/** One thing the code appears to be able to do. */
export interface Reached {
  id: string;
  title: string;
  detail: string;
  seenIn: string[];
  /** Whether it goes through Sill's own seam or straight to Node. */
  mediated: boolean;
}

/** What was fetched, and what it appears to do, before anybody agrees to it. */
export interface Preparation {
  name: string;
  title: string;
  revision: string;
  folder: string;
  icon: string;
  sourceUrl: string;
  files: number;
  bytes: number;
  commands: StoreCommand[];
  capabilities: Reached[];
  packages: string[];
  secrets: string[];
  /** Said when it asks for a newer `@raycast/api` than Sill implements. */
  apiWarning: string | null;
  /** The commands Sill will refuse to install, one sentence each. */
  refused: string[];
  notEnforced: string;
}

export interface Done {
  extension: string;
  title: string;
  commands: string[];
  /** The commands that were refused, one sentence each. */
  refused: string[];
  revision: string;
}

/**
 * How far along an install is, as Rust says it.
 *
 * npm and esbuild are the whole of the wait, and neither said anything until
 * it finished. Their own output is the content worth showing: npm names the
 * package it is fetching, esbuild names the file it is on.
 */
export type InstallProgress =
  | { stage: "fetching"; done: number; total: number }
  | { stage: "dependencies"; said: string }
  | { stage: "building"; command: string; done: number; total: number }
  | { stage: "bundling"; said: string };

/** The event Rust emits it on. Spelled once, here. */
export const INSTALL_PROGRESS = "store:install";

/** One line for the window, or nothing when there is nothing worth saying. */
export function progressLine(progress: InstallProgress): string {
  if (progress.stage === "fetching") {
    return progress.total === 0
      ? "Fetching"
      : `Fetching ${progress.done} of ${progress.total} files`;
  }

  if (progress.stage === "building") {
    return `Building ${progress.command} (${progress.done} of ${progress.total})`;
  }

  // npm and esbuild both print blank lines and progress bars. A line of
  // punctuation is worse than the word that was already there.
  const said = progress.said.trim();
  if (said.length < 3) return "";

  return progress.stage === "dependencies" ? `Dependencies: ${said}` : said;
}

/** Where one installed extension came from. */
export interface Pin {
  extension: string;
  source: string;
  revision: string;
  path: string;
  installedAt: number;
}

/**
 * One screen of the store.
 *
 * `refresh` is the only thing here that ever reaches the network on purpose.
 * Everything else answers from the catalogue already held, which is what makes
 * typing in the store cost nothing.
 */
export function storeBrowse(query: StoreQuery, refresh = false): Promise<Browse> {
  return invoke<Browse>("store_browse", { query, refresh });
}

/** Lets go of the catalogue. Called when the store is left, always. */
/** The screenshots the store shows for one extension; none when it has none. */
export function storeGallery(name: string): Promise<string[]> {
  return invoke<unknown>("store_gallery", { name })
    .then((got) => (Array.isArray(got) ? got.filter((one): one is string => typeof one === "string") : []))
    .catch(orElse("launcher", `the pictures for ${name}`, [], "extensions"));
}

export function storeClose(): Promise<void> {
  return invoke("store_close");
}

/** Step one: fetch and read. Installs nothing and runs nothing. */
export function storePrepare(name: string): Promise<Preparation> {
  return invoke<Preparation>("store_prepare", { name });
}

/** Step two: install what was prepared. Also what an update is. */
export function storeInstall(name: string): Promise<Done> {
  return invoke<Done>("store_install", { name });
}

/** Throws away a prepared install nobody accepted. */
export function storeDiscard(): Promise<void> {
  return invoke("store_discard");
}

/**
 * Removes an installed extension, and answers with what was done.
 *
 * The message rather than a boolean, because Rust runs this through the action
 * registry now and the registry answers in sentences: an extension that was
 * already gone is the end state somebody asked for rather than a failure, and
 * the sentence is what says which of the two happened.
 */
export function storeUninstall(extension: string): Promise<string> {
  return invoke<string>("store_uninstall", { extension });
}

export function storePins(): Promise<Pin[]> {
  return invoke<Pin[]>("store_pins");
}

/** Whether this machine has the Node an extension needs to run at all. */
export function storeReady(): Promise<boolean> {
  return invoke<boolean>("store_ready");
}


/** One command an installed extension contributes. */
export interface InstalledCommand {
  id: string;
  title: string;
  subtitle: string;
  mode: string;
  /** Whether Sill can run it. Decided in Rust. */
  runnable: boolean;
}

/** One permission, and whether this extension has it. */
export interface PermissionState {
  capability: string;
  /** What it lets the extension do, in the words the approval card uses. */
  plainly: string;
  granted: boolean;
}

/** Everything the settings screen needs about one installed extension. */
export interface InstalledExtension {
  extension: string;
  title: string;
  /** The picture beside its bundle, when the manifest named one. */
  icon: string | null;
  commands: InstalledCommand[];
  /** `store`, `folder`, or empty when nothing recorded it. */
  source: string;
  revision: string;
  path: string;
  installedAt: number;
  permissions: PermissionState[];
}

/**
 * What is installed, what it runs, and what it may reach.
 *
 * One call for all three. Reaches no network: the index is a file, the origins
 * are files beside the bundles, and the grants are already in memory.
 */
export function installedExtensions(): Promise<InstalledExtension[]> {
  return invoke<InstalledExtension[]>("installed_extensions");
}

/** One of an extension's commands that is loaded right now. */
export interface RunningCommand {
  session: string;
  extension: string;
  command: string;
  /** Bytes of memory it holds, or null when it did not answer in time. */
  heapBytes: number | null;
  /** The cap it would be stopped at, in bytes. */
  heapLimitBytes: number;
  /** How much of one processor core it used since the last reading. */
  corePercent: number;
  /** Whether it answered at all. A command stuck in a loop cannot. */
  answering: boolean;
}

/** What one extension has cost to open, and what it is holding now. */
export interface ExtensionCost {
  extension: string;
  /** Milliseconds to first screen when Sill had to start Node, if measured. */
  coldMs: number | null;
  coldOpens: number;
  /** The same when it did not have to, if measured. */
  warmMs: number | null;
  warmOpens: number;
  /**
   * The most one of its commands was holding when it was closed, in bytes.
   *
   * Read on the way out rather than sampled, which is what makes the panel a
   * comparison: a launcher has one command loaded at a time, so what is
   * running is a single number, and somebody hunting for the expensive
   * extension has closed the other three by the time they come to look.
   */
  heldBytes: number | null;
  running: RunningCommand[];
}

/**
 * What each extension costs, slowest to open first.
 *
 * Measured while Sill has been running and not kept between runs, so an
 * extension nobody has opened this time is simply absent. Nothing is started
 * to answer this: the live half is asked only of an extension runtime that is
 * already up.
 */
export function extensionResources(): Promise<ExtensionCost[]> {
  return invoke<ExtensionCost[]>("extension_resources");
}

/**
 * Gives one permission to one extension.
 *
 * The only way in for a permission needed at `require`: module loading is
 * synchronous, so there is no RPC to hang an approval card on and the
 * extension dies before anything can be asked.
 */
export function grantPermission(extension: string, capability: string): Promise<void> {
  return invoke("grant_extension_permission", { extension, capability });
}

/** Takes one back. The extension is asked again next time it tries. */
export function revokePermission(extension: string, capability: string): Promise<void> {
  return invoke("revoke_extension_grant", { extension, capability });
}

/** One setting an extension declares, and what it is currently set to. */
export interface ExtensionPreference {
  /** Which command it belongs to, or empty when the extension declares it. */
  command: string;
  commandTitle: string;
  name: string;
  /** `textfield`, `password`, `checkbox`, `dropdown`, or anything else. */
  kind: string;
  title: string;
  description: string;
  required: boolean;
  choices: { title: string; value: unknown }[];
  /**
   * What it answers with as things stand.
   *
   * For a `password` this is a boolean saying whether one is set, never the
   * value: a settings window that can display an API key is one somebody can
   * read over a shoulder.
   */
  value: unknown;
  /** Whether that came from the manifest rather than from somebody. */
  isDefault: boolean;
}

/** Everything one installed extension can be told, and what it has been. */
export function extensionPreferences(extension: string): Promise<ExtensionPreference[]> {
  return invoke<ExtensionPreference[]>("extension_preferences", { extension });
}

/** Sets one. An empty value puts the manifest's default back. */
export function setExtensionPreference(
  extension: string,
  command: string,
  name: string,
  value: unknown,
): Promise<void> {
  return invoke("set_extension_preference", { extension, command, name, value });
}

/**
 * A download count, shortened.
 *
 * Six figures of precision on "how many people installed this" is noise, and
 * the numbers reach the hundreds of thousands.
 */
export function installs(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}m`;
  if (count >= 10_000) return `${Math.round(count / 1000)}k`;
  if (count >= 1_000) return `${(count / 1000).toFixed(1)}k`;
  return String(count);
}

/** A byte count, shortened. */
export function weight(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} bytes`;
}

/** The first seven characters of a commit, which is how one is quoted. */
export function shortRevision(revision: string): string {
  return revision.slice(0, 7);
}

/**
 * How long ago something was, in words.
 *
 * Used for when the catalogue was fetched, which is the one thing that makes a
 * stale list honest rather than a list pretending to be current.
 */
export function ago(seconds: number, now = Date.now() / 1000): string {
  const passed = Math.max(0, Math.round(now - seconds));

  if (passed < 90) return "just now";
  if (passed < 3600) return `${Math.round(passed / 60)} minutes ago`;
  if (passed < 2 * 3600) return "an hour ago";
  if (passed < 86400) return `${Math.round(passed / 3600)} hours ago`;
  if (passed < 2 * 86400) return "yesterday";
  return `${Math.round(passed / 86400)} days ago`;
}

/**
 * How far along an install is, from nothing to one, or nothing at all.
 *
 * Only the two stages that count something can answer this. npm and esbuild
 * report lines rather than positions, and a bar that guessed at their share
 * would be a bar that moves without meaning: what those two get is the line
 * they wrote, which says more than an invented fraction would.
 *
 * The two halves are weighted rather than each drawn as a full sweep. Fetching
 * is the first part of one wait and building is the last, so a bar that filled,
 * emptied and filled again would read as two installs. Fetching takes the first
 * third because it is usually the shorter half, and npm sits between them with
 * nothing to report, which is why the bar holds rather than jumps.
 */
export function progressFraction(progress: InstallProgress): number | null {
  if (progress.stage === "fetching") {
    return progress.total === 0 ? 0 : (progress.done / progress.total) * FETCH_SHARE;
  }

  if (progress.stage === "building") {
    const built = progress.total === 0 ? 0 : progress.done / progress.total;
    return FETCH_SHARE + built * (1 - FETCH_SHARE);
  }

  return null;
}

/** How much of the bar the download is worth. */
const FETCH_SHARE = 0.34;
