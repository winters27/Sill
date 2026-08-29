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

export interface Hotkey {
  summon: string;
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
}

export interface FileSearch {
  enabled: boolean;
  maxResults: number;
  matchPath: boolean;
  matchCase: boolean;
  regex: boolean;
  onlyIn: string[];
}

export interface ClipboardHistorySettings {
  enabled: boolean;
  /** Days an unpinned entry is kept. Zero keeps everything. */
  retainDays: number;
  keepImages: boolean;
  /** Applications whose copies are never recorded. */
  ignoredApps: string[];
}

export interface SnippetSettings {
  /** Watch typing and expand a keyword wherever it is typed. */
  expandKeywords: boolean;
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
