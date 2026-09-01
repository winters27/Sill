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
  sourceUrl: string;
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
  notEnforced: string;
}

export interface Done {
  extension: string;
  title: string;
  commands: string[];
  revision: string;
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

export function storeUninstall(extension: string): Promise<boolean> {
  return invoke<boolean>("store_uninstall", { extension });
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
