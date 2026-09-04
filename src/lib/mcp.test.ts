import { describe, expect, it } from "vitest";

import { actionId, joined, split, type McpServer } from "./mcp";

/**
 * The one piece of logic on this side.
 *
 * Everything else about MCP is Rust's: what a server contributes, when one is
 * started, what happens when it does not answer. What the window owns is a
 * single field holding a command line, because that is how every MCP server on
 * the internet is written down, and Rust wants the program and its arguments
 * apart.
 */
describe("a command line typed into one field", () => {
  it("splits into the program and its arguments", () => {
    expect(split("npx -y @modelcontextprotocol/server-filesystem C:/Notes")).toEqual({
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-filesystem", "C:/Notes"],
    });
  });

  /**
   * The case that matters on Windows.
   *
   * `C:\Program Files\thing.exe` split on spaces is three programs, none of
   * which exist, and the failure arrives as "could not start C:\Program",
   * which reads as a corrupted setting rather than as a missing quote.
   */
  it("keeps a quoted path with a space in it whole", () => {
    expect(split('"C:\\Program Files\\notes\\server.exe" --stdio')).toEqual({
      command: "C:\\Program Files\\notes\\server.exe",
      args: ["--stdio"],
    });
  });

  it("handles single quotes the same way", () => {
    expect(split("node 'C:/my server/index.mjs'")).toEqual({
      command: "node",
      args: ["C:/my server/index.mjs"],
    });
  });

  /** A field somebody is still typing into is not a program with no name. */
  it("reads an empty line as nothing rather than as a blank program", () => {
    expect(split("   ")).toEqual({ command: "", args: [] });
  });

  it("does not mind extra spaces between words", () => {
    expect(split("  node    server.mjs  ")).toEqual({
      command: "node",
      args: ["server.mjs"],
    });
  });
});

describe("the same command line, back in the field", () => {
  /**
   * A round trip, which is what the field actually does: it shows `joined` and
   * hands what was typed to `split` on every keystroke. A pair that does not
   * round trip would rewrite somebody's line under their cursor.
   */
  it("comes back as what was typed", () => {
    for (const line of [
      "node server.mjs",
      "npx -y @modelcontextprotocol/server-filesystem C:/Notes",
      '"C:\\Program Files\\notes\\server.exe" --stdio',
    ]) {
      const parts = split(line);
      expect(split(joined({ ...parts, name: "x", actions: [] }))).toEqual(parts);
    }
  });

  it("puts the quotes back around a part with a space in it", () => {
    const server: McpServer = {
      name: "notes",
      command: "C:\\Program Files\\notes\\server.exe",
      args: ["--stdio"],
      actions: [],
    };

    expect(joined(server)).toBe('"C:\\Program Files\\notes\\server.exe" --stdio');
  });
});

/**
 * The id the panel shows, which mirrors `actions::mcp::PREFIX` and the format
 * beside it.
 *
 * It is what a keyboard shortcut in the Shortcuts panel refers to, and
 * somebody looking for it there has no other way to find out what it is
 * called. A spelling that drifted from Rust's would be an id that names
 * nothing, silently.
 */
describe("what Sill will call a declared tool", () => {
  it("is the server and the tool under the mcp namespace", () => {
    expect(
      actionId(
        { name: "notes", command: "node", args: [], actions: [] },
        { tool: "summarise", title: "Summarise", actsOn: ["file"], argument: "path" },
      ),
    ).toBe("mcp.notes.summarise");
  });
});
