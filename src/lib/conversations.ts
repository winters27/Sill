/**
 * The list of everything that has been asked, as rows.
 *
 * ## Why the shaping is here and not in Rust
 *
 * These never go through search. The list is short, it arrives already ordered
 * by when each was last spoken to, and filtering it is a substring test on the
 * question, so sending a query to Rust for it would be a round trip that
 * answers something the window already knows. Rule 1's line is drawn at
 * computation, and this is not computation.
 *
 * ## Why it is not in `+page.svelte`
 *
 * It was, and nothing could call it: the row's shape decides what the action
 * panel offers, what `openSelected` reopens and what Delete forgets, and none
 * of those could be checked without a window. What a row says underneath the
 * question is the same story: a conversation that is already open must not
 * offer to be reopened, and that is a sentence a test should be able to read.
 */
import type { AiConversation, RankedCommand } from "$lib/exthost/commands";

/** What a conversation row says underneath the question. */
export function saidAbout(one: AiConversation): string {
  const when =
    one.age < 60
      ? "Just now"
      : one.age < 3600
        ? `${Math.floor(one.age / 60)} min ago`
        : one.age < 86_400
          ? `${Math.floor(one.age / 3600)} hr ago`
          : `${Math.floor(one.age / 86_400)} d ago`;

  const replies = `${one.replies} ${one.replies === 1 ? "reply" : "replies"}`;

  // Saying which one is open stops the row offering to reopen something that
  // is already open.
  return one.open ? `${when} · ${replies} · open` : `${when} · ${replies}`;
}

/**
 * Every conversation, narrowed by what is typed.
 *
 * `past-conversation` rather than `conversation`, which is the single row the
 * root list offers back: they behave differently on Enter and neither can hold
 * an alias, so they are two kinds rather than one wearing two hats.
 */
export function conversationRows(all: AiConversation[], query: string): RankedCommand[] {
  const wanted = query.trim().toLowerCase();

  return all
    .filter((one) => !wanted || one.title.toLowerCase().includes(wanted))
    .map((one) => ({
      id: `chat-row:${one.id}`,
      extension: "sill",
      extensionTitle: "Conversations",
      command: "conversation",
      title: one.title,
      subtitle: saidAbout(one),
      mode: "past-conversation" as const,
      // Not a switch, and the row shape wants to be told.
      toggle: undefined,
      entrypoint: one.id,
      panel: "ai",
      score: 0,
      matched: [],
    }));
}
