/**
 * Chords as keys, for every recorder and every keycap in the settings window.
 *
 * ## Why this exists
 *
 * There were three ways to turn a keypress into a chord (`acceleratorFrom`,
 * `chordFrom`, and an inline copy in the shortcuts panel that never named
 * Space or the arrows) and four ways to draw one: the raw string with plus
 * signs, the string with the pluses swapped for spaces, one keycap around the
 * whole chord, and per-key caps from a Raycast `Shortcut` object. One panel
 * showed three of the four. This is the one place the two grammars meet.
 *
 * ## What is not here
 *
 * Which chord is free. That is `key_owners` in Rust, which reads the same
 * sheet the keyboard reference draws. A recorder asks it before saving.
 */
import { acceleratorFrom, chordFrom } from "$lib/settings";

/**
 * What a recorder is recording for, which decides what it will accept.
 *
 * No scope requires more than one key. Every scope allows a combination.
 *
 * - `hotkey`: a global key Windows registers. A key on its own is fine (F12,
 *   Pause); a bare letter, digit or Space is allowed too, with a caution,
 *   because it takes that key from every program while Sill runs. The Windows
 *   key is allowed.
 * - `binding`: the same, for a key that runs an action on the selection.
 * - `navigation`: a key that moves around the launcher while it has focus.
 *   Anything goes: Down means Down.
 * - `action`: a key that runs an action on the selected row. A bare letter,
 *   digit or punctuation is refused, because the search field has focus and
 *   would type it; a key that types nothing (F5, Insert) works on its own. The
 *   Windows key is refused, because the launcher reads it as Ctrl and the
 *   chord would fire on the Ctrl version of itself; Rust refuses it too.
 */
export type Scope = "hotkey" | "binding" | "navigation" | "action";

/**
 * The sections of the keyboard reference, as Rust titles them (`keysheet.rs`).
 *
 * A recorder names the section its key lives in so a key that already does
 * something in the same section is refused, and the map files keys by them.
 */
export const SECTIONS = {
  opening: "Opening Sill",
  anywhere: "From anywhere",
  moving: "Moving around",
  acting: "Acting on a row",
} as const;

/**
 * What a keycap prints, for the keys whose chord name is not it.
 *
 * The chord keeps the browser's name (`ContextMenu`, `AudioVolumeUp`),
 * because that is what the recorder captured and what Rust parses; the cap
 * prints what the keyboard does, because a cap reading ContextMenu is a cap
 * reading the wrong language.
 */
const LABELS: Record<string, string> = {
  Ctrl: "Ctrl",
  Alt: "Alt",
  Shift: "Shift",
  Super: "Win",
  Up: "↑",
  Down: "↓",
  Left: "←",
  Right: "→",
  Enter: "↵",
  Backspace: "⌫",
  Delete: "Del",
  Escape: "Esc",
  Space: "Space",
  Tab: "Tab",
  ContextMenu: "Menu",
  Insert: "Ins",
  PageUp: "PgUp",
  PageDown: "PgDn",
  CapsLock: "Caps",
  NumLock: "NumLk",
  ScrollLock: "ScrLk",
  PrintScreen: "PrtSc",
  AudioVolumeUp: "Vol+",
  AudioVolumeDown: "Vol−",
  AudioVolumeMute: "Mute",
  MediaPlayPause: "Play",
  MediaStop: "Stop",
  MediaTrackNext: "Next",
  MediaTrackPrevious: "Prev",
  BrowserBack: "Back",
  BrowserForward: "Forward",
  BrowserRefresh: "Refresh",
  BrowserSearch: "Search",
  BrowserFavorites: "Favorites",
  BrowserHome: "Home",
  LaunchMail: "Mail",
  LaunchMediaPlayer: "Media",
  LaunchApplication1: "Explorer",
  LaunchApplication2: "Calculator",
  NumpadAdd: "Num +",
  NumpadSubtract: "Num −",
  NumpadMultiply: "Num *",
  NumpadDivide: "Num /",
  NumpadDecimal: "Num .",
};

/** The modifiers, in the order a chord writes them. */
const MODIFIERS = ["Ctrl", "Alt", "Shift", "Super"];

/**
 * The keycaps a chord is drawn as, in the order they are pressed.
 *
 * `"Ctrl+Shift+Up"` is `["Ctrl", "Shift", "↑"]`. An empty chord is no caps at
 * all, so a caller can draw nothing rather than one empty cap.
 */
export function keysOf(accelerator: string): string[] {
  return accelerator
    .split("+")
    .map((part) => part.trim())
    .filter((part) => part.length > 0)
    .map((part) => LABELS[part] ?? part);
}

/** The modifiers a chord holds, as caps, for lighting a keyboard map. */
export function modifiersOf(accelerator: string): string[] {
  return accelerator
    .split("+")
    .map((part) => part.trim())
    .filter((part) => MODIFIERS.includes(part))
    .map((part) => LABELS[part] ?? part);
}

/** The key a chord ends on, as a cap, or empty for a chord with no key. */
export function keyOf(accelerator: string): string {
  const parts = accelerator
    .split("+")
    .map((part) => part.trim())
    .filter((part) => part.length > 0 && !MODIFIERS.includes(part));
  const last = parts.at(-1);
  return last ? (LABELS[last] ?? last) : "";
}

/** Whether a key on its own would type something: a letter, a digit, punctuation or Space. */
function types(event: Pick<KeyboardEvent, "key">): boolean {
  return [...event.key].length === 1;
}

/**
 * What a keypress amounts to for a recorder in one scope.
 *
 * - `{ held }` while only modifiers are down, so the recorder can show the
 *   keys so far without committing to a chord nobody can press.
 * - `{ refused }` for a press the scope does not accept, with the sentence to
 *   show under the control.
 * - `{ chord }` when there is something to save, with a `caution` when it is
 *   legal but worth a second look.
 */
export function chordFor(
  scope: Scope,
  event: Pick<KeyboardEvent, "key" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">,
): { chord: string; caution?: string } | { refused: string } | { held: string[] } {
  const isModifier = ["Control", "Alt", "Shift", "Meta", "OS"].includes(event.key);
  if (isModifier) {
    const held: string[] = [];
    if (event.ctrlKey) held.push("Ctrl");
    if (event.altKey) held.push("Alt");
    if (event.shiftKey) held.push("Shift");
    if (event.metaKey) held.push("Win");
    return { held };
  }

  const bare = !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey;

  if (scope === "action") {
    if (event.metaKey) {
      return { refused: "The Windows key cannot run an action" };
    }
    if (bare && types(event)) {
      return { refused: "On its own that key would be typed into the search field. Add Ctrl or Alt, or use a key that types nothing." };
    }
  }

  if (scope === "hotkey" || scope === "binding") {
    const chord = acceleratorFrom(event as KeyboardEvent);
    if (!chord) {
      return { refused: "That key cannot be a shortcut" };
    }
    if (bare && types(event)) {
      return { chord, caution: `${chord} on its own is taken from every program while Sill runs.` };
    }
    return { chord };
  }

  const chord = chordFrom(event as KeyboardEvent);
  return chord ? { chord } : { refused: "That key cannot be a shortcut" };
}
