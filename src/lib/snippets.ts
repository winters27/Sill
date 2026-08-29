/**
 * Snippets, mirrored from `src-tauri/src/snippets/`.
 *
 * Rust is the authority: it fills in any field this side omits, so a stale
 * copy here loses a field rather than corrupting one.
 */
import { invoke } from "@tauri-apps/api/core";

export interface Snippet {
  /** Stable across renames. Empty on a snippet that has never been saved. */
  id: string;
  name: string;
  /** Typed anywhere to expand it. Empty means launcher-only. */
  keyword: string;
  content: string;
  uses: number;
  /** Unix seconds. */
  created: number;
  /** Only fire when the keyword stands as a whole word. */
  wholeWord: boolean;
}

export interface Expansion {
  text: string;
  /** Characters from the start, or null when the snippet said nothing. */
  cursor: number | null;
}

/**
 * Every placeholder, for the editor's own reference.
 *
 * Shown beside the content field, because a feature nobody can discover is a
 * feature nobody has.
 */
export const PLACEHOLDERS: { token: string; means: string }[] = [
  { token: "{cursor}", means: "Where the caret ends up" },
  { token: "{clipboard}", means: "Whatever is on the clipboard" },
  { token: "{date}", means: "Today, as 2026-08-29" },
  { token: "{time}", means: "Now, as 14:32" },
  { token: "{uuid}", means: "A fresh unique id" },
];

export function listSnippets(): Promise<Snippet[]> {
  return invoke<Snippet[]>("list_snippets");
}

/** Adds or replaces one. Rejects a keyword another snippet already uses. */
export function saveSnippet(snippet: Snippet): Promise<void> {
  return invoke("save_snippet", { snippet });
}

export function deleteSnippet(id: string): Promise<void> {
  return invoke("delete_snippet", { id });
}

/** Fills in the placeholders, without typing anything. */
export function expandSnippet(id: string): Promise<Expansion> {
  return invoke<Expansion>("expand_snippet", { id });
}

/** A blank snippet, for the editor. */
export function emptySnippet(): Snippet {
  return { id: "", name: "", keyword: "", content: "", uses: 0, created: 0, wholeWord: true };
}
