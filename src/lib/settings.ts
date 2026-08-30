/**
 * Sill's own preferences, mirrored from `src-tauri/src/preferences.rs`.
 *
 * The two must stay in step. Rust is the authority: it fills in any field this
 * side omits, so a stale copy here loses a setting rather than corrupting one.
 */
import { invoke } from "@tauri-apps/api/core";
import type { DictationSettings } from "$lib/dictation";

export type Backdrop = "acrylic" | "blur" | "none";

/** Which face the interface is set in. */
export type InterfaceFont = "satoshi" | "inter" | "system";

export interface General {
  openAtLogin: boolean;
  showInTray: boolean;
}

/** Skin tone and what Enter does, for the emoji picker. */
export interface EmojiSettings {
  tone: "default" | "light" | "mediumLight" | "medium" | "mediumDark" | "dark";
  primary: "paste" | "copy";
}

/** Which keys move around the launcher. */
export interface NavigationSettings {
  preset: "standard" | "vim" | "emacs";
  /** Ctrl and a digit jumps straight to that row. */
  numeric: boolean;
  /** One chord replacing whatever a movement would otherwise have. */
  overrides: Partial<Record<Move, string>>;
}

/** A name the user chose for one thing in the index. */
export interface Alias {
  alias: string;
  command: string;
}

export interface Hotkey {
  summon: string;
  /** Opens straight onto the window list. Empty means off. */
  switcher: string;
  dismissOnBlur: boolean;
  selectQueryOnSummon: boolean;
  resetOnSummon: boolean;
}

export interface Appearance {
  backdrop: Backdrop;
  font: InterfaceFont;
  glassStrength: number;
  tintAlpha: number;
  visibleRows: number;
  windowWidth: number;
}

export interface Sources {
  shortcuts: boolean;
  packagedApps: boolean;
  appPaths: boolean;
  installedPrograms: boolean;
  pathExecutables: boolean;
  windowsSettings: boolean;
  excluded: string[];
  /** Individual entries switched off by id. */
  hidden: string[];
}

export interface FileSearch {
  enabled: boolean;
  maxResults: number;
  matchPath: boolean;
  matchCase: boolean;
  regex: boolean;
  /** A filter on results, not on what gets read. */
  onlyIn: string[];
  /** The folders Sill reads. Empty means the home folder. */
  roots: string[];
  /** Whether Sill keeps an index of its own at all. */
  index: boolean;
}

export interface ClipboardHistorySettings {
  enabled: boolean;
  /** Days an unpinned entry is kept. Zero keeps everything. */
  retainDays: number;
  keepImages: boolean;
  /** Applications whose copies are never recorded. */
  ignoredApps: string[];
  /** What to do with something that looks like a credential. */
  secrets: "skip" | "redact" | "keep";
}

export interface SnippetSettings {
  /** Watch typing and expand a keyword wherever it is typed. */
  expandKeywords: boolean;
}

/** Where a bound action gets the thing it acts on. */
export type BindingSource =
  | { from: "selection" }
  | { from: "clipboard" }
  | { from: "command"; id: string };

/** A key that runs an action without the launcher appearing. */
export interface Binding {
  /** An accelerator like "Ctrl+Alt+U". */
  accelerator: string;
  /** The action's stable id, which is why action ids are stable. */
  action: string;
  source: BindingSource;
  /** Put the result back over the selection rather than only copying it. */
  replace: boolean;
}

export interface Preferences {
  general: General;
  snippets: SnippetSettings;
  dictation: DictationSettings;
  clipboard: ClipboardHistorySettings;
  hotkey: Hotkey;
  appearance: Appearance;
  sources: Sources;
  files: FileSearch;
  bindings: Binding[];
  aliases: Alias[];
  navigation: NavigationSettings;
  emoji: EmojiSettings;
}

/**
 * Pushes the appearance preferences the page itself owns onto the document.
 *
 * The backdrop and the window size are Windows' business and Rust applies
 * them; glass strength is a CSS variable the page reads, so it has to be set
 * here or the setting saves and does nothing.
 */
export function applyAppearance(prefs: Preferences): void {
  const root = document.documentElement;
  root.style.setProperty("--glass-strength", String(prefs.appearance.glassStrength));
  // An attribute rather than a variable: the face is a whole block of tokens
  // (stack, features, display cut), and they have to change together.
  root.setAttribute("data-font", prefs.appearance.font);
}

export interface ExtensionInfo {
  id: string;
  title: string;
  commands: number;
}

export interface Diagnostics {
  version: string;
  dataDir: string;
  everythingRunning: boolean;
  indexedCommands: number;
  launchedEntries: number;
  extensions: ExtensionInfo[];
  bySource: { mode: string; count: number }[];
}

export function getDiagnostics(): Promise<Diagnostics> {
  return invoke<Diagnostics>("diagnostics");
}

export function rebuildIndex(): Promise<void> {
  return invoke("rebuild_index");
}

/**
 * One of Sill's own settings, from the catalogue Rust owns.
 *
 * The launcher searches the same list, so a setting added in one place is
 * findable in both.
 */
export interface SettingEntry {
  /** The panel it lives in, which is also its icon and its deep link. */
  panel: string;
  panelName: string;
  title: string;
  keywords: string;
}

export function listOwnSettings(): Promise<SettingEntry[]> {
  return invoke<SettingEntry[]>("list_own_settings");
}

export function openLog(): Promise<void> {
  return invoke("open_log");
}

export function openDataFolder(): Promise<void> {
  return invoke("open_data_folder");
}

export function clearUsageHistory(): Promise<void> {
  return invoke("clear_usage_history");
}

