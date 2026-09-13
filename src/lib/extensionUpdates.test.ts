import { describe, expect, it } from "vitest";

import {
  NOTHING_BEHIND,
  asStanding,
  updatingDetail,
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

  /**
   * The reason travels with the failure.
   *
   * It used to be logged and dropped, so the launcher said a name and the one
   * sentence somebody could act on was in a file nobody was looking at.
   */
  it("says why it did not work, not just that it did not", () => {
    const said = updatingWords(
      standing({ failed: [{ title: "Brew", why: "npm is not beside the Node." }] }),
    );

    expect(said).toBe("Brew: npm is not beside the Node.");
  });

  it("still says something when there is no reason to give", () => {
    expect(updatingWords(standing({ failed: [{ title: "Brew", why: "" }] }))).toBe(
      "Brew could not be updated.",
    );
  });

  it("reports what is waiting for a decision", () => {
    expect(updatingWords(standing({ asking: ["GitHub"] }))).toBe(
      "GitHub asks for more than before.",
    );
  });

  /**
   * **Both, not whichever came first.**
   *
   * A batch really does end this way: measured on a real machine, one
   * extension failed for want of npm while another was parked because its new
   * version reaches something it was never granted. Reporting only the failure
   * left the parked one invisible, which is a working guard reading as nothing
   * having happened.
   */
  it("says both when one failed and another is waiting", () => {
    const said = updatingWords(
      standing({
        failed: [{ title: "Brew", why: "npm is not beside the Node." }],
        asking: ["GitHub"],
      }),
    );

    expect(said).toContain("Brew: npm is not beside the Node.");
    expect(said).toContain("GitHub asks for more than before.");
  });

  /** Several reasons will not fit, and picking one would be arbitrary. */
  it("names several failures without trying to give every reason", () => {
    const said = updatingWords(
      standing({
        failed: [
          { title: "Brew", why: "one thing" },
          { title: "Jira", why: "another thing" },
        ],
      }),
    );

    expect(said).toBe("Brew and Jira could not be updated.");
  });

  /**
   * The line is written to fit a row that ellipsises; the detail is the rest.
   *
   * Measured on a real machine: the first wording spent so much width on
   * "could not be updated" that the second half of a two-outcome batch was
   * cut off entirely, which is the half the message existed to add.
   */
  it("keeps the path and the fix in the detail rather than the line", () => {
    const why =
      "Sill's Node has no npm, so dependencies cannot be installed.\n" +
      "Looked beside C:/Sill/node.exe. Installing Node.js from nodejs.org includes npm.";
    const at = standing({ failed: [{ title: "Brew", why }] });

    expect(updatingWords(at)).toBe("Brew: Sill's Node has no npm, so dependencies cannot be installed.");
    expect(updatingDetail(at)).toContain("Installing Node.js from nodejs.org");
  });

  /** Nothing more to say than the line says, so there is no hover text. */
  it("has no detail when the reason is one line", () => {
    expect(updatingDetail(standing({ failed: [{ title: "Brew", why: "one line" }] }))).toBe("");
  });

  it("has no detail at rest", () => {
    expect(updatingDetail(standing())).toBe("");
  });

  /** Applying is happening now, so it wins over what an earlier batch left. */
  it("says what is happening rather than what already happened", () => {
    const said = updatingWords(
      standing({
        doing: { kind: "applying", title: "Linear", done: 1, total: 1 },
        failed: [{ title: "Brew", why: "one thing" }],
      }),
    );

    expect(said).toBe("Updating Linear");
  });
});
