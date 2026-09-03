/**
 * Clipboard history, mirrored from `src-tauri/src/clipboard/`.
 *
 * Rust is the authority: it fills in any field this side omits, so a stale
 * copy here loses a field rather than corrupting one.
 */
import { invoke } from "@tauri-apps/api/core";
import { orElse, silently } from "$lib/status";

export type ClipKind = "text" | "link" | "email" | "color" | "file" | "image";

/** What the type filter offers, in the order it offers them. */
export const KIND_FILTERS: { id: ClipKind | "all"; label: string }[] = [
  { id: "all", label: "All Types" },
  { id: "text", label: "Text" },
  { id: "image", label: "Images" },
  { id: "file", label: "Files" },
  { id: "link", label: "Links" },
  { id: "email", label: "Emails" },
  { id: "color", label: "Colors" },
];

export interface ClipEntry {
  id: number;
  kind: ClipKind;
  text: string;
  /** Unix seconds. */
  firstSeen: number;
  lastSeen: number;
  uses: number;
  pinned: boolean;
  app: string | null;
  /** The source application's executable, which the icon comes from. */
  appPath: string | null;
  bytes: number;
  /** Whether a formatted version was kept beside the text. */
  rich: boolean;
}

/** One entry with everything the preview pane shows. */
export interface ClipDetail extends ClipEntry {
  /** A PNG data URI, for an image entry. */
  image: string | null;
  /** The source application's icon, as a data URI. */
  appIcon: string | null;
}

/**
 * What one entry is, in the singular.
 *
 * The filter labels are plural because they name a set to choose from; a row
 * saying an entry's type is "Links" is describing one thing with the word for
 * many.
 */
export function kindName(kind: ClipKind): string {
  return {
    text: "Text",
    link: "Link",
    email: "Email",
    color: "Color",
    file: "File",
    image: "Image",
  }[kind];
}

export function clipboardSearch(query: string, kind: ClipKind | "all"): Promise<ClipEntry[]> {
  return invoke<ClipEntry[]>("clipboard_search", { query, kind });
}

/**
 * One entry in full.
 *
 * Separate from the listing because a list of four hundred rows must not
 * carry four hundred screenshots with it.
 */
export function clipboardEntry(id: number): Promise<ClipDetail | null> {
  return invoke<ClipDetail | null>("clipboard_entry", { id });
}

/** Puts an entry back on the clipboard, and optionally pastes it. */
export function clipboardPaste(
  id: number,
  paste: boolean,
  plainText = false,
): Promise<void> {
  return invoke("clipboard_paste", { id, paste, plainText });
}

export function clipboardPin(id: number, pinned: boolean): Promise<void> {
  return invoke("clipboard_pin", { id, pinned });
}

export function clipboardDelete(id: number): Promise<void> {
  return invoke("clipboard_delete", { id });
}

/** Empties the history. Pinned entries survive unless `everything`. */
export function clipboardClear(everything: boolean): Promise<number> {
  return invoke<number>("clipboard_clear", { everything });
}

export function clipboardCount(): Promise<number> {
  return invoke<number>("clipboard_count");
}

/**
 * The day heading an entry belongs under.
 *
 * Compared as calendar days rather than by elapsed hours: something copied at
 * one in the morning belongs under Today, not under Yesterday because it was
 * twenty-three hours ago.
 */
export function dayLabel(at: number, now = Date.now()): string {
  const then = new Date(at * 1000);
  const today = new Date(now);

  const midnight = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((midnight(today) - midnight(then)) / 86_400_000);

  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return then.toLocaleDateString(undefined, { weekday: "long" });
  if (then.getFullYear() === today.getFullYear()) {
    return then.toLocaleDateString(undefined, { day: "numeric", month: "long" });
  }
  return then.toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" });
}

