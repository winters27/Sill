/**
 * The turn being written, as it arrives.
 *
 * A plain object mutated by plain functions, so it can be proved without a
 * window and so both surfaces hold it the same way: each keeps one in
 * `$state`, and a mutation through these functions is one Svelte sees.
 *
 * This is a copy for drawing. Rust records the same parts as they happen and
 * commits the turn on done; what is here exists so the first words are on
 * screen while the rest is still being written.
 */

import type { AiAsking, AiFinished, AiPart, AiSpent, AiStep, AiUsed } from "$lib/exthost/commands";
import type { Shown } from "./parts";

export interface Live {
  /** What has arrived so far, in order. */
  parts: AiPart[];
  /** Whether a question is in flight. */
  asking: boolean;
  /** What the model wants to do, while nobody has said yes or no. */
  asked: AiAsking | null;
  /** What went wrong, in Rust's words. Empty when nothing did. */
  trouble: string;
  /** What the last turn cost, once it was over. */
  finished: AiFinished | null;
  /**
   * What the conversation has cost so far, as Rust last said.
   *
   * Null until Rust has been asked or a turn has finished. A copy for
   * drawing: Rust holds the total and hands it over on every done event.
   */
  spent: AiSpent | null;
  /**
   * Pieces of the answer in flight, and when the first one came.
   *
   * A stand-in for the token count Rust cannot know until the service says
   * at the end: a streamed chunk is usually a token, so the pill can tick
   * while the answer arrives and settle on the real number when it is over.
   * Reset with the turn, and never read once `spent` has the truth.
   */
  streamed: number;
  /** `performance.now()` at the first piece; zero before one has come. */
  streamBegan: number;
}

export function fresh(): Live {
  return {
    parts: [],
    asking: false,
    asked: null,
    trouble: "",
    finished: null,
    spent: null,
    streamed: 0,
    streamBegan: 0,
  };
}

/** Every word so far, which is what the answer is. */
export function textOf(parts: AiPart[]): string {
  return parts
    .filter((part) => part.kind === "text")
    .map((part) => part.text)
    .join("");
}

/** A question has gone; the answer is on its way. */
export function begin(live: Live) {
  live.parts = [];
  live.asking = true;
  live.asked = null;
  live.trouble = "";
  live.finished = null;
  live.streamed = 0;
  live.streamBegan = 0;
}

/** One more piece has come, for the count that ticks while it arrives. */
function stream(live: Live) {
  if (!live.streamBegan) live.streamBegan = performance.now();
  live.streamed += 1;
}

/**
 * When the thinking now being written began, per turn.
 *
 * Rust stamps the stored part with how long it thought; this stamps the one
 * on screen, which is drawn before Rust's copy is ever read back. Kept
 * beside the turn rather than on the part, so the part stays the shape Rust
 * writes.
 */
const thinkingBegan = new WeakMap<Live, number>();

/** Stamps the thinking part being written with how long it took. */
function closeThinking(live: Live) {
  const began = thinkingBegan.get(live);
  if (began === undefined) return;
  thinkingBegan.delete(live);

  const last = live.parts[live.parts.length - 1];
  if (last?.kind === "thinking") last.ms = Math.round(performance.now() - began);
}

/** Words arriving. Whitespace on its own starts nothing. */
export function said(live: Live, piece: string) {
  stream(live);
  closeThinking(live);
  const last = live.parts[live.parts.length - 1];
  if (last?.kind === "text") {
    last.text += piece;
  } else if (piece.trim()) {
    live.parts.push({ kind: "text", text: piece });
  }
}

/** Thinking arriving, before the words. */
export function thought(live: Live, piece: string) {
  stream(live);
  const last = live.parts[live.parts.length - 1];
  if (last?.kind === "thinking") {
    last.text += piece;
  } else {
    thinkingBegan.set(live, performance.now());
    live.parts.push({ kind: "thinking", text: piece });
  }
}

/** A tool being reached for. */
export function using(live: Live, step: AiStep) {
  closeThinking(live);
  live.parts.push({ kind: "step", id: step.id, tool: step.tool, subject: step.subject });
}

/** That tool finished. A result for a step nobody recorded is ignored. */
export function used(live: Live, done: AiUsed) {
  for (let at = live.parts.length - 1; at >= 0; at -= 1) {
    const part = live.parts[at];
    if (part.kind === "step" && part.id === done.id) {
      part.ok = done.ok;
      return;
    }
  }
}

/**
 * The turn is over, one way or the other.
 *
 * What arrived becomes a turn, even after a failure: half an answer is often
 * enough to see what went wrong. Nothing at all becomes nothing.
 */
export function settle(live: Live): Shown | null {
  closeThinking(live);
  const parts = live.parts;
  const text = textOf(parts);

  live.parts = [];
  live.asking = false;
  live.streamed = 0;
  live.streamBegan = 0;

  if (parts.length === 0) return null;
  return { role: "assistant", text, parts, attachments: [] };
}

/**
 * Everything of the turn in flight, and the card with it.
 *
 * The total goes too: this is what a resumed or fresh conversation calls,
 * and the number belongs to the one just left. Whoever resumes asks Rust
 * for the right one.
 */
export function reset(live: Live) {
  live.parts = [];
  live.asking = false;
  live.asked = null;
  live.trouble = "";
  live.finished = null;
  live.spent = null;
  live.streamed = 0;
  live.streamBegan = 0;
}
