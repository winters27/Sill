/**
 * How the store writes numbers down.
 *
 * These four are all the logic the window owns for the store. Everything else
 * about it (filtering, ranking, joining against what is installed, deciding
 * what can run) happens in Rust and arrives ready to draw, which is why there
 * is so little here to test.
 *
 * They are worth testing anyway because each one has a boundary that reads
 * wrong rather than failing: a download count that says "1000k", a size that
 * says "0.0 MB", and a timestamp that says "in a moment" for something fetched
 * yesterday.
 */
import { describe, expect, test } from "vitest";
import { ago, installs, progressFraction, progressLine, shortRevision, weight } from "$lib/store";

describe("how many people have it", () => {
  test("a small number is itself", () => {
    expect(installs(0)).toBe("0");
    expect(installs(999)).toBe("999");
  });

  test("thousands keep one decimal, ten thousands drop it", () => {
    expect(installs(1_000)).toBe("1.0k");
    expect(installs(31_545)).toBe("32k");
    expect(installs(356_834)).toBe("357k");
  });

  test("a million reads as a million rather than as 1000k", () => {
    expect(installs(1_000_000)).toBe("1.0m");
    expect(installs(2_400_000)).toBe("2.4m");
  });
});

describe("what it weighs", () => {
  test("bytes are named, because an extension can be tiny", () => {
    expect(weight(0)).toBe("0 bytes");
    expect(weight(940)).toBe("940 bytes");
  });

  test("kilobytes are whole and megabytes keep a decimal", () => {
    expect(weight(161_829)).toBe("158 KB");
    expect(weight(3 * 1024 * 1024)).toBe("3.0 MB");
  });

  /* The boundary that would otherwise read "1024 KB". */
  test("exactly a megabyte is a megabyte", () => {
    expect(weight(1024 * 1024)).toBe("1.0 MB");
  });
});

describe("which version", () => {
  test("a commit is quoted at seven characters, the way one is quoted", () => {
    expect(shortRevision("6939fc298cd701b66a652b5bcc6d1c763252391e")).toBe("6939fc2");
  });

  /* A folder install records no revision, and slicing nothing is nothing
     rather than an exception. */
  test("no revision is no text", () => {
    expect(shortRevision("")).toBe("");
  });
});

describe("when the catalogue was fetched", () => {
  const now = 1_000_000;

  test("a fetch that just happened says so", () => {
    expect(ago(now, now)).toBe("just now");
    expect(ago(now - 60, now)).toBe("just now");
  });

  test("minutes, then hours, then days", () => {
    expect(ago(now - 600, now)).toBe("10 minutes ago");
    expect(ago(now - 3600, now)).toBe("an hour ago");
    expect(ago(now - 5 * 3600, now)).toBe("5 hours ago");
    expect(ago(now - 30 * 3600, now)).toBe("yesterday");
    expect(ago(now - 5 * 86400, now)).toBe("5 days ago");
  });

  /*
   * A clock that has gone backwards must not say the catalogue was fetched in
   * the future. It reads as fresh, which is the safe direction: the number
   * exists so a stale list cannot pretend to be current, and a negative one
   * would be a stale list claiming something stranger.
   */
  test("a fetch stamped in the future reads as just now", () => {
    expect(ago(now + 5000, now)).toBe("just now");
  });
});

describe("how far along an install is", () => {
  /*
   * The half that was silent. Fetching an extension is one request per file
   * and a large one is a hundred of them, so this is where the wait starts;
   * before it reported anything the screen said "Fetching" and then nothing
   * at all until npm began, and a slow network read as a launcher that had
   * stopped.
   */
  test("the download counts files and takes the first share of the bar", () => {
    expect(progressFraction({ stage: "fetching", done: 0, total: 40 })).toBe(0);
    expect(progressLine({ stage: "fetching", done: 12, total: 40 })).toBe(
      "Fetching 12 of 40 files",
    );

    const half = progressFraction({ stage: "fetching", done: 20, total: 40 });
    expect(half).toBeGreaterThan(0);
    expect(half, "the download alone must never fill the bar").toBeLessThan(0.5);
  });

  /*
   * One wait, not two. Building starts where fetching stopped, so the bar
   * carries on rather than emptying and filling a second time, which would
   * read as a second install.
   */
  test("building carries on from where the download left off, and reaches the end", () => {
    const fetched = progressFraction({ stage: "fetching", done: 40, total: 40 });
    const starting = progressFraction({ stage: "building", command: "a", done: 0, total: 4 });

    expect(starting).toBe(fetched);
    expect(progressFraction({ stage: "building", command: "d", done: 4, total: 4 })).toBe(1);
  });

  /*
   * npm and esbuild report lines and no position. A bar that guessed at their
   * share would move without meaning, so they answer null and the window holds
   * the last position it was given.
   */
  test("what cannot be measured says so rather than guessing", () => {
    expect(progressFraction({ stage: "dependencies", said: "added 40 packages" })).toBeNull();
    expect(progressFraction({ stage: "bundling", said: "index.js 40kb" })).toBeNull();
  });

  /** Nothing to count is not the same as nothing to say. */
  test("an empty fetch is a position rather than a division by zero", () => {
    expect(progressFraction({ stage: "fetching", done: 0, total: 0 })).toBe(0);
    expect(progressLine({ stage: "fetching", done: 0, total: 0 })).toBe("Fetching");
    expect(progressFraction({ stage: "building", command: "a", done: 0, total: 0 })).toBe(
      progressFraction({ stage: "fetching", done: 1, total: 1 }),
    );
  });
});
