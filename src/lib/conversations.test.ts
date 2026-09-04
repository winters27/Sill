import { describe, expect, it } from "vitest";

import { conversationRows, saidAbout } from "./conversations";
import type { AiConversation } from "./exthost/commands";

function conversation(over: Partial<AiConversation> = {}): AiConversation {
  return {
    id: "c1",
    title: "What windows do I have open?",
    age: 30,
    replies: 2,
    open: false,
    ...over,
  } as AiConversation;
}

describe("what a conversation row says underneath the question", () => {
  it("says how long ago in the largest unit that still reads", () => {
    expect(saidAbout(conversation({ age: 30 }))).toContain("Just now");
    expect(saidAbout(conversation({ age: 60 }))).toContain("1 min ago");
    expect(saidAbout(conversation({ age: 3600 }))).toContain("1 hr ago");
    expect(saidAbout(conversation({ age: 86_400 }))).toContain("1 d ago");
  });

  it("counts one reply as a reply", () => {
    expect(saidAbout(conversation({ replies: 1 }))).toContain("1 reply");
    expect(saidAbout(conversation({ replies: 2 }))).toContain("2 replies");
  });

  /*
   * Saying which one is open is what stops the row offering to reopen
   * something that is already open.
   */
  it("says so when the conversation is the one already open", () => {
    expect(saidAbout(conversation({ open: true }))).toContain("open");
    expect(saidAbout(conversation({ open: false }))).not.toContain("open");
  });
});

describe("the list of what has been asked", () => {
  const all = [
    conversation({ id: "a", title: "What windows do I have open?" }),
    conversation({ id: "b", title: "Find the largest files" }),
  ];

  it("keeps the order it arrived in, which is when each was last spoken to", () => {
    expect(conversationRows(all, "").map((r) => r.entrypoint)).toEqual(["a", "b"]);
  });

  it("narrows on the question, ignoring case and surrounding space", () => {
    expect(conversationRows(all, "  LARGEST ").map((r) => r.entrypoint)).toEqual(["b"]);
  });

  /*
   * The kind decides what the action panel offers and what Enter does. A row
   * that arrived as `conversation` would be the single one the root list
   * offers back, which is reopened rather than resumed from this list, and
   * neither can hold an alias.
   */
  it("is a past conversation rather than the open one", () => {
    expect(conversationRows(all, "")[0]?.mode).toBe("past-conversation");
  });

  /*
   * A duplicate key in a keyed each blanks the whole list rather than drawing
   * twice, so an id that is not unique takes the view down with it.
   */
  it("gives every row an id of its own", () => {
    const rows = conversationRows(all, "");
    expect(new Set(rows.map((r) => r.id)).size).toBe(rows.length);
  });

  // The launcher resumes by entrypoint, so it has to be the conversation's id
  // rather than the row's.
  it("carries the conversation id where the launcher looks for it", () => {
    expect(conversationRows(all, "")[0]?.entrypoint).toBe("a");
  });
});
