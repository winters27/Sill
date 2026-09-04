/**
 * Turning what an extension costs into something a person can act on.
 *
 * Rust measures and Rust orders: the list arrives with the dearest extension
 * first, because deciding which of two things is worse is a comparison and
 * comparisons belong there. What is left here is the sentence, and the
 * sentence is most of the value.
 *
 * The panel this feeds exists for one question. Somebody installed four
 * extensions and their launcher got slower, and until now the honest answer
 * was a shrug. **The answer is a name, not a dashboard**, so the reading at
 * the top says which one and by how much, and the table underneath is there
 * for whoever wants to check it.
 */
import type { ExtensionCost, RunningCommand } from "$lib/store";

/** One extension's cost, with the name a person calls it by. */
export interface CostRow {
  extension: string;
  title: string;
  cost: ExtensionCost;
}

/**
 * How much slower one extension has to be before it is worth naming.
 *
 * Twice the next one. Below that the difference is the machine: two openings
 * of the same command a minute apart differ by tens of milliseconds on a
 * laptop that is doing anything else, and a screen that names a culprit on
 * that evidence is a screen that blames a different extension every time it
 * is opened.
 */
const STANDS_OUT = 2;

/**
 * The share of a processor core past which a loaded command is not idling.
 *
 * Half of one. The busiest of the five real extensions Sill tests against
 * reads under 1% once it has drawn, and the host does not stop a command until
 * it has held a whole core for thirty seconds, so this sits in the gap: high
 * enough that nothing ordinary reaches it, low enough to be said before the
 * watchdog acts.
 */
const BUSY = 50;

/**
 * The figure two extensions are compared on.
 *
 * Warm where there is one. A cold open is mostly Node starting, which every
 * extension pays the same and none of them causes; what one extension does
 * and another does not is the work after that. An extension only ever opened
 * cold is compared on that, because leaving it out of the comparison is
 * worse than comparing it on the wrong half.
 *
 * This mirrors `Opening::typical_us` in `timing.rs`. It is duplicated rather
 * than sent because the panel needs it per row for the sentence, and shipping
 * a fourth number that is always one of the other two would be a field that
 * can disagree with them.
 */
export function typicalMs(cost: ExtensionCost): number | null {
  return cost.warmMs ?? cost.coldMs;
}

/** A duration, in the unit somebody would say it in. */
export function showMs(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "not measured";
  // Under a second reads as milliseconds, because that is how the difference
  // between 40 and 900 stays visible. Above it, "1.4 s" is what a person says.
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
}

/**
 * An amount of memory, in the unit somebody would say it in.
 *
 * "Not measured" rather than a dash or a zero. An extension nobody has opened
 * this run has no figure, and a zero in that column would read as an extension
 * that costs nothing, which is the opposite of what is known about it.
 */
export function showBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "not measured";
  return `${Math.round(bytes / 1024 / 1024)} MB`;
}

/**
 * What one running command is doing, in a line.
 *
 * A command that will not answer is the interesting case and it is said
 * first, because "not answering while using a whole processor core" is the
 * signature of an extension in a loop and it is the thing this panel exists
 * to catch. The share of a core is measured from outside the worker, so it
 * arrives even when nothing else does.
 */
export function describeRunning(one: RunningCommand): string {
  const core = `${Math.round(one.corePercent)}% of a processor core`;

  if (!one.answering) {
    return `${one.command} is not answering. It is using ${core}.`;
  }

  return `${one.command} is holding ${showBytes(one.heapBytes)} and using ${core}.`;
}

/**
 * The most memory this extension is known to have held, in bytes.
 *
 * What is running now if anything is, and otherwise the most it was holding
 * when it was last closed. The live reading first because it is now, and the
 * closing one at all because a launcher has one command loaded at a time: a
 * screen showing only what is running shows one number, and one number is not
 * a comparison.
 */
export function memoryBytes(cost: ExtensionCost): number | null {
  const live = cost.running
    .map((one) => one.heapBytes)
    .filter((bytes): bytes is number => bytes !== null);

  if (live.length > 0) return Math.max(...live);

  return cost.heldBytes;
}

/**
 * The one line worth reading, or nothing when there is nothing to say.
 *
 * **Memory is asked about first, and that came out of measuring rather than
 * out of taste.** Across the five real extensions Sill's view gate draws, the
 * time to open spans 36 ms to 114 ms, most of which is one of them waiting on
 * a network; the memory they hold spans 11 MB to 63 MB. An extension is rarely
 * slow and often heavy, so a screen that only compared openings would have
 * said "nothing stands out" about the one holding six times what the others do.
 *
 * The last case is the one that keeps this honest. Several extensions that are
 * all much the same get no culprit named, because naming a winner by four
 * milliseconds sends somebody to remove an extension that was fine.
 */
export function verdict(rows: CostRow[]): string {
  /*
   * A command using a processor core is said first, ahead of everything.
   *
   * It is the worst thing this screen can find and the only one that is
   * costing something at the moment somebody is reading about it. A loaded
   * command that has finished drawing should be asleep; one that is not is
   * either doing work nobody asked for or stuck in a loop, and the host's own
   * watchdog will not stop it for another half minute.
   *
   * Well above anything ordinary: the busiest of the five real extensions
   * reads under 1% once it has settled.
   */
  const busy = rows
    .flatMap((row) => row.cost.running.map((one) => ({ row, one })))
    .filter(({ one }) => one.corePercent >= BUSY)
    .sort((a, b) => b.one.corePercent - a.one.corePercent);

  if (busy.length > 0) {
    const { row, one } = busy[0];
    const quiet = one.answering ? "" : ", and it is not answering";

    return `${row.title} is using ${Math.round(one.corePercent)}% of a processor core right now${quiet}. A command that has finished drawing should be asleep.`;
  }

  const byMemory = rows
    .map((row) => ({ row, bytes: memoryBytes(row.cost) }))
    .filter((one): one is { row: CostRow; bytes: number } => one.bytes !== null)
    .sort((a, b) => b.bytes - a.bytes);

  if (byMemory.length > 1 && byMemory[0].bytes >= byMemory[1].bytes * STANDS_OUT) {
    return `${byMemory[0].row.title} is the expensive one: ${showBytes(byMemory[0].bytes)} of memory, against ${showBytes(byMemory[1].bytes)} for ${byMemory[1].row.title}.`;
  }

  const measured = rows.filter((row) => typicalMs(row.cost) !== null);

  if (measured.length === 0) return "";

  const worst = measured[0];
  const worstMs = typicalMs(worst.cost) ?? 0;

  if (measured.length === 1) {
    return `${worst.title} took ${showMs(worstMs)} to open. Nothing else has been opened this run to compare it with.`;
  }

  const next = measured[1];
  const nextMs = typicalMs(next.cost) ?? 0;

  if (nextMs > 0 && worstMs < nextMs * STANDS_OUT) {
    return `Nothing here stands out. The slowest to open is ${worst.title}, at ${showMs(worstMs)}.`;
  }

  return `${worst.title} is the slow one: ${showMs(worstMs)} to open, against ${showMs(nextMs)} for ${next.title}.`;
}
