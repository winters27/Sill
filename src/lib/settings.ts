/**
 * Sill's own preferences, mirrored from `src-tauri/src/preferences.rs`.
 *
 * The two must stay in step. Rust is the authority: it fills in any field this
 * side omits, so a stale copy here loses a setting rather than corrupting one.
 */
import { invoke } from "@tauri-apps/api/core";
import { orElse, silently } from "$lib/status";
import type { DictationSettings } from "$lib/dictation";

export type Backdrop = "acrylic" | "blur" | "none";

/** Which face the interface is set in. */
export type InterfaceFont = "satoshi" | "inter" | "system";

export interface General {
  openAtLogin: boolean;
  showInTray: boolean;
  /**
   * Write the per-keystroke and per-summon lines to the log as well.
   *
   * For somebody chasing a fault. It only ever adds: nothing here can stop a
   * failure or a crash reaching the log, which is why there is no setting in
   * the other direction.
   */
  detailedLog: boolean;
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

/**
 * Which chord runs which action, where it differs from what it ships with.
 *
 * The sibling of `NavigationSettings`: that one is movement, this one is doing
 * something to what is selected. Only what was changed is held, so an action
 * whose default is later reconsidered gets the new one.
 */
export interface ActionKeySettings {
  /**
   * Action id to accelerator, in `chordFrom` spelling.
   *
   * An empty string means the action should have no key, which is how a
   * default is turned off rather than changed.
   */
  overrides: Record<string, string>;
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
  /** Picks an area of the screen without the launcher. Empty means off. */
  capture: string;
  /** Copies every screen at once. Empty means off. */
  captureScreen: string;
  dismissOnBlur: boolean;
  selectQueryOnSummon: boolean;
  resetOnSummon: boolean;
}

/**
 * Which palette the interface is drawn in.
 *
 * The ids are the `[data-theme]` values in `theme.css`, so a new theme is a
 * block there plus a variant here plus a variant in Rust, and nothing else.
 */
export type Theme =
  | "winters-glass"
  | "oilslick"
  | "graphite"
  | "ember"
  | "moss"
  | "aberration";

/**
 * Which screen the launcher comes up on.
 *
 * It used to be centred once at startup and never moved, so on two monitors
 * it always appeared on the primary one however far away that was.
 */
export type SummonOn = "cursor" | "activeWindow" | "primary";

export interface Appearance {
  backdrop: Backdrop;
  theme: Theme;
  chromaStrength: number;
  font: InterfaceFont;
  glassStrength: number;
  tintAlpha: number;
  visibleRows: number;
  windowWidth: number;
  summonOn: SummonOn;
}

export interface Sources {
  shortcuts: boolean;
  packagedApps: boolean;
  appPaths: boolean;
  installedPrograms: boolean;
  pathExecutables: boolean;
  windowsSettings: boolean;
  /** Installed games, read from the Steam and Epic libraries. */
  games: boolean;
  /** Extra folders walked exactly as the Start Menu is. */
  folders: string[];
  excluded: string[];
  /** Individual entries switched off by id. */
  hidden: string[];
}

/**
 * Reading what a browser remembers.
 *
 * Off by default. Nothing else Sill reads is as personal as a browsing
 * history, and helping itself to one because a browser happens to be installed
 * is not a decision Sill gets to make.
 */
export interface Browsers {
  enabled: boolean;
  /** Pages that were visited. */
  history: boolean;
  /** Pages that were saved, which is the smaller and more deliberate set. */
  bookmarks: boolean;
  maxResults: number;
  /** Tabs the running browsers have open, read when somebody types. */
  tabs: boolean;
  /**
   * Whether that includes Firefox and the browsers built on it.
   *
   * Its own switch because it is the one setting in Sill whose cost lands in
   * another program: a Firefox keeps its accessibility engine off until a
   * client asks, and reading tabs is the asking.
   */
  tabsFirefox: boolean;
}

/** The extension store. */
export interface StoreSettings {
  /**
   * Only offer extensions that say they run on Windows.
   *
   * Raycast ships for macOS and for Windows and its store is one index for
   * both. The ones that name macOS and not Windows never reach Sill at all.
   * This decides what happens to the ones that name nothing because they were
   * published before the field existed: hidden and counted, or shown and
   * marked.
   */
  windowsOnly: boolean;
  /**
   * A GitHub token, so more requests an hour are allowed.
   *
   * Sealed on its way to disk, so what comes back from Rust is the real value
   * and what is on disk is not. Null when unset.
   */
  githubToken: string | null;
}

/**
 * Looking something up on the web.
 *
 * On by default, unlike browser search: this reads nothing and knows nothing,
 * it is one row offering to open an address.
 */
export interface WebSearch {
  enabled: boolean;
  /** Which engine, by id. */
  engine: string;
  /** An address of your own, with `{query}` in it. Wins over `engine`. */
  customUrl: string;
}

/** What a screenshot does once it has been taken. */
export type AfterCapture = "copy" | "edit";

/** Taking pictures of the screen. */
export interface Screenshot {
  after: AfterCapture;
  /** Whether clicking a window in the picker captures that window. */
  clickAWindow: boolean;
  /** Which tool the editor opens with. */
  tool: string;
  /** The colour it opens with. */
  colour: string;
  /** The stroke width it opens with. */
  weight: number;
  /** The number the first badge shows. */
  stepFrom: number;
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
  /** How many unpinned entries are kept. Zero keeps as many as arrive. */
  maxEntries: number;
  keepImages: boolean;
  /** Lock stored pictures to this Windows account. */
  encryptImages: boolean;
  /** Applications whose copies are never recorded. */
  ignoredApps: string[];
  /** What to do with something that looks like a credential. */
  secrets: "skip" | "redact" | "keep";
}

export interface SnippetSettings {
  /** Watch typing and expand a keyword wherever it is typed. */
  expandKeywords: boolean;
}

/** Who answers when you ask something. */
export interface AiSettings {
  /** The id of the provider that answers. Empty means none is chosen. */
  provider: string;
  /** The ones set up. Each key is sealed before this file is written. */
  providers: import("$lib/ai").AiProvider[];
}

/** A modifier, as a person thinks of it rather than as Windows sends it. */
export type TapModifier = "control" | "alt" | "shift" | "win";

/**
 * Double-tapping a modifier to reach the launcher.
 *
 * The gesture every launcher eventually grows, because it needs no chord and
 * no key anything else wants: the modifier keeps doing its own job, and doing
 * it twice quickly is a thing nothing else listens for.
 */
export interface TapSettings {
  /** Which modifier, or null for the gesture being off. */
  modifier: TapModifier | null;
  /** How long the second tap has to arrive. */
  windowMs: number;
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

export type TtsEngine = "system" | "http" | "piper";

export interface TtsProvider {
  enabled: boolean;
  name: string | null;
  providerType: string | null;
  apiKey: string | null;
  baseUrl: string | null;
  lastModelId: string | null;
}

/** How text is read aloud. */
export interface TtsSettings {
  engine: TtsEngine;
  provider: TtsProvider;
  voice: string;
  piperVoice: string;
}

/** Where the weather is for. */
export interface Place {
  name: string;
  region: string;
  latitude: number;
  longitude: number;
}

/** The widgets, and which ride along in the launcher's chin. */
export interface WidgetSettings {
  pinned: string[];
  place: Place;
  fahrenheit: boolean;
  seconds: boolean;
}

/** Script commands: files somebody keeps that the launcher can run. */
export interface Scripts {
  enabled: boolean;
  folders: string[];
  timeoutSeconds: number;
  /** The scripts allowed to ask Windows for administrator rights, by path. */
  elevated: string[];
}

/** One key standing in for four modifiers. */
export interface HyperKey {
  /** Virtual key code, or null for off. */
  key: number | null;
}

export interface Preferences {
  general: General;
  snippets: SnippetSettings;
  taps: TapSettings;
  ai: AiSettings;
  dictation: DictationSettings;
  tts: TtsSettings;
  widgets: WidgetSettings;
  clipboard: ClipboardHistorySettings;
  hotkey: Hotkey;
  appearance: Appearance;
  sources: Sources;
  files: FileSearch;
  browsers: Browsers;
  store: StoreSettings;
  webSearch: WebSearch;
  screenshot: Screenshot;
  scripts: Scripts;
  hyper: HyperKey;
  bindings: Binding[];
  aliases: Alias[];
  navigation: NavigationSettings;
  actionKeys: ActionKeySettings;
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
  // Same reasoning. A palette is a canvas, an accent and sometimes a chroma
  // layer, and setting them one at a time would leave the window briefly
  // wearing half of one theme and half of another.
  root.setAttribute("data-theme", prefs.appearance.theme);
  // A multiplier the chroma gradients apply to their own alphas, so one
  // control moves all three washes together and keeps their balance.
  root.style.setProperty("--chroma-strength", String(prefs.appearance.chromaStrength));
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
  /** Whether the machine has the interpreter extensions run in. */
  nodeInstalled: boolean;
  indexedCommands: number;
  launchedEntries: number;
  extensions: ExtensionInfo[];
  bySource: { mode: string; count: number }[];
  /** Whether Sill believes the keyboard hook is installed. */
  keyboardHookInstalled: boolean;
  /**
   * Keystrokes that hook has actually been called for.
   *
   * Installed with this stuck at zero is the signature of a hook Windows
   * removed, which it does silently to any low-level hook whose callback runs
   * long. Snippet expansion, the hyper key and double-tap all die together and
   * nothing else says so.
   */
  keyboardKeysSeen: number;
}

export function getDiagnostics(): Promise<Diagnostics> {
  return invoke<Diagnostics>("diagnostics");
}

/** One summon, from the hotkey to being able to type. */
export interface SummonTiming {
  /** Hotkey to the window being shown. */
  shownMs: number;
  /** Shown to the page having painted. Absent if it never reported. */
  paintedMs?: number | null;
}

/** What reaching the launcher has cost lately. */
export interface Timings {
  /** Process start to the hotkey being live. */
  coldStartMs?: number | null;
  summons: SummonTiming[];
  /**
   * The middle complete summon, which is what to quote.
   *
   * The median rather than the mean: the slow ones are slow for reasons that
   * have nothing to do with Sill, such as a display coming out of sleep, and
   * one of those drags an average somewhere no summon ever was.
   */
  medianMs?: number | null;
  /** What each search source has cost this session, slowest per call first. */
  sources: Cost[];
  /** What each extension opened this session has cost, slowest first. */
  extensions: Cost[];
}

/** What one search source or one extension has cost this session. */
export interface Cost {
  /** The source, or the extension's id. */
  name: string;
  count: number;
  /** Microseconds: a search source answers in a couple of milliseconds. */
  totalUs: number;
  slowestUs: number;
}

export function getTimings(): Promise<Timings> {
  return invoke<Timings>("timings");
}

/** A search engine Sill knows. */
export interface SearchEngine {
  id: string;
  name: string;
  /** The address, with `{query}` where the words go. */
  url: string;
}

/**
 * The engines Sill knows.
 *
 * Asked for rather than listed here, so adding one is a line in Rust and not
 * two edits that can disagree.
 */
export function searchEngines(): Promise<SearchEngine[]> {
  return invoke<SearchEngine[]>("search_engines").catch(orElse("settings", "the search engines", [], "sources"));
}

/**
 * Which browsers are on this machine, named.
 *
 * So the settings page can say what would be read rather than asking somebody
 * to trust a switch. A feature that reads a browsing history should be able to
 * answer "whose?" before it is turned on.
 */
export interface KnownBrowser {
  name: string;
  /** The program behind it, so the pane can show its own mark. */
  program: string | null;
}

export function browserProfiles(): Promise<KnownBrowser[]> {
  return invoke<KnownBrowser[]>("browser_profiles").catch(orElse("settings", "which browsers are on this machine", [], "sources"));
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

/**
 * Writes everything Sill knows about itself into one file, and opens it.
 *
 * Answers with where it was written, so the row can say so rather than leaving
 * somebody to guess which of the files in the data folder it was.
 *
 * Deliberately not caught into a default. The whole point of the button is to
 * produce a file to send, and quietly answering with nothing would leave the
 * row saying it worked.
 */
export function exportDiagnostics(): Promise<string> {
  return invoke<string>("export_diagnostics");
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

/**
 * Writes every setting to a file, with every credential left out.
 *
 * Answers with where it went, or nothing if the dialog was closed without
 * choosing, which is an ordinary thing to do and needs no message.
 *
 * The file carries no API key and no token, in either the plain form Sill
 * holds or the sealed form its own file holds. A sealed value is bound to one
 * Windows account on one machine, so exporting one would leak a credential and
 * hand over something that could not be used anyway. `withheld` in the file
 * names what was left out.
 */
export function exportPreferences(): Promise<string | null> {
  return invoke<string | null>("export_preferences");
}

/** What an import of a settings file did. */
export interface Imported {
  /** What the file turned out to be, said the way somebody would say it. */
  readAs: string;
  /** The settings sections the file had something to say about. */
  sections: string[];
  /** Credentials the file did not carry, so the ones already here were kept. */
  keptKeys: number;
  /** Only for a Raycast export, which carries snippets rather than settings. */
  snippets?: { added: number; updated: number; skipped: number; keywordsTaken: number } | null;
  quicklinks?: { added: number; updated: number; skipped: number; keywordsTaken: number } | null;
}

/**
 * Reads a settings file over the settings held now.
 *
 * Reads a Sill export, a `preferences.json`, PowerToys Run's settings, or a
 * Raycast `.rayconfig`. A section the file says nothing about keeps what it
 * has, and **every way this can fail leaves the settings exactly as they
 * were**: the whole file is turned into settings before any of it is saved.
 */
export function importPreferences(): Promise<Imported | null> {
  return invoke<Imported | null>("import_preferences");
}

/**
 * Puts one settings panel back to what it shipped with.
 *
 * Which sections a panel owns is decided in Rust, so a reset cannot reach the
 * panel next to it.
 */
export function resetPanel(panel: string): Promise<void> {
  return invoke("reset_panel", { panel });
}

/**
 * The panels that have something of their own to reset.
 *
 * Asked rather than listed here, so the button appears exactly where the
 * command would do something. An empty answer draws no buttons, which is the
 * right outcome for a window that could not reach Rust.
 */
export function resettablePanels(): Promise<string[]> {
  return invoke<string[]>("resettable_panels").catch(silently([]));
}

/** Opens settings, optionally jumping straight to one section. */
export function openSettings(section?: string): Promise<void> {
  return invoke("open_settings", { section: section ?? null });
}

/**
 * Opens the window where a conversation has room.
 *
 * Built when first asked for rather than declared, so a session that never
 * opens it never pays for the renderer.
 */
export function openAsk(): Promise<void> {
  return invoke("open_ask");
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
  return invoke<string[]>("hotkey_conflicts").catch(orElse("settings", "which hotkeys another application already has", [], "shortcuts"));
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
  return invoke<IndexPage>("index_rows", { query, mode }).catch(
    orElse("settings", "what is in the index", { rows: [], total: 0 }, "sources"),
  );
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
 *
 * Silent, unlike the reads around it, for two reasons. It is the launcher that
 * asks, not this window, so a report would land in the settings window's group
 * and be cleared by the act of opening settings to read it. And an empty map
 * is not a lie anybody believes: the arrows and Enter are not chords and keep
 * working, so what is left is Ctrl+N doing nothing, which is visible to the
 * person pressing it in the instant they press it.
 */
export function navigationChords(): Promise<Record<string, Move>> {
  return invoke<Record<string, Move>>("navigation_chords").catch(silently({}));
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
  return invoke<NavigationKey[]>("navigation_keys").catch(orElse("settings", "which keys move around the launcher", [], "shortcuts"));
}

/** One action, as the shortcuts panel shows it. */
export interface ActionShortcut {
  id: string;
  title: string;
  /** The accelerator that runs it, or empty for an action with no key. */
  chord: string;
  /** Whether this was set by hand rather than shipped. */
  overridden: boolean;
  /**
   * The other action that wants this chord and gets it.
   *
   * Worked out in Rust, because whether two chords clash depends on which
   * actions appear on one list together and only the registry knows that.
   */
  contested?: string;
}

/**
 * Every action a key can be given, and the key it has.
 *
 * From the same registry the action panel and Enter use, so an action added in
 * Rust becomes bindable without this file changing.
 */
export function actionShortcuts(): Promise<ActionShortcut[]> {
  return invoke<ActionShortcut[]>("action_shortcuts").catch(
    orElse("settings", "which keys run which action", [], "shortcuts"),
  );
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
  return invoke<ToneChoice[]>("emoji_tones").catch(orElse("settings", "the emoji skin tones", [], "emoji"));
}
