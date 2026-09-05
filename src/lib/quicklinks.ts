/**
 * Quicklinks, mirrored from `src-tauri/src/quicklinks/`.
 *
 * Rust is the authority: it fills in any field this side omits, so a stale
 * copy here loses a field rather than corrupting one.
 */
import { invoke } from "@tauri-apps/api/core";
import { orElse } from "$lib/status";

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
  /**
   * A scheme this one link may open beyond the ones Sill opens on its own,
   * `notion` for a `notion://` deep link. Empty for nearly every link.
   * Granted by the person editing it, never by an imported file.
   */
  allowedScheme: string;
  /** Words of your own it is found by, and grouped under with `tag:`. */
  tags: string[];
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
  return {
    id: "",
    name: "",
    link: "",
    keyword: "",
    openWith: "",
    allowedScheme: "",
    tags: [],
    uses: 0,
    created: 0,
  };
}

/**
 * The scheme a link would need allowing for, or nothing.
 *
 * Rust decides, because Rust is what refuses at open time and the editor
 * must not disagree with it. Nothing when the link is a web address, a path,
 * or one of the schemes that run code and can never be allowed.
 */
export async function quicklinkSchemeToAllow(link: string): Promise<string | null> {
  const said = await invoke<unknown>("quicklink_scheme_to_allow", { link }).catch(
    orElse("settings", "whether a quicklink needs a scheme allowed", null, "quicklinks"),
  );
  return typeof said === "string" && said ? said : null;
}
