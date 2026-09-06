/**
 * The page's half of keeping the browser out of Sill's windows.
 *
 * Sill draws in WebView2, and most of the browser's habits are switched off
 * natively, once per window, in `src-tauri/src/webchrome.rs`: the default
 * context menu, developer tools, the accelerator keys, zoom. This is what is
 * left for the page to do, in every window, from the root layout:
 *
 * - The `contextmenu` event is cancelled. The native switch already draws no
 *   menu; cancelling the event as well means a runtime that ignores the
 *   switch, or a page element the browser would give its own menu, gets none
 *   either.
 * - The browser keys that still reach the page are cancelled: F12, F5, F3,
 *   F7, F11, Shift+F10, the Menu key, Ctrl with the print, save, open, find,
 *   view-source, reload and zoom letters, and Alt with the arrow keys that
 *   walk history. Cancelling is only the browser's default going away; every
 *   handler Sill has on those keys still runs.
 * - Sill's own global hotkeys are honoured.
 *
 * ## Why the page honours the global hotkeys
 *
 * Every global hotkey is taken by the low-level keyboard hook before any
 * window sees it, and by a registration with Windows behind that. Both do
 * their work while some other program has the keyboard. While a Sill window
 * itself is in front, a keystroke has been seen to arrive at the page with
 * neither having taken it (2026-09-05: the Menu key bound to screenshots
 * opened the browser's context menu in the launcher). So the window that has
 * the keyboard asks Rust whether the chord is one of Sill's and has Rust run
 * it, the way the hook would.
 *
 * The list of chords is Rust's (`hotkey_chords`, the same table the hook
 * watches), read once per window and again when preferences change. Matching
 * is one set lookup per keystroke, on a string built the way the recorder
 * builds the one it saved, so no normalisation is needed on either side.
 *
 * A recorder in the settings window taking a new chord is exempt: the key it
 * is being handed may well be one that already does something.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { chordFor } from "$lib/keys";
import { orElse, silently } from "$lib/status";

/** What a keydown amounts to for this layer. */
export type Answer = { hotkey: string } | { browser: true } | null;

/** Keys the browser acts on by itself, with no modifier. */
const BROWSER_KEYS = new Set(["F12", "F5", "F3", "F7", "F11", "ContextMenu"]);

/** Letters the browser acts on under Ctrl: reload, print, find, source, save, open, zoom. */
const BROWSER_CTRL = new Set(["r", "p", "f", "g", "u", "s", "o", "+", "-", "=", "0"]);

/** Letters the browser acts on under Ctrl+Shift: developer tools, three ways. */
const BROWSER_CTRL_SHIFT = new Set(["i", "j", "c"]);

type Press = Pick<KeyboardEvent, "key" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">;

/**
 * Decides what one keydown is: one of Sill's hotkeys, a key the browser
 * would act on, or nothing this layer cares about.
 */
export function answerFor(event: Press, chords: ReadonlySet<string>): Answer {
  const seen = chordFor("hotkey", event);
  if ("chord" in seen && chords.has(seen.chord)) {
    return { hotkey: seen.chord };
  }

  if (BROWSER_KEYS.has(event.key)) return { browser: true };
  if (event.shiftKey && event.key === "F10") return { browser: true };

  const letter = event.key.toLowerCase();
  if (event.ctrlKey && !event.altKey) {
    if (event.shiftKey && BROWSER_CTRL_SHIFT.has(letter)) return { browser: true };
    if (!event.shiftKey && BROWSER_CTRL.has(letter)) return { browser: true };
  }

  if (event.altKey && !event.ctrlKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
    return { browser: true };
  }

  return null;
}

/** Whether the keystroke is being recorded by a recorder rather than typed. */
function recording(event: Event): boolean {
  return event.target instanceof Element && event.target.closest("[data-recording]") !== null;
}

/** Whether this page is inside Sill rather than a plain browser preview. */
function bridged(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * The chords the keyboard hook watches, as Rust has them. An empty list when
 * the answer is not a list, and a reported one when the command was refused:
 * a window with no hotkeys would be believed.
 */
async function hotkeyChords(): Promise<string[]> {
  const got = await invoke<unknown>("hotkey_chords").catch(
    orElse("launcher", "which keys are Sill's own", null),
  );
  return Array.isArray(got) ? got.filter((one): one is string => typeof one === "string") : [];
}

/** Runs one of Sill's hotkeys, the way the hook would. */
function pressHotkey(accelerator: string): void {
  void invoke("press_hotkey", { accelerator }).catch(
    orElse("launcher", `the ${accelerator} shortcut`, false),
  );
}

/**
 * Installs the layer on this document. Returns what takes it down again.
 */
export function quiet(): () => void {
  const chords = new Set<string>();

  const onContextMenu = (event: Event) => {
    event.preventDefault();
  };

  const onKeydown = (event: KeyboardEvent) => {
    if (recording(event)) return;
    const answer = answerFor(event, chords);
    if (!answer) return;
    event.preventDefault();
    if ("hotkey" in answer) {
      // Stopped as well as cancelled: a chord that opens the screenshot
      // overlay must not also be a character in the search field.
      event.stopPropagation();
      pressHotkey(answer.hotkey);
    }
  };

  document.addEventListener("contextmenu", onContextMenu, true);
  document.addEventListener("keydown", onKeydown, true);

  let unlisten: (() => void) | undefined;
  let gone = false;

  if (bridged()) {
    const reload = async () => {
      const next = await hotkeyChords();
      if (gone) return;
      chords.clear();
      for (const one of next) chords.add(one);
    };
    void reload();
    // A window that cannot hear preference changes keeps the list it read;
    // the next open reads a fresh one.
    listen("sill://preferences-changed", () => void reload())
      .then((off) => {
        if (gone) off();
        else unlisten = off;
      })
      .catch(silently(undefined));
  }

  return () => {
    gone = true;
    document.removeEventListener("contextmenu", onContextMenu, true);
    document.removeEventListener("keydown", onKeydown, true);
    unlisten?.();
  };
}
