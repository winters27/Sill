/**
 * What the panel sends, and what it makes of a time field.
 *
 * Two things are worth holding on this side. The command names and the shape
 * of the argument, because Rust names its parameters and a rename here is a
 * command that fails at runtime with nothing at build time saying so. And the
 * reading of the time field, which is the one place the frontend interprets
 * anything: a blank that quietly became midnight is a trigger firing at a
 * time nobody chose.
 *
 * Every decision that matters lives in `src-tauri/src/automation.rs` and is
 * tested there. This is the wire, not the rule.
 */
import { beforeEach, describe, expect, test, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const {
  listAutomations,
  said,
  schedulableActions,
  scheduleAutomation,
  timeToWhen,
  unscheduleAutomation,
} = await import("$lib/automations");

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
});

describe("what reaches Rust", () => {
  test("each call names the command Rust registered", async () => {
    invoke.mockResolvedValue([]);
    await listAutomations();
    await schedulableActions();

    expect(invoke.mock.calls.map(([name]) => name)).toEqual(["automations", "schedulable"]);
  });

  /*
   * The parameter names, not just the command name. Tauri matches the object's
   * keys against the Rust function's arguments, so `trigger` and `name` are
   * part of the contract and a rename on either side is a call that arrives
   * and does nothing.
   */
  test("a new trigger arrives under the name Rust takes", async () => {
    invoke.mockResolvedValue("Windows will run this every day at 09:00.");

    await scheduleAutomation({
      name: "Morning notes",
      action: "sill.copyPath",
      target: "C:/notes.txt",
      kind: null,
      argument: null,
      when: { kind: "daily", hour: 9, minute: 0 },
    });

    expect(invoke).toHaveBeenCalledWith("schedule", {
      trigger: expect.objectContaining({ name: "Morning notes", action: "sill.copyPath" }),
    });
  });

  test("removing one names the task rather than an index", async () => {
    await unscheduleAutomation("Morning notes");
    expect(invoke).toHaveBeenCalledWith("unschedule", { name: "Morning notes" });
  });
});

describe("reading the time field", () => {
  test("a real time becomes a daily schedule", () => {
    expect(timeToWhen("09:05")).toEqual({ kind: "daily", hour: 9, minute: 5 });
    expect(timeToWhen("23:59")).toEqual({ kind: "daily", hour: 23, minute: 59 });
    expect(timeToWhen(" 7:30 ")).toEqual({ kind: "daily", hour: 7, minute: 30 });
  });

  /*
   * Null rather than a guess, and the empty string is why. The field is empty
   * while somebody is still typing in it, and a blank read as midnight is a
   * trigger that fires at a time nobody chose and that nothing on screen
   * said would happen.
   */
  test("anything that is not a time is refused rather than guessed", () => {
    expect(timeToWhen("")).toBeNull();
    expect(timeToWhen("nine")).toBeNull();
    expect(timeToWhen("24:00")).toBeNull();
    expect(timeToWhen("09:60")).toBeNull();
    expect(timeToWhen("09:00:00")).toBeNull();
  });
});

/*
 * The sentence the form shows before anything is written down has to be the
 * sentence Rust puts in the task's description, or the panel is promising one
 * thing and Windows is holding another. Mirrors `When::said`.
 */
test("the schedule reads the same on both sides", () => {
  expect(said({ kind: "daily", hour: 9, minute: 5 })).toBe("every day at 09:05");
  expect(said({ kind: "atLogon" })).toBe("when you sign in");
  expect(said({ kind: "onUnlock" })).toBe("when you unlock this PC");
});