export function getPreferences(): Promise<Preferences> {
  return invoke<Preferences>("get_preferences");
}

export function setPreferences(prefs: Preferences): Promise<void> {
  return invoke("set_preferences", { prefs });
}

/** Opens settings, optionally jumping straight to one section. */
export function openSettings(section?: string): Promise<void> {
  return invoke("open_settings", { section: section ?? null });
}

export function quitApp(): Promise<void> {
  return invoke("quit_app");
}

/**
 * Turns a keyboard event into a Tauri accelerator.
 *
 * Returns null while only modifiers are held, so a recorder can show the keys
 * so far without committing to an accelerator that cannot be pressed.
 */
export function acceleratorFrom(event: KeyboardEvent): string | null {
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");

  const key = event.key;
  const isModifier = ["Control", "Alt", "Shift", "Meta", "OS"].includes(key);
  if (isModifier) return null;

  const named: Record<string, string> = {
    " ": "Space",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Escape: "Escape",
    Enter: "Enter",
    Tab: "Tab",
    Backspace: "Backspace",
  };

  parts.push(named[key] ?? (key.length === 1 ? key.toUpperCase() : key));

  // A bare letter is not a global hotkey; it would swallow that key system
  // wide. At least one modifier is required.
  if (parts.length < 2) return null;

  return parts.join("+");
}

/**
 * Accelerators another application already owns.
 *
 * Windows refuses a shortcut that is already taken and does not say by whom,
 * so the settings row can only report that the key is unavailable. Reported
 * at all, though: a shortcut that looks bound and does nothing is worse than
 * one that is visibly off.
 */
export function hotkeyConflicts(): Promise<string[]> {
  return invoke<string[]>("hotkey_conflicts").catch(() => []);
}

/**
 * Gives a command a name of the user's own, or takes one away.
 *
 * An empty alias removes it. Returns the whole preferences object, because
 * setting one is a write and every other window has to see the result.
 */
export function setAlias(command: string, alias: string): Promise<Preferences> {
  return invoke<Preferences>("set_alias", { command, alias });
}

/** One indexed thing, as the settings list shows it. */
export interface IndexRow {
  id: string;
  title: string;
  /** The index mode, which the window turns into a readable kind. */
  mode: string;
  icon: string | null;
  alias: string | null;
  /** The accelerator bound to opening this, if any. */
  hotkey: string | null;
  /** Switched off individually, so it never appears in the launcher. */
  hidden: boolean;
}

/** A page of the list, and how many matched in total. */
export interface IndexPage {
  rows: IndexRow[];
  total: number;
}

/**
 * Everything in the index, filtered and capped.
 *
 * The total comes back so the list can say "200 of 1,502" rather than quietly
 * showing the first two hundred as though that were all of them.
 */
export function indexRows(query: string, mode?: string): Promise<IndexPage> {
  return invoke<IndexPage>("index_rows", { query, mode }).catch(() => ({
    rows: [],
    total: 0,
  }));
}

/**
 * Binds a key to opening one indexed thing, or unbinds it.
 *
 * Writes an ordinary binding rather than a second kind of hotkey, so the
 * Shortcuts panel keeps showing it. One model, two ways in.
 */
export function setCommandHotkey(command: string, accelerator: string): Promise<Preferences> {
  return invoke<Preferences>("set_command_hotkey", { command, accelerator });
}

/** Switches one indexed entry off, or back on. */
export function setHidden(command: string, hidden: boolean): Promise<Preferences> {
  return invoke<Preferences>("set_hidden", { command, hidden });
}

/**
 * A key event as one chord string, for looking up in the navigation map.
 *
 * Separate from `acceleratorFrom`, which requires a modifier because a global
 * hotkey without one would swallow that key system wide. A navigation key has
 * no such problem: Down means Down, and only while the launcher has focus.
 */
export function chordFrom(event: KeyboardEvent): string | null {
  const key = event.key;
  if (["Control", "Alt", "Shift", "Meta", "OS"].includes(key)) return null;

  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");

  const named: Record<string, string> = {
    " ": "Space",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
  };

  parts.push(named[key] ?? (key.length === 1 ? key.toUpperCase() : key));
  return parts.join("+");
}

/** What a key does while moving around the launcher. */
export type Move =
  | "next"
  | "previous"
  | "pageDown"
  | "pageUp"
  | "first"
  | "last"
  | "sectionNext"
  | "sectionPrevious"
  | "open"
  | "actions"
  | "back";

/**
 * Every chord that moves around the launcher, and what it means.
 *
 * Resolved in Rust so this and the settings screen cannot hold two opinions
 * about what Ctrl+N does.
 */
export function navigationChords(): Promise<Record<string, Move>> {
  return invoke<Record<string, Move>>("navigation_chords").catch(() => ({}));
}

/** One movement, as a settings row shows it. */
export interface NavigationKey {
  id: Move;
  title: string;
  /** What actually happens, not what was preferred. */
  chord: string;
  overridden: boolean;
}

export function navigationKeys(): Promise<NavigationKey[]> {
  return invoke<NavigationKey[]>("navigation_keys").catch(() => []);
}

/** One skin tone, shown as a hand rather than named. */
export interface ToneChoice {
  id: EmojiSettings["tone"];
  swatch: string;
}

/**
 * The skin tones, each with the hand that shows it.
 *
 * From Rust because the swatch is the emoji itself and the set is a fact about
 * Unicode. Naming them in words is both awkward and less clear than the thing:
 * nobody picks "medium-light" off a list, they pick the one that looks right.
 */
export function emojiTones(): Promise<ToneChoice[]> {
  return invoke<ToneChoice[]>("emoji_tones").catch(() => []);
}