/** "just now", "14:32", "28 Aug 14:32". */
export function timeLabel(at: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.floor(now / 1000) - at);
  if (seconds < 60) return "just now";

  const then = new Date(at * 1000);
  const time = then.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  if (seconds < 86_400) return time;

  return `${then.toLocaleDateString(undefined, { day: "numeric", month: "short" })} ${time}`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** A single line to stand in for a multi-line entry in the list. */
export function preview(text: string): string {
  const line = text.trim().split("\n", 1)[0] ?? "";
  return line.length > 160 ? `${line.slice(0, 160)}…` : line;
}

/** Something that was not recorded because it looked like a credential. */
export interface Skipped {
  /** What it appeared to be, in the vendor's words: "GitHub personal access token". */
  what: string;
  length: number;
}

/**
 * The last thing declined for looking like a credential.
 *
 * Asked for on mount rather than only listened for. Almost every copy happens
 * while the launcher is hidden, so an event alone would arrive with nobody
 * there and the entry would be quietly missing when the history is next
 * opened.
 */
export function clipboardLastSkipped(): Promise<Skipped | null> {
  // Silent, because `null` is what this answers nearly every time it is
  // asked and the offer it produces is a courtesy rather than an answer.
  // Nothing was going to be shown here, so nothing is missing from the view,
  // and a sentence about it would be the surface talking about itself.
  return invoke<Skipped | null>("clipboard_last_skipped").catch(silently(null));
}

/**
 * Records what is on the clipboard now, whatever it looks like.
 *
 * The way back from a wrong guess. Nothing was held to make this possible: the
 * entry is still on the clipboard, so this simply reads it again.
 */
export function clipboardKeepCurrent(): Promise<void> {
  return invoke("clipboard_keep_current");
}

/**
 * Several entries joined into one piece of text.
 *
 * Built in Rust from ids rather than here from rows already on screen: the
 * list picked from is not necessarily the list still showing, and reading the
 * entries again means the result is what was chosen.
 */
export function clipboardMerge(ids: number[], separator: string): Promise<string> {
  return invoke<string>("clipboard_merge", { ids, separator });
}

/** A named group of history entries. */
export interface Collection {
  id: number;
  name: string;
  created: number;
  /** How many entries are in it right now. */
  count: number;
}

/**
 * Every named group, for the rail that lists them.
 *
 * Reported when it fails. An empty list here is drawn as "you have not made
 * any collections", which is a claim about somebody's own saved work, and it
 * is wrong in the one direction that matters: the collections are still in the
 * database and the view says they are not.
 */
export function clipboardCollections(): Promise<Collection[]> {
  return invoke<Collection[]>("clipboard_collections").catch(
    orElse("launcher", "the collections in the clipboard history", [], "clipboard"),
  );
}

/** Makes a collection, or returns the one already called that. */
export function clipboardCreateCollection(name: string): Promise<number> {
  return invoke<number>("clipboard_create_collection", { name });
}

export function clipboardRenameCollection(id: number, name: string): Promise<void> {
  return invoke("clipboard_rename_collection", { id, name });
}

/** Removes a collection. The entries in it are untouched. */
export function clipboardDeleteCollection(id: number): Promise<void> {
  return invoke("clipboard_delete_collection", { id });
}

export function clipboardAddToCollection(collection: number, ids: number[]): Promise<number> {
  return invoke<number>("clipboard_add_to_collection", { collection, ids });
}

export function clipboardRemoveFromCollection(collection: number, id: number): Promise<void> {
  return invoke("clipboard_remove_from_collection", { collection, id });
}

/**
 * What is in a collection, in the order it was arranged.
 *
 * Reported for the same reason as the list of collections. A collection that
 * opens empty reads as one somebody emptied, and the row above it is still
 * showing the count of what is really in there.
 */
export function clipboardCollectionEntries(collection: number): Promise<ClipEntry[]> {
  return invoke<ClipEntry[]>("clipboard_collection_entries", { collection }).catch(
    orElse("launcher", "what is in a clipboard collection", [], "clipboard"),
  );
}
