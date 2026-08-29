/**
 * Clipboard history, mirrored from `src-tauri/src/clipboard/`.
 *
 * Rust is the authority: it fills in any field this side omits, so a stale
 * copy here loses a field rather than corrupting one.
 */
import { invoke } from "@tauri-apps/api/core";

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
export function clipboardPaste(id: number, paste: boolean): Promise<void> {
  return invoke("clipboard_paste", { id, paste });
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
