/**
 * What the two surfaces say about an update, tested without drawing anything.
 *
 * The words are in `update.ts` rather than in the components precisely so this
 * can exist. Both tests below were written by breaking the thing first: the
 * chin one passes trivially if `chinLine` returns an object for every state,
 * and the settings one passes trivially if `updateWords` returns the same
 * sentence twice, so each asserts the distinction rather than the presence.
 */
import { describe, expect, it } from "vitest";

import { asState, chinLine, updateWords, NOTHING_KNOWN, type Progress } from "$lib/update";

/** Every state, so a new one added later fails to compile rather than slip. */
const EVERY: Progress[] = [
  { kind: "unknown" },
  { kind: "upToDate" },
  { kind: "available", version: "0.3.0", notes: null },
  { kind: "downloading", version: "0.3.0", percent: null },
  { kind: "downloading", version: "0.3.0", percent: 42 },
  { kind: "ready", version: "0.3.0" },
  { kind: "failed", why: "the server did not answer" },
];

describe("what the chin shows", () => {
  it("says nothing at all unless there is something to press", () => {
    // The whole point of the surface. A launcher that announces being current
    // has turned its quietest row into noise on every single summon.
    expect(chinLine({ kind: "unknown" })).toBeNull();
    expect(chinLine({ kind: "upToDate" })).toBeNull();
    expect(chinLine({ kind: "failed", why: "no network" })).toBeNull();
  });

  it("offers a button for an update and for one already downloaded", () => {
    // The button carries the whole message. A sentence beside it would say the
    // same thing twice into a row that has no width to spare, and prose is the
    // item down there set to give way, so both together arrive clipped.
    expect(chinLine({ kind: "available", version: "0.3.0", notes: null })).toEqual({
      words: null,
      button: "Update to 0.3.0",
    });
    expect(chinLine({ kind: "ready", version: "0.3.0" })).toEqual({
      words: null,
      button: "Restart for 0.3.0",
    });
  });

  it("offers nothing to press while the download is in flight", () => {
    // A second press would start a second download. There is nothing useful to
    // offer mid-flight, so the line speaks and the button is gone.
    const at42 = chinLine({ kind: "downloading", version: "0.3.0", percent: 42 });
    expect(at42?.button).toBeNull();
    expect(at42?.words).toContain("42%");

    // A server that never said how big it is, which is common enough that the
    // words have to work without a number rather than show "null%".
    const unknownSize = chinLine({ kind: "downloading", version: "0.3.0", percent: null });
    expect(unknownSize?.button).toBeNull();
    expect(unknownSize?.words).toBe("Updating to 0.3.0");
  });

  it("names the version in whichever half it does show", () => {
    // Otherwise the chin says "an update is ready" and the reader has to go to
    // settings to find out ready for what. Which half carries it depends on
    // whether there is anything to press, so this asks the line as a whole.
    for (const progress of EVERY) {
      const line = chinLine(progress);
      if (line) expect(`${line.words ?? ""}${line.button ?? ""}`).toContain("0.3.0");
    }
  });

  it("never draws the words and a button at the same time", () => {
    // The row is shared with the readings, Escape and the pill. Two items from
    // this one state is what overran it.
    for (const progress of EVERY) {
      const line = chinLine(progress);
      if (line) expect(line.words === null || line.button === null).toBe(true);
    }
  });
});

describe("what settings says", () => {
  it("answers for every state, including the ones the chin keeps quiet", () => {
    // The difference between the two surfaces. Somebody came here to ask, so
    // "up to date" and a failed check are both answers rather than noise.
    for (const progress of EVERY) {
      expect(updateWords(progress).length).toBeGreaterThan(0);
    }
    expect(updateWords({ kind: "upToDate" })).toBe("This is the newest version.");
    expect(updateWords({ kind: "unknown" })).toBe("Not checked yet.");
  });

  it("gives a different sentence to every state", () => {
    // The guard against a `default:` that answers for a state nobody wrote
    // words for. `RootList` shipped that bug for eleven kinds.
    const said = EVERY.map(updateWords);
    expect(new Set(said).size).toBe(EVERY.length);
  });

  it("repeats the reason a check failed, rather than hiding it", () => {
    // This is the only surface that says so at all, so dropping the reason
    // here means it is nowhere.
    expect(updateWords({ kind: "failed", why: "the server did not answer" })).toContain(
      "the server did not answer",
    );
  });

  it("never claims to be current when nothing has been checked", () => {
    // The exact untrue-interface failure the status surface exists to prevent.
    expect(updateWords({ kind: "unknown" })).not.toContain("newest");
  });
});

describe("what comes back from Rust", () => {
  it("falls back rather than handing on something that is not a state", () => {
    /*
     * The bug this was written for. `orElse` catches a rejection and nothing
     * else, so a command that resolves to nothing walked straight past it and
     * the chin read `.kind` off `undefined`, which took the launcher's whole
     * footer down. Tauri refuses a command missing from `capabilities/`
     * silently, so this is a production shape and not only a test one.
     */
    expect(asState(undefined)).toEqual(NOTHING_KNOWN);
    expect(asState(null)).toEqual(NOTHING_KNOWN);
    expect(asState({})).toEqual(NOTHING_KNOWN);
    expect(asState({ progress: null })).toEqual(NOTHING_KNOWN);
    expect(asState({ progress: {} })).toEqual(NOTHING_KNOWN);
    expect(asState("nonsense")).toEqual(NOTHING_KNOWN);
  });

  it("keeps a real answer intact", () => {
    // The other half. A guard that returned the fallback for everything would
    // pass the test above and make the feature do nothing at all.
    expect(
      asState({ progress: { kind: "upToDate" }, current: "0.2.0", checkedRecently: true }),
    ).toEqual({
      progress: { kind: "upToDate" },
      current: "0.2.0",
      checkedRecently: true,
    });
  });

  it("fills in what an older Rust might not send", () => {
    // The version and the freshness are additions; the progress is the answer.
    // A build that sends only the progress should still draw.
    expect(asState({ progress: { kind: "upToDate" } })).toEqual({
      progress: { kind: "upToDate" },
      current: "",
      checkedRecently: false,
    });
  });
});
