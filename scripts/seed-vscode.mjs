/**
 * A VS Code state database for the gate to read, so the gate reads the same
 * one everywhere.
 *
 * `visual-studio-code-recent-projects` opens
 * `%APPDATA%/Code/User/globalStorage/state.vscdb` and lists what it finds. On
 * a developer's machine that file exists and the extension draws a list; on a
 * build agent with no VS Code it does not, the extension throws before
 * rendering anything, and the gate reported `root view is <undefined>` and
 * zero of everything.
 *
 * That looked exactly like the timing bug next door and was not. It is worse
 * than a timing bug: an assertion whose answer depends on what happens to be
 * installed is not an assertion, and it had been quietly passing here and
 * failing there for as long as the line has existed.
 *
 * So the gate builds the file it is about to read. `APPDATA` is what the
 * extension resolves the path from (`db.ts:322`), so pointing that at a
 * directory this wrote makes the run identical on both machines, and makes the
 * row count something the gate is entitled to assert exactly rather than
 * "at least".
 *
 * Usage: `node scripts/seed-vscode.mjs <dir>`, then run the extension with
 * `APPDATA=<dir>`.
 */
import { DatabaseSync } from "node:sqlite";
import { mkdirSync, rmSync } from "node:fs";
import path from "node:path";

const root = process.argv[2];
if (!root) {
  console.error("usage: node scripts/seed-vscode.mjs <appdata-dir>");
  process.exit(1);
}

// The exact layout `getGlobalStorageDirectory` builds: <APPDATA>/<build>/User/
// globalStorage, where the build name is the "Code" preference default.
const storage = path.join(root, "Code", "User", "globalStorage");
mkdirSync(storage, { recursive: true });

const file = path.join(storage, "state.vscdb");
// Rebuilt every run. A database left over from a previous gate would make this
// pass for a reason nobody chose.
rmSync(file, { force: true });

/*
 * Four folders and two files.
 *
 * Enough that the list has sections and the dropdown has something to filter
 * by, and few enough that the numbers in `gate-views.sh` can be exact. The
 * paths are obviously invented: a fixture that reads like somebody's real
 * machine invites the next person to wonder whose.
 */
const entries = [
  { folderUri: "file:///c%3A/fixture/alpha" },
  { folderUri: "file:///c%3A/fixture/beta" },
  { folderUri: "file:///c%3A/fixture/gamma" },
  { folderUri: "file:///c%3A/fixture/delta" },
  { fileUri: "file:///c%3A/fixture/notes.md" },
  { fileUri: "file:///c%3A/fixture/main.rs" },
];

const db = new DatabaseSync(file);
db.exec("CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value BLOB)");

// `recently.opened` rather than the older `history.recentlyOpenedPathsList`,
// because the extension's own query prefers it and takes exactly one row.
db.prepare("INSERT INTO ItemTable (key, value) VALUES (?, ?)").run(
  "recently.opened",
  JSON.stringify({ entries }),
);
db.close();

console.log(`seeded ${entries.length} recent entries at ${file}`);
