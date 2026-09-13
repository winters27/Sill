import { describe, expect, it } from "vitest";

import {
  NOTHING_BEHIND,
  asStanding,
  updatingWords,
  type Standing,
} from "./extensionUpdates";

/** A standing with only the fields a case is about. */
function standing(over: Partial<Standing> = {}): Standing {
  return { ...NOTHING_BEHIND, ...over };
}

describe("what came back from Rust", () => {
  /**
   * The failure this exists for.
   *
   * Tauri refuses a command missing from `capabilities/` **silently**, and a
   * denied command resolves rather than throwing, so neither a `.catch` nor
   * the type parameter catches it. Reading `.length` off `undefined` here
   * would take the root list down.
   */
  it("answers with nothing behind when the reply is not the shape it claims", () => {
    for (const nonsense of [null, undefined, {}, 42, "yes", { behind: "no" }]) {
      expect(asStanding(nonsense)).toEqual(NOTHING_BEHIND);
    }
  });

  it("keeps what is there and fills in what is not", () => {
    const said = asStanding({ behind: [], checkedAt: 17 });

    expect(said.checkedAt).toBe(17);
    expect(said.doing).toEqual({ kind: "nothing" });
    expect(said.asking).toEqual([]);
    expect(said.failed).toEqual([]);
  });

  /** A state Rust could send that this build has never heard of. */
  it("treats a doing it cannot read as nothing happening", () => {
    expect(asStanding({ behind: [], doing: { what: "?" } }).doing).toEqual({
      kind: "nothing",
    });
  });
});

describe("the line shown while updates are being applied", () => {
  it("says nothing at rest", () => {
    expect(updatingWords(standing())).toBeNull();
  });

  /**
   * A check that finds nothing must leave no trace. This is the state the
   * launcher is in for a second on most summons, and a line about it would be
   * an interruption with nothing to press.
   */
  it("says nothing while it is only looking", () => {
    expect(updatingWords(standing({ doing: { kind: "checking" } }))).toBeNull();
  });

  it("names the one being applied", () => {
    const said = updatingWords(
      standing({ doing: { kind: "applying", title: "Brew", done: 1, total: 1 } }),
    );

    expect(said).toBe("Updating Brew");
  });

  /** With more than one there is a position worth knowing. */
  it("counts the way through when there is more than one", () => {
    const said = updatingWords(
      standing({ doing: { kind: "applying", title: "Jira", done: 2, total: 3 } }),
    );

    expect(said).toBe("Updating Jira, 2 of 3");
  });

  it("reports what did not work once it is over", () => {
    expect(updatingWords(standing({ failed: ["Brew"] }))).toBe(
      "Brew could not be updated",
    );
  });

  it("reports what is waiting for a decision", () => {
    expect(updatingWords(standing({ asking: ["GitHub"] }))).toBe(
      "GitHub asks for more than before, so it is waiting for you",
    );
  });

  /**
   * A failure is the one somebody has to act on, so it wins the single line
   * over an extension that is merely waiting.
   */
  it("puts a failure ahead of something waiting", () => {
    const said = updatingWords(standing({ failed: ["Brew"], asking: ["GitHub"] }));

    expect(said).toBe("Brew could not be updated");
  });

  it("says a list the way a person would", () => {
    expect(updatingWords(standing({ failed: ["Brew", "Jira"] }))).toBe(
      "Brew and Jira could not be updated",
    );
    expect(updatingWords(standing({ failed: ["Brew", "Jira", "Slack"] }))).toBe(
      "Brew, Jira and Slack could not be updated",
    );
  });

  /** Applying is happening now, so it wins over what an earlier batch left. */
  it("says what is happening rather than what already happened", () => {
    const said = updatingWords(
      standing({
        doing: { kind: "applying", title: "Linear", done: 1, total: 1 },
        failed: ["Brew"],
      }),
    );

    expect(said).toBe("Updating Linear");
  });
});
