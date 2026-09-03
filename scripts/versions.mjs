/**
 * Every place Sill's version number is written down, in one list.
 *
 * There is one source, `package.json`, and `src-tauri/tauri.conf.json` reads
 * it rather than repeating it: Tauri takes a path to a `package.json` where a
 * semver string would go, so the installer's version and the window's title
 * bar come from the same line the frontend does.
 *
 * The rest cannot point at anything. Cargo will not read a version out of
 * another file, and neither will npm, so `src-tauri/Cargo.toml` and
 * `host/package.json` carry copies, and three lock files carry copies of
 * those. They are kept honest by a check rather than by memory, because the
 * failure is silent and asymmetric: `CARGO_PKG_VERSION` is in the log header,
 * the MCP handshake and the store's User-Agent, while Settings reads
 * `package_info().version`, which comes from the Tauri config. Two of those
 * disagreeing means a bug report naming a version that was never built.
 *
 * `read` reports what each file says. `write` sets them all. `verify:source`
 * uses the first and `npm run version:set` uses the second, so the list of
 * places cannot drift from the tool that maintains it.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");

/** The one file anybody edits by hand, and what the others are checked against. */
export const SOURCE = "package.json";

/** What `tauri.conf.json` must hold instead of a number, relative to `src-tauri`. */
export const TAURI_POINTER = "../package.json";

const path = (file) => join(root, file);

/**
 * The `[[package]]` block for this crate in a lock file.
 *
 * By name, because `Cargo.lock` holds six hundred packages and four of them
 * are also at 0.1.0. Anchored on the block rather than on the line, since a
 * bare `version = "0.1.0"` matches whichever dependency happens to come first.
 *
 * `\r?\n` and not `\n`. Only `*.sh` is pinned to LF in `.gitattributes`, so
 * every other file arrives with whatever the checkout does: CRLF on a Windows
 * clone, LF on the Linux one somebody will eventually run this from. A `\n`
 * here matched nothing on the machine it was written on.
 */
const CARGO_LOCK_ENTRY = /(\[\[package\]\]\r?\nname = "sill"\r?\nversion = ")([^"]+)(")/;

/*
 * A copy that has to agree, in the order somebody would edit them.
 *
 * `read` returns the version each one currently holds, or `null` when the
 * file no longer has the shape the pattern expects, which is itself a
 * failure worth reporting rather than a silent pass.
 */
const COPIES = [
  {
    file: "src-tauri/Cargo.toml",
    what: "the crate version, which is `CARGO_PKG_VERSION` in the log and the User-Agent",
    read: (text) => text.match(/^version = "([^"]+)"$/m)?.[1] ?? null,
    write: (text, version) => text.replace(/^version = "[^"]+"$/m, `version = "${version}"`),
  },
  {
    file: "src-tauri/Cargo.lock",
    what: "the lock file's copy of the crate version",
    read: (text) => text.match(CARGO_LOCK_ENTRY)?.[2] ?? null,
    write: (text, version) => text.replace(CARGO_LOCK_ENTRY, `$1${version}$3`),
  },
  {
    file: "host/package.json",
    what: "the extension host, which ships inside the same installer",
    read: (text) => JSON.parse(text).version ?? null,
    write: (text, version) => setJsonVersion(text, version, 1),
  },
  {
    file: "package-lock.json",
    what: "npm's copy, which `npm ci` rewrites the tree from",
    read: (text) => JSON.parse(text).version ?? null,
    write: (text, version) => setJsonVersion(text, version, 2),
  },
  {
    file: "host/package-lock.json",
    what: "npm's copy for the host",
    read: (text) => JSON.parse(text).version ?? null,
    write: (text, version) => setJsonVersion(text, version, 2),
  },
];

/**
 * A version written into JSON without reformatting the file around it.
 *
 * `JSON.parse` then `stringify` would reindent a 2,000 line lock file and
 * bury one changed character in a diff nobody can read, so the string is
 * edited in place, and `howMany` says how many `"version":` lines to set,
 * counted from the top. The root object's is the first in the file and
 * `packages[""].version` is the second: a lock file has both, and a package
 * file has only the first.
 */
function setJsonVersion(text, version, howMany) {
  let out = text;
  let from = 0;

  for (let n = 0; n < howMany; n += 1) {
    const at = out.indexOf('"version":', from);
    if (at === -1) break;

    const end = out.indexOf("\n", at);
    const line = out.slice(at, end);
    out = out.slice(0, at) + line.replace(/"version":\s*"[^"]*"/, `"version": "${version}"`) + out.slice(end);
    from = at + 1;
  }

  return out;
}

/** The version `package.json` declares, which is the answer everything else owes. */
export function source() {
  return JSON.parse(readFileSync(path(SOURCE), "utf8")).version;
}

/** What `tauri.conf.json` has where its version goes. */
export function tauriVersion() {
  return JSON.parse(readFileSync(path("src-tauri/tauri.conf.json"), "utf8")).version;
}

/** Each copy and what it currently says. */
export function read() {
  return COPIES.map((copy) => ({
    file: copy.file,
    what: copy.what,
    version: copy.read(readFileSync(path(copy.file), "utf8")),
  }));
}

/** Sets every copy to `version`, and reports which files changed. */
export function write(version) {
  const touched = [];

  for (const copy of COPIES) {
    const before = readFileSync(path(copy.file), "utf8");
    const after = copy.write(before, version);
    if (after === before) continue;

    writeFileSync(path(copy.file), after);
    touched.push(copy.file);
  }

  return touched;
}
