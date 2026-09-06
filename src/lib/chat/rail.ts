/**
 * The rail beside a conversation: what is listed, in what groups, and who
 * answers. Pure shaping of what Rust already handed over, so a test can read
 * what a row says.
 */

import type { AiConversation, AiReady } from "$lib/exthost/commands";

/** Seconds in a day, for the two labels that are about days. */
const DAY = 86_400;

export interface Group {
  label: "Today" | "Yesterday" | "Earlier";
  rows: AiConversation[];
}

/**
 * Conversations by the day they were last spoken to, newest first.
 *
 * `one.age` is how old it was when the list was fetched, so the moment it was
 * spoken to is `fetched - age`, read against a clock that ticks. Days are
 * counted from local midnight, because "yesterday" means the calendar day and
 * not the last twenty-four hours: something asked at 23:50 is yesterday's ten
 * minutes later.
 */
export function byDay(all: AiConversation[], fetched: number, now: number): Group[] {
  const midnight = new Date(now * 1000);
  midnight.setHours(0, 0, 0, 0);
  const today = Math.floor(midnight.getTime() / 1000);

  const groups: Group[] = [
    { label: "Today", rows: [] },
    { label: "Yesterday", rows: [] },
    { label: "Earlier", rows: [] },
  ];

  for (const one of [...all].sort((a, b) => a.age - b.age)) {
    const at = fetched - one.age;
    const which = at >= today ? 0 : at >= today - DAY ? 1 : 2;
    groups[which].rows.push(one);
  }

  return groups.filter((group) => group.rows.length > 0);
}

/**
 * The rows whose title contains what was typed.
 *
 * The same rule the launcher's list of conversations uses, so finding one
 * here and finding one there is one behaviour.
 */
export function narrow(all: AiConversation[], query: string): AiConversation[] {
  const wanted = query.trim().toLowerCase();
  if (!wanted) return all;
  return all.filter((one) => one.title.toLowerCase().includes(wanted));
}

/** Where the answer comes from, in three words. */
export function whereFrom(answersWith: AiReady | null): string {
  if (!answersWith?.ready) return "";
  switch (answersWith.kind) {
    case "local":
      return "on this PC";
    case "cli":
      return "through Claude Code";
    default:
      return "by key";
  }
}
