/**
 * Quicklinks, mirrored from `src-tauri/src/quicklinks/`.
 *
 * Rust is the authority: it fills in any field this side omits, so a stale
 * copy here loses a field rather than corrupting one.
 */
import { invoke } from "@tauri-apps/api/core";

export interface Quicklink {
  /** Empty when creating; Rust assigns one on the first save. */
  id: string;
  name: string;
  /** Where it goes, with `{query}` and the other placeholders still in it. */
  link: string;
  /** Typed in the launcher to reach it directly. Optional. */
  keyword: string;
  /** Path to the application that opens it, or empty for the system default. */
  openWith: string;
  uses: number;
  /** Unix seconds. */
  created: number;
}

export function listQuicklinks(): Promise<Quicklink[]> {
  return invoke<Quicklink[]>("list_quicklinks");
}

/** Adds or replaces one, and returns the saved list. */
export function saveQuicklink(link: Quicklink): Promise<Quicklink[]> {
  return invoke<Quicklink[]>("save_quicklink", { link });
}

export function deleteQuicklink(id: string): Promise<Quicklink[]> {
  return invoke<Quicklink[]>("delete_quicklink", { id });
}

/** Opens one, with `query` filling `{query}`. Returns the resolved target. */
export function openQuicklink(id: string, query: string): Promise<string> {
  return invoke<string>("open_quicklink", { id, query });
}

/**
 * Whether opening this needs something typed first.
 *
 * Mirrors `Quicklink::needs_argument`. Only `{query}` counts: the other
 * placeholders answer themselves from the clipboard or the clock.
 */
/** What an import changed, counted rather than summarised. */
export interface TransferSummary {
  added: number;
  updated: number;
  /** Already here word for word, and left alone. */
  skipped: number;
  /** Arrived with a keyword another link already answers to. */
  keywordsTaken: number;
}

/** Writes them all to a file, and says where. Nothing chosen returns null. */
export function exportQuicklinks(): Promise<string | null> {
  return invoke<string | null>("export_quicklinks");
}

/** Reads a file in. Additive: nothing already here is removed. */
export function importQuicklinks(): Promise<TransferSummary | null> {
  return invoke<TransferSummary | null>("import_quicklinks");
}

export function needsArgument(link: string): boolean {
  return /\{\s*query\s*\}/i.test(link);
}

/** An empty one, for the editor's "new" state. */
export function blankQuicklink(): Quicklink {
  return { id: "", name: "", link: "", keyword: "", openWith: "", uses: 0, created: 0 };
}
