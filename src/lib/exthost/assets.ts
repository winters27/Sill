/**
 * Pictures out of a running extension's own `assets` directory.
 *
 * An extension writes `icon: "files.png"` or `content: "logo.svg"` and means
 * the file beside its code. The window cannot read that file: it has no idea
 * where an installed extension lives, and a page in WebView2 cannot open a
 * local path even if it did. So it asks Rust, naming the session the view
 * belongs to, and Rust finds the extension behind that session, refuses a
 * name that climbs out of `assets`, reads the picture and hands back a data
 * URI. The window's part is to ask once per picture and to be able to answer
 * synchronously the second time, so a row redrawn on the next keystroke does
 * not flash a letter before the picture comes back.
 *
 * The same shape as the application icon cache in `commands.ts`, for the
 * same reason: an answer already in hand is readable without awaiting, and
 * an answer not yet in hand is distinguishable from "no picture".
 *
 * ## The session
 *
 * The page that hosts a view puts a getter for the current session id in
 * Svelte context under `VIEW_SESSION`; `ExtIcon` reads it from wherever it
 * sits in the tree. A view drawn with no session, which is what the design
 * previews are, gets `null` for every asset and letters itself as before.
 */

import { invoke } from "@tauri-apps/api/core";
import { silently } from "$lib/status";

/** The context key the hosting page sets to `() => session`. */
export const VIEW_SESSION = "sill:view-session";

/** A getter, so the context is set once and the session can still change. */
export type SessionOf = () => string | null;

interface Held {
  asked: Promise<string | null>;
  /** Present once the answer is back. */
  uri?: string | null;
}

const held = new Map<string, Held>();

/** Enough for a long grid; the oldest are let go beyond it. */
const KEPT = 400;

function keyOf(session: string, name: string): string {
  return `${session}\n${name}`;
}

/**
 * The picture, if the answer is already here.
 *
 * `undefined` when nothing is known yet, `null` when Rust had no picture.
 */
export function knownAsset(session: string | null, name: string): string | null | undefined {
  if (!session) return null;
  const entry = held.get(keyOf(session, name));
  return entry && "uri" in entry ? (entry.uri ?? null) : undefined;
}

/** Asks Rust for the picture, once per session and name. */
export function extensionAsset(session: string | null, name: string): Promise<string | null> {
  if (!session) return Promise.resolve(null);

  const key = keyOf(session, name);
  const already = held.get(key);
  if (already) return already.asked;

  // Remembered as "no picture" whether Rust said so or refused to answer:
  // this is asked once per row drawn, and a picture that cannot be read
  // will not read better on the next keystroke.
  const asked = invoke<string | null>("extension_asset", { session, name })
    .then((got) => (typeof got === "string" ? got : null))
    .catch(silently<string | null>(null));

  const entry: Held = { asked };
  held.set(key, entry);
  void asked.then((uri) => {
    if (held.get(key) === entry) entry.uri = uri;
  });

  while (held.size > KEPT) {
    const oldest = held.keys().next();
    if (oldest.done) break;
    held.delete(oldest.value);
  }

  return asked;
}

/** For tests. */
export function forgetAssets(): void {
  held.clear();
}
