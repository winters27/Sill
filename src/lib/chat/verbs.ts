/**
 * What each tool did, in words.
 *
 * A table rather than the tool's own name, because the names are written for
 * a model and read like an API. Two tenses: what it is doing while the step
 * runs, what it did once the step is over. A subject where there is one, so
 * ten lookups read as ten things rather than the same word ten times.
 *
 * Every key here is a tool in `src-tauri/src/ai/tools.rs`, and every tool
 * there is a key here. `verify:source` refuses the build when the two drift,
 * which is the failure that is otherwise silent: a tool added for the model
 * that the window reads out as `snake_case`.
 */

import type { StepPart } from "./parts";

/** The mark drawn beside a step. Names, not files: `StepIcon` draws them. */
export type StepIconName =
  | "search"
  | "file"
  | "folder"
  | "clipboard"
  | "window"
  | "machine"
  | "eye"
  | "act";

export interface Verb {
  doing: string;
  done: string;
  icon: StepIconName;
  /** Whether the subject is a phrase somebody typed, so it is quoted. */
  quotes?: boolean;
}

export const VERBS: Record<string, Verb> = {
  search_sill: {
    doing: "Searching this machine for",
    done: "Searched this machine for",
    icon: "search",
    quotes: true,
  },
  find_files: {
    doing: "Looking for files called",
    done: "Looked for files called",
    icon: "search",
    quotes: true,
  },
  read_file: { doing: "Reading", done: "Read", icon: "file" },
  list_directory: { doing: "Looking inside", done: "Looked inside", icon: "folder" },
  read_clipboard: {
    doing: "Reading what you copied",
    done: "Read what you copied",
    icon: "clipboard",
    quotes: true,
  },
  list_windows: { doing: "Looking at what is open", done: "Looked at what is open", icon: "window" },
  system_state: {
    doing: "Checking how this machine is set",
    done: "Checked how this machine is set",
    icon: "machine",
  },
  read_selection: {
    doing: "Reading what was selected",
    done: "Read what was selected",
    icon: "eye",
  },
  read_screen: { doing: "Reading what is on screen", done: "Read what is on screen", icon: "eye" },
  what_can_be_done: {
    doing: "Working out what can be done to",
    done: "Worked out what can be done to",
    icon: "act",
  },
  run_action: { doing: "Acting on", done: "Acted on", icon: "act" },
};

/** Anything unknown falls back to its name rather than to nothing. */
export function verbFor(tool: string): Verb {
  return VERBS[tool] ?? { doing: tool, done: tool, icon: "act" };
}

/**
 * One step as a sentence.
 *
 * A step still running is in the present tense; one that is over is in the
 * past. Whether it is over is the caller's to say, because a saved
 * conversation reopened later has steps that never got their result and
 * reading those as "running" left old answers looking busy forever.
 */
export function describe(step: StepPart, running: boolean): string {
  const verb = verbFor(step.tool);
  const said = running ? verb.doing : verb.done;
  const subject = step.subject.trim();

  if (!subject) return said;
  return verb.quotes ? `${said} “${subject}”` : `${said} ${subject}`;
}
