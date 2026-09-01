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
import { ago, installs, shortRevision, weight } from "$lib/store";

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
