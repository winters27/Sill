/**
 * A conversation as a window draws it, and how an answer's parts become
 * blocks on screen.
 *
 * Rust records what happened in the order it happened: words, thinking and
 * tool steps, interleaved. The window draws consecutive steps as one
 * timeline, thinking as one fold and words as prose, so the only shaping done
 * here is grouping neighbours. No state, no decisions about the conversation.
 */

import type { AiAttached, AiPart, AiTurn } from "$lib/exthost/commands";

/** One turn as a window draws it. */
export interface Shown {
  /** `user` or `assistant`. */
  role: string;
  text: string;
  parts: AiPart[];
  attachments: AiAttached[];
}

export type StepPart = Extract<AiPart, { kind: "step" }>;
export type ThinkingPart = Extract<AiPart, { kind: "thinking" }>;

/**
 * One thing to draw.
 *
 * `at` is the index of the first part it came from, and is what a keyed
 * `{#each}` keys on. Keying on the block's position instead meant a step
 * arriving mid-answer shifted every later key, and a paragraph being written
 * was handed to a different element and started its reveal again.
 */
export type Block =
  | { kind: "text"; at: number; text: string }
  | { kind: "thinking"; at: number; text: string; ms?: number }
  | { kind: "steps"; at: number; steps: StepPart[] };

/** Consecutive steps become one timeline; everything else stands alone. */
export function groupParts(parts: AiPart[]): Block[] {
  const blocks: Block[] = [];

  parts.forEach((part, at) => {
    if (part.kind === "step") {
      const last = blocks[blocks.length - 1];
      if (last?.kind === "steps") {
        last.steps.push(part);
      } else {
        blocks.push({ kind: "steps", at, steps: [part] });
      }
      return;
    }

    if (part.kind === "thinking") {
      blocks.push({ kind: "thinking", at, text: part.text, ms: part.ms });
      return;
    }

    blocks.push({ kind: "text", at, text: part.text });
  });

  return blocks;
}

/** A turn from Rust, as the window draws it. */
export function fromTurn(turn: AiTurn): Shown {
  return {
    role: turn.role,
    text: turn.text,
    parts: turn.parts ?? [],
    attachments: turn.attachments ?? [],
  };
}

/** A question, as the window draws it the moment it is sent. */
export function fromQuestion(text: string, attachments: AiAttached[] = []): Shown {
  return { role: "user", text, parts: [], attachments };
}
