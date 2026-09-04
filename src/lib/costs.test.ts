import { describe, expect, it } from "vitest";
import {
  describeRunning,
  memoryBytes,
  showBytes,
  showMs,
  typicalMs,
  verdict,
  type CostRow,
} from "./costs";
import type { ExtensionCost, RunningCommand } from "./store";

function cost(
  extension: string,
  coldMs: number | null,
  warmMs: number | null,
  heldMb: number | null = null,
): ExtensionCost {
  return {
    extension,
    coldMs,
    coldOpens: coldMs === null ? 0 : 1,
    warmMs,
    warmOpens: warmMs === null ? 0 : 1,
    heldBytes: heldMb === null ? null : heldMb * 1024 * 1024,
    running: [],
  };
}

function row(
  extension: string,
  title: string,
  coldMs: number | null,
  warmMs: number | null,
  heldMb: number | null = null,
): CostRow {
  return { extension, title, cost: cost(extension, coldMs, warmMs, heldMb) };
}

function running(command: string, over: Partial<RunningCommand> = {}): RunningCommand {
  return {
    session: "s",
    extension: "emoji",
    command,
    heapBytes: 63 * 1024 * 1024,
    heapLimitBytes: 512 * 1024 * 1024,
    corePercent: 10,
    answering: true,
    ...over,
  };
}

describe("naming the expensive one", () => {
  /*
   * The whole feature in one assertion, on the axis the real extensions
   * actually differ on.
   *
   * Measured across the five the view gate draws: openings span 36 ms to
   * 114 ms and memory spans 11 MB to 63 MB. Comparing only openings would have
   * said "nothing stands out" about the one holding six times what the rest do.
   */
  it("names the extension holding the most memory, and what it beat", () => {
    const said = verdict([
      row("uuid-generator", "UUID Generator", 526, 36, 11),
      row("emoji", "Emoji Search", 516, 74, 63),
    ]);

    expect(said).toContain("Emoji Search");
    expect(said).toContain("63 MB");
    expect(said).toContain("UUID Generator");
    expect(said).toContain("11 MB");
  });

  /*
   * A command burning a core is said ahead of everything else.
   *
   * It is the only thing on the screen that is costing something at the moment
   * somebody is reading about it, and the host's own watchdog will not stop it
   * for another half minute. Without this the panel found the extension using
   * 99% of a core, printed the fact in a line under its row, and led with
   * "nothing here stands out".
   */
  it("leads with a command that is using a processor core", () => {
    const wedged = row("kill-process", "Kill Process", 531, 50, 37);
    wedged.cost.running = [
      running("Kill Process", {
        extension: "kill-process",
        answering: false,
        heapBytes: null,
        corePercent: 99.4,
      }),
    ];

    const said = verdict([row("emoji", "Emoji Search", 516, 74, 63), wedged]);

    expect(said).toContain("Kill Process");
    expect(said).toContain("99%");
    expect(said).toContain("not answering");
  });

  /// With nothing to compare on memory, the slow one is still worth naming.
  it("falls back to the time to open when no memory is known", () => {
    const said = verdict([
      row("emoji", "Emoji Search", 528, 780),
      row("uuid-generator", "UUID Generator", 522, 48),
    ]);

    expect(said).toContain("Emoji Search");
    expect(said).toContain("780 ms");
    expect(said).toContain("UUID Generator");
    expect(said).toContain("48 ms");
  });

  /*
   * Memory is read live when a command is loaded, and from the last close
   * otherwise. The live figure wins because it is now.
   */
  it("prefers what a running command is holding over what it held last time", () => {
    const cheap = row("emoji", "Emoji Search", 516, 74, 11);
    cheap.cost.running = [
      {
        session: "s",
        extension: "emoji",
        command: "Search Emoji",
        heapBytes: 63 * 1024 * 1024,
        heapLimitBytes: 512 * 1024 * 1024,
        corePercent: 1,
        answering: true,
      },
    ];

    expect(memoryBytes(cheap.cost)).toBe(63 * 1024 * 1024);
  });

  /*
   * Four milliseconds is the machine, not the extension.
   *
   * A screen that names a culprit on that evidence names a different one every
   * time it is opened, and somebody acts on it by removing an extension that
   * was fine.
   */
  it("refuses to name one when they are all much the same", () => {
    const said = verdict([
      row("emoji", "Emoji Search", 528, 52),
      row("uuid-generator", "UUID Generator", 522, 48),
    ]);

    expect(said).toContain("Nothing here stands out");
    expect(said).toContain("Emoji Search");
  });

  /// One extension is not a comparison, and saying it is the slowest is true
  /// and useless.
  it("says there is nothing to compare against when only one was opened", () => {
    const said = verdict([row("emoji", "Emoji Search", 528, 780)]);

    expect(said).toContain("Nothing else has been opened");
  });

  it("says nothing at all when nothing has been opened", () => {
    expect(verdict([])).toBe("");
  });

  /*
   * An extension only ever opened cold is still in the comparison.
   *
   * Leaving it out would hide an extension somebody opened once, which is the
   * most likely state for the one they have just installed and are wondering
   * about.
   */
  it("compares an extension opened only cold on its cold figure", () => {
    expect(typicalMs(cost("hacker-news", 1400, null))).toBe(1400);
    expect(typicalMs(cost("emoji", 528, 780))).toBe(780);
    expect(typicalMs(cost("never", null, null))).toBe(null);
  });
});

describe("saying a number the way a person would", () => {
  it("keeps milliseconds under a second and seconds above one", () => {
    expect(showMs(48)).toBe("48 ms");
    expect(showMs(999)).toBe("999 ms");
    expect(showMs(1400)).toBe("1.4 s");
  });

  it("says a figure is missing rather than showing a zero", () => {
    expect(showMs(null)).toBe("not measured");
    expect(showBytes(null)).toBe("not measured");
  });

  it("rounds memory to whole megabytes", () => {
    expect(showBytes(63 * 1024 * 1024)).toBe("63 MB");
  });
});

describe("what one running command is doing", () => {
  it("says what it holds and what it is using", () => {
    const said = describeRunning(running("Search Emoji"));

    expect(said).toContain("Search Emoji");
    expect(said).toContain("63 MB");
    expect(said).toContain("10% of a processor core");
  });

  /*
   * The case the panel exists for.
   *
   * An extension in a loop cannot answer how much memory it is holding,
   * because answering needs the event loop it is holding. The share of a core
   * is measured from outside the worker and still arrives, so the two together
   * describe it exactly.
   */
  it("says a command is not answering rather than leaving it blank", () => {
    const said = describeRunning(
      running("Never Draws", { answering: false, heapBytes: null, corePercent: 99.4 }),
    );

    expect(said).toContain("is not answering");
    expect(said).toContain("99% of a processor core");
  });
});
