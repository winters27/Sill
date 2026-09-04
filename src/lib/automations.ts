/**
 * Triggers, mirrored from `src-tauri/src/automation.rs`.
 *
 * Nothing is kept here. Every list comes from Task Scheduler on the call that
 * asks for it, because Windows holds the schedule and a copy on this side
 * would be a second answer to a question that already has one.
 */
import { invoke } from "@tauri-apps/api/core";

/** When Windows should start it. Mirrors `automation::When`. */
export type When =
  | { kind: "daily"; hour: number; minute: number }
  | { kind: "atLogon" }
  | { kind: "onUnlock" };

/** A trigger, as it is described on the way in. */
export interface Trigger {
  name: string;
  action: string;
  target: string;
  kind: string | null;
  argument: string | null;
  when: When;
}

/**
 * A trigger, as Windows holds it.
 *
 * `title` and `target` together, or `suspect` alone. A row Sill will not
 * vouch for is drawn as what it actually says rather than under the name it
 * was given, because the one thing this list must never do is describe a
 * rewritten task in Sill's own words.
 */
export interface Row {
  name: string;
  enabled: boolean;
  /** What Windows says happens next, or null for a trigger with no next. */
  next: string | null;
  title: string | null;
  target: string | null;
  suspect: string | null;
}

/** An action a trigger may name. */
export interface Offer {
  id: string;
  title: string;
}

export function listAutomations(): Promise<Row[]> {
  return invoke<Row[]>("automations");
}

/** The actions that never stop and ask, which is the ones a trigger may run. */
export function schedulableActions(): Promise<Offer[]> {
  return invoke<Offer[]>("schedulable");
}

/** Writes one into Task Scheduler, and says what Windows now holds. */
export function scheduleAutomation(trigger: Trigger): Promise<string> {
  return invoke<string>("schedule", { trigger });
}

export function unscheduleAutomation(name: string): Promise<void> {
  return invoke<void>("unschedule", { name });
}

/** The clock time an empty field means, so a new trigger starts somewhere. */
export const DEFAULT_TIME = "09:00";

/**
 * A `<input type="time">` value, as the schedule Rust takes.
 *
 * Returns null rather than a guess for anything that is not `HH:MM`. The
 * field can be empty while somebody is still typing in it, and a blank that
 * quietly became midnight is a trigger that fires at a time nobody chose.
 */
export function timeToWhen(value: string): When | null {
  const parts = /^(\d{1,2}):(\d{2})$/.exec(value.trim());
  if (!parts) return null;

  const hour = Number(parts[1]);
  const minute = Number(parts[2]);
  if (hour > 23 || minute > 59) return null;

  return { kind: "daily", hour, minute };
}

/** The same schedule, as the sentence the panel shows. Mirrors `When::said`. */
export function said(when: When): string {
  switch (when.kind) {
    case "daily":
      return `every day at ${String(when.hour).padStart(2, "0")}:${String(
        when.minute,
      ).padStart(2, "0")}`;
    case "atLogon":
      return "when you sign in";
    case "onUnlock":
      return "when you unlock this PC";
  }
}
