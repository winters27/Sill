/**
 * What a window does when Rust does not answer.
 *
 * Every call into Rust that a page draws from needs a fallback, because a pane
 * that throws part way through leaves the rest of the window unbuilt. The
 * fallback was never the problem. The problem was that it was all there was:
 * an empty list drawn where an answer belongs is indistinguishable from the
 * answer being empty, so a refused command read as "there are no collections",
 * "this machine has no drives", "nothing is set up to answer".
 *
 * Tauri denies a command to a window missing from `capabilities/default.json`
 * **silently**. Nothing throws in Rust, nothing reaches the log, and the page
 * renders perfectly. That is how the tray menu once shipped completely dead,
 * so an empty pane is more likely to be a permission than a fact.
 *
 * ## Two decisions, and the code says which one was made
 *
 * `orElse` keeps the fallback and reports the reason, for a failure that
 * leaves the interface saying something untrue. `silently` keeps the fallback
 * and says nothing, for a failure where the fallback is the honest answer.
 * Both are named so that the next person adding a wrapper has to choose rather
 * than reach for `.catch(() => [])`, which is the shape that chooses for them.
 * `scripts/verify-source.mjs` refuses that shape on anything chained to an
 * `invoke`.
 */
import { invoke } from "@tauri-apps/api/core";

/**
 * Which window is reporting.
 *
 * Each one withdraws what it last failed to read before asking again, and the
 * groups are kept apart so that opening settings is not the act that erases
 * the launcher's reports before anybody has read them.
 */
export type Surface = "launcher" | "settings" | "ask" | "capture";

/** Something Sill is quietly not doing, as the status surface holds it. */
export interface Trouble {
  id: string;
  message: string;
  /** The settings section holding the control it is about, when there is one. */
  section: string | null;
}

/**
 * A read that failed, kept usable and no longer kept quiet.
 *
 * The failure goes to Rust rather than being held here, because the tray and
 * the settings window both show it and neither of them is this module. The
 * caller still gets its fallback and still draws.
 *
 * `what` names the thing the way a sentence would, because Rust puts it in
 * one: "Sill could not read the collections in the clipboard history". The
 * optional `section` is the settings panel holding whatever the reader would
 * go and look at, so the band showing this can offer to take them there.
 *
 * Use this when the fallback would be believed. Use `silently` when it would
 * not.
 */
export function orElse<T>(
  surface: Surface,
  what: string,
  fallback: T,
  section = "",
): (reason: unknown) => T {
  return (reason) => {
    invoke("note_unreadable", {
      failed: { surface, what, reason: String(reason), section },
    }).catch((err) => {
      // The report itself was refused, which is the same denial one layer up.
      // The console is all that is left, and it is better than nothing.
      console.error("[sill] could not report that", what, "was unreadable", err);
    });

    return fallback;
  };
}

/**
 * A read that failed where the fallback is the truth anyway.
 *
 * Named rather than written as an empty arrow, so that choosing to say nothing
 * is a decision somebody made rather than the shape that was easiest to type.
 * The test for using it: would a person looking at the window be misled? If
 * the fallback is what they would have seen on a good day, or the failure is
 * already obvious where it happened, or nothing they could do would change it,
 * then there is nothing worth a sentence and the surface is better for staying
 * empty.
 */
export function silently<T>(fallback: T): (reason: unknown) => T {
  return () => fallback;
}

/**
 * Everything Sill is quietly not doing.
 *
 * Read by the settings window and kept current by `sill://status-changed`, so
 * a failure that happens while it is open appears without anybody reopening
 * it.
 */
export function statusTroubles(): Promise<Trouble[]> {
  return invoke<Trouble[]>("status_troubles").catch((err) => {
    // The one read that cannot report its own failure through the surface,
    // because the surface is what it failed to read. The console is the end
    // of the line here.
    console.error("[sill] could not read what is not working", err);
    return [];
  });
}

/**
 * Forgets what one window last failed to read, because it is about to ask
 * again.
 *
 * Called once before a window re-reads all of them rather than clearing each
 * on success, which would put an extra call on every read that worked. Failure
 * is the rare path and it is the one that should cost something.
 */
export function forgetUnreadable(surface: Surface): Promise<void> {
  return invoke<void>("forget_unreadable", { surface }).catch((err) => {
    console.error("[sill] could not clear what was last unreadable", err);
  });
}
