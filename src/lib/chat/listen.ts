/**
 * The events an answer arrives on, heard once.
 *
 * Two surfaces draw the same conversation and used to register the same five
 * listeners each, with the same bodies. One of them was going to grow an
 * event the other did not hear. Both call this now and hand it the turn they
 * hold; what differs between them is what happens once a turn is over, and
 * that is the one thing left to the caller.
 *
 * Rust emits to every window, so a launcher and a chat window both open both
 * hear the same stream. That is the design: one conversation, two views.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AiAsking, AiFinished, AiStep, AiUsed } from "$lib/exthost/commands";
import { said, settle, thought, used, using, type Live } from "./live";
import type { Shown } from "./parts";

export interface OnChat {
  /** The turn is over. `turn` is what arrived, or nothing if nothing did. */
  done(turn: Shown | null, finished: AiFinished): void;
  /** It went wrong. What arrived before it did is still a turn. */
  failed(turn: Shown | null, why: string): void;
}

/**
 * Hears everything about the turn in flight into `live`, and reports the end.
 *
 * Returns one function that forgets all of it.
 */
export async function listenToChat(live: Live, on: OnChat): Promise<UnlistenFn> {
  const heard = await Promise.all([
    listen<string>("sill://ai-said", ({ payload }) => said(live, payload)),
    listen<string>("sill://ai-thinking", ({ payload }) => thought(live, payload)),
    listen<AiStep>("sill://ai-using", ({ payload }) => using(live, payload)),
    listen<AiUsed>("sill://ai-used", ({ payload }) => used(live, payload)),
    listen<AiAsking>("sill://ai-asking", ({ payload }) => {
      live.asked = payload;
    }),
    listen<AiFinished>("sill://ai-done", ({ payload }) => {
      live.finished = payload;
      on.done(settle(live), payload);
    }),
    listen<string>("sill://ai-failed", ({ payload }) => {
      const turn = settle(live);
      live.trouble = payload;
      on.failed(turn, payload);
    }),
  ]);

  return () => {
    for (const forget of heard) forget();
  };
}
