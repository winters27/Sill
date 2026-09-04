/**
 * What Rust said was selected, as rows the launcher already knows how to draw.
 *
 * One key opens the action panel on whatever is selected: three files
 * highlighted in Explorer, or a paragraph in a document. Rust does the reading,
 * because it has to happen while something else is in front, and it hands over
 * a list of objects. Turning an object into a row is presentation, so it is
 * here.
 *
 * The rows are ordinary `RankedCommand`s on purpose. Everything that already
 * works on a result row then works on these: the arrow keys, the grouping, the
 * preview, and above all the action panel, which asks Rust what can be done to
 * the row's mode. A list of its own would have been a second answer to a
 * question the launcher already answers.
 */
import type { RankedCommand } from "$lib/exthost/commands";

/**
 * One thing Rust resolved a shortcut to.
 *
 * The wire form of `Object`. `kind` is not read here: `mode` carries the same
 * decision in the vocabulary the rows and the action panel already speak, and
 * reading both would be two opinions about what a thing is.
 */
export interface SelectedObject {
  kind: string;
  id: string;
  target: string;
  title: string;
  mode: string;
}

/** How much of a selection is worth putting on a row. */
const PREVIEW = 80;

/**
 * A one-line stand-in for a block of text.
 *
 * Rust already shortens the title to one line for its own logs; this shortens
 * the body for the row underneath it, and the two are separate because the
 * subtitle has a whole row to itself and can afford more.
 */
function glance(text: string): string {
  const flattened = text.replace(/\s+/g, " ").trim();
  return flattened.length > PREVIEW ? `${flattened.slice(0, PREVIEW)}…` : flattened;
}

/**
 * What each mode's rows are headed, and what a mode nobody named gets.
 *
 * A table rather than the chain of ternaries this was. The chain answered
 * "File" for everything it did not name, so a reminder arriving from a fired
 * timer would have been headed "File" with no file anywhere near it and
 * nothing would have said so. Naming the fallback once, here, is what makes a
 * mode added later obviously unnamed rather than quietly wrong.
 */
const HEADED: Record<string, string> = {
  folder: "Folder",
  file: "File",
  text: "Selection",
  // A reminder Windows started, which is text with a reason for being here.
  // It is `text` to Rust and to the action panel; only the heading differs.
  "reminder-shown": "Reminder",
};

/** The modes whose target is words rather than a path. */
const WORDS = new Set(["text", "reminder-shown"]);

/**
 * The rows for a resolved selection.
 *
 * A file keeps its path as the subtitle and as the icon, exactly as a file
 * found by searching does, so the same selection looks the same however it
 * arrived. Text has no path and no icon, so its subtitle is the start of the
 * text itself: the title is already the first line, and on a multi-line
 * selection the second line is what tells you which paragraph you caught.
 */
export function selectionRows(objects: SelectedObject[]): RankedCommand[] {
  return objects.map((object) => {
    const words = WORDS.has(object.mode);

    return {
      id: object.id,
      extension: "selection",
      // The heading a lone row never shows and a mixed selection does. Rust
      // decides which of the two a thing is; this only names it.
      extensionTitle: HEADED[object.mode] ?? "File",
      title: object.title,
      subtitle: words ? glance(object.target) : object.target,
      mode: object.mode as RankedCommand["mode"],
      entrypoint: object.target,
      // A path is its own icon. Words have no file to take one from, and an
      // icon guessed for them would be an icon for the wrong thing.
      icon: words ? null : object.target,
      matched: [],
    };
  });
}
