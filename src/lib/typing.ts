/**
 * Characters that arrive before the field is ready to take them.
 *
 * ## Why this exists
 *
 * The launcher is summoned by a key and typed into immediately, and for a
 * short window after it appears there was nowhere for those characters to go.
 * Focus was taken inside a `requestAnimationFrame`, so anything typed between
 * the window being shown and that frame running landed on the document and was
 * discarded. A launcher that drops the first letter of a fast query is worse
 * than one that is slow, because the query that comes back is wrong rather
 * than late.
 *
 * The window handler catches those keystrokes and puts them in the field
 * itself. That covers the frame gap and every other moment focus is briefly
 * elsewhere, rather than one of them.
 */

/** Enough of a keyboard event to decide what it is. */
export interface Keystroke {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/** Where a field's caret is, and what it holds. */
export interface Field {
  value: string;
  start: number;
  end: number;
}

/**
 * Whether this keystroke is a character somebody meant to type.
 *
 * Length is how the DOM itself separates text from commands: `key` is the
 * character for text and a name like `ArrowDown` or `Escape` otherwise. Space
 * is a character and is treated as one, which is right in a search field.
 *
 * Held modifiers make it a command whatever the key is, and that includes the
 * navigation presets, which are all Ctrl chords precisely because the field
 * has focus the whole time and a bare `j` is the letter j.
 */
export function isTyping(event: Keystroke): boolean {
  if (event.ctrlKey || event.altKey || event.metaKey) return false;

  // Counted by code point rather than by `.length`, so a character outside
  // the basic plane is one keystroke rather than two units.
  return [...event.key].length === 1;
}

/**
 * The field after that character is typed into it.
 *
 * Replaces the selection, because that is what typing does and because a
 * summon that selected the old query means the next character replaces it.
 * The two ends are ordered rather than trusted: a selection made by dragging
 * backwards reports its start after its end.
 */
export function typedInto(field: Field, key: string): { value: string; caret: number } {
  const start = Math.max(0, Math.min(field.start, field.end));
  const end = Math.min(field.value.length, Math.max(field.start, field.end));

  return {
    value: field.value.slice(0, start) + key + field.value.slice(end),
    caret: start + key.length,
  };
}

/**
 * Whether Delete means the row under the cursor rather than a character.
 *
 * The clipboard and the conversation list both remove the selected row on
 * Delete, and both have a search field that holds focus the whole time. So
 * somebody filtering a list, pressing Delete to remove a character they just
 * typed, destroyed a saved conversation or a clipboard entry instead. No
 * confirmation, and for a conversation no undo.
 *
 * With nothing typed there is no character to delete, so the key can only mean
 * the row. With something typed it is text editing, and Ctrl+Delete is how to
 * say you meant the row after all. The action panel advertises the chord that
 * always works, which is where somebody who does not know this reads it.
 */
export function deleteMeansTheRow(event: Keystroke, typed: string): boolean {
  if (event.key !== "Delete") return false;

  return event.ctrlKey || event.metaKey || typed.length === 0;
}
