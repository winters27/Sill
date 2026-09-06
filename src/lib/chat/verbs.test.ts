import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { describe as say, VERBS, verbFor } from "./verbs";

/**
 * The tool names, read out of the Rust catalogue rather than repeated here.
 *
 * `verify:source` checks the same thing; this is the copy that fails in the
 * unit suite, so somebody adding a tool finds out before the gate does.
 */
function catalogue(): string[] {
  const rust = readFileSync(
    resolve(import.meta.dirname, "../../../src-tauri/src/ai/tools.rs"),
    "utf8",
  );
  const start = rust.indexOf("CATALOGUE");
  const body = rust.slice(start);
  return [...body.matchAll(/^\s*name: "([a-z_]+)",/gm)].map((found) => found[1]);
}

describe("the words for each tool", () => {
  it("has words for every tool the model can reach, and no others", () => {
    const tools = catalogue();
    expect(tools.length).toBeGreaterThan(5);

    for (const tool of tools) {
      expect(VERBS, `${tool} has no words`).toHaveProperty(tool);
    }
    for (const named of Object.keys(VERBS)) {
      expect(tools, `${named} is not a tool`).toContain(named);
    }
  });

  it("speaks in the present while a step runs and the past once it is over", () => {
    const step = { kind: "step" as const, id: "a", tool: "read_file", subject: "notes.txt" };
    expect(say(step, true)).toBe("Reading notes.txt");
    expect(say(step, false)).toBe("Read notes.txt");
  });

  it("quotes what somebody searched for and leaves a path bare", () => {
    const search = { kind: "step" as const, id: "a", tool: "search_sill", subject: "chrome" };
    expect(say(search, false)).toBe("Searched this machine for \u201cchrome\u201d");

    const folder = { kind: "step" as const, id: "b", tool: "list_directory", subject: "C:\\x" };
    expect(say(folder, false)).toBe("Looked inside C:\\x");
  });

  it("says nothing after a verb that has no subject", () => {
    const step = { kind: "step" as const, id: "a", tool: "list_windows", subject: "  " };
    expect(say(step, false)).toBe("Looked at what is open");
  });

  it("reads an unknown tool by its name rather than by nothing", () => {
    expect(verbFor("later_tool").doing).toBe("later_tool");
    expect(say({ kind: "step", id: "a", tool: "later_tool", subject: "" }, true)).toBe(
      "later_tool",
    );
  });
});
