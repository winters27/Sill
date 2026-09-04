/**
 * MCP servers, mirrored from `src-tauri/src/preferences.rs`.
 *
 * These are declarations and nothing else. A server in this list costs a few
 * strings until somebody runs one of its actions or presses Check, which is
 * what lets Rust build the action panel out of them without asking anybody
 * anything. Nothing on this side ever opens a connection.
 */
import { invoke } from "@tauri-apps/api/core";

/** One of a server's tools, as an action in the panel. */
export interface McpAction {
  /** The tool's own name, as the server lists it. */
  tool: string;
  /** What the panel shows. The tool's name when this is blank. */
  title: string;
  /** Sill's kinds, spelled the way an extension's `actionOn` spells them. */
  actsOn: string[];
  /** The tool argument the thing being acted on is passed as. */
  argument: string;
}

/** One MCP server, as the person described it. */
export interface McpServer {
  name: string;
  command: string;
  args: string[];
  actions: McpAction[];
}

export interface McpSettings {
  servers: McpServer[];
}

/** One tool a server said it has. */
export interface McpTool {
  name: string;
  description: string;
}

/**
 * Asks one server what it can do.
 *
 * **The only thing on this page that starts a program**, and it is a button.
 * Not called on mount, because opening the panel with five servers configured
 * must not start five of them.
 *
 * Sent the form as it is on screen rather than what was last saved, since the
 * whole reason to press it is to find out whether what was just typed works.
 */
export function mcpTools(server: McpServer): Promise<McpTool[]> {
  return invoke<McpTool[]>("mcp_tools", {
    name: server.name,
    command: server.command,
    args: server.args,
  });
}

/**
 * The kinds a tool may be declared against.
 *
 * The same words `actionOn` takes, and the same list `docs/extensions.md`
 * prints. Only the ones worth offering are here: an action on a calculator
 * answer or a store listing is a row nobody would put a server behind, and a
 * picker of twenty-six kinds is a picker nobody reads.
 */
export const KINDS: { value: string; label: string }[] = [
  { value: "file", label: "File" },
  { value: "folder", label: "Folder" },
  { value: "text", label: "Text" },
  { value: "clipboardEntry", label: "Clipboard entry" },
  { value: "url", label: "Web page" },
  { value: "application", label: "Application" },
  { value: "window", label: "Window" },
  { value: "script", label: "Script" },
];

/**
 * A command line as a person types it, split the way a shell would.
 *
 * The field is one line because that is how every MCP server on the internet
 * is written down, and Rust wants the program and its arguments apart. Quotes
 * are honoured, because a path with a space in it is the common case on
 * Windows and `C:\Program Files\thing.exe` split on spaces is three programs
 * that do not exist.
 */
export function split(line: string): { command: string; args: string[] } {
  const parts: string[] = [];
  let current = "";
  let quote = "";

  for (const character of line.trim()) {
    if (quote) {
      if (character === quote) quote = "";
      else current += character;
      continue;
    }

    if (character === '"' || character === "'") {
      quote = character;
      // A quoted empty string is still a word somebody wrote on purpose, so
      // the part is started here rather than only when a character arrives.
      continue;
    }

    if (character === " " || character === "\t") {
      if (current) parts.push(current);
      current = "";
      continue;
    }

    current += character;
  }

  if (current) parts.push(current);

  return { command: parts[0] ?? "", args: parts.slice(1) };
}

/** The same command line, back as one string for the field. */
export function joined(server: McpServer): string {
  return [server.command, ...server.args]
    .filter((part) => part.length > 0)
    .map((part) => (part.includes(" ") ? `"${part}"` : part))
    .join(" ");
}

/**
 * The id Sill will mint for a declared tool.
 *
 * Shown beside the row rather than computed only in Rust, because it is what
 * a keyboard shortcut in the Shortcuts panel refers to, and somebody looking
 * for it there has no other way to find out what it is called. Mirrors
 * `actions::mcp::PREFIX` and the format beside it.
 */
export function actionId(server: McpServer, action: McpAction): string {
  return `mcp.${server.name}.${action.tool}`;
}
