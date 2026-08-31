/**
 * Runs a built extension against the real host and reports what it did.
 *
 * Stands in for the Rust side so an extension can be exercised without a
 * window: it serves the API layer, prints every call the extension makes, and
 * applies the op stream with the UI's own ViewTree so the rendered structure
 * can be inspected as text.
 *
 * Any call the extension makes that is not implemented shows up in the log as
 * an explicit gap, which is the whole point: this is the tool for finding out
 * what a real extension actually needs.
 *
 * Usage: node scripts/run-extension.mjs <entrypoint.js> [extensionName] [--seed key=json]
 */
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const args = process.argv.slice(2);
const entrypoint = resolve(args[0] ?? "");
const extensionName = args[1] && !args[1].startsWith("--") ? args[1] : "ext";

/*
 * Mode matters: a no-view command's default export is an async function, not
 * a component. Loading one as a view makes React call it as a component over
 * and over, which shows up as the same side effect firing a hundred times.
 */
const modeArg = args.includes("--no-view") ? "NoView" : "View";

/** Pre-populated LocalStorage, so a history view has history to show. */
const seeds = new Map();
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--seed" && args[i + 1]) {
    const eq = args[i + 1].indexOf("=");
    seeds.set(args[i + 1].slice(0, eq), JSON.parse(args[i + 1].slice(eq + 1)));
  }
}

const root = resolve(import.meta.dirname, "..");
const hostJs = join(root, "host", "dist", "host.js");

/*
 * Scratch directories, and getting rid of them.
 *
 * Two bundles are written to temp on every run, once per extension, and the
 * view gate runs this over every extension there is. Nothing removed them, so
 * they accumulated: seven hundred and seventy-four of them on the machine this
 * was found on, going back to the first day the gate existed.
 *
 * Registered rather than removed at the end, because the interesting exits are
 * the ones that do not reach the end: a bundle that fails to build, a host
 * that dies, somebody pressing Ctrl+C while watching the log go past.
 */
const scratch = [];

function scratchDir(prefix) {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  scratch.push(dir);
  return dir;
}

let tidied = false;

function tidy() {
  if (tidied) return;
  tidied = true;

  for (const dir of scratch) {
    // `force` because a directory that is already gone is not a problem worth
    // failing an otherwise green gate over.
    try {
      rmSync(dir, { recursive: true, force: true });
    } catch {
      // Windows will refuse while the file is still mapped by the import
      // above. One left behind is what this used to do every time.
    }
  }
}

process.on("exit", tidy);
// `exit` does not fire for these, and these are how a gate run usually ends
// when something is wrong.
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(signal, () => {
    tidy();
    process.exit(130);
  });
}
process.on("uncaughtException", (err) => {
  tidy();
  console.error(err);
  process.exit(1);
});

// ---- the UI's real tree module ----
const out = join(scratchDir("sill-run-"), "tree.mjs");
await new Promise((done, fail) => {
  const p = spawn(
    process.execPath,
    [
      join(root, "host", "node_modules", "esbuild", "bin", "esbuild"),
      join(root, "src", "lib", "exthost", "tree.ts"),
      "--bundle",
      "--format=esm",
      `--outfile=${out}`,
      "--log-level=error",
    ],
    { stdio: "inherit" },
  );
  p.on("exit", (c) => (c === 0 ? done() : fail(new Error(`esbuild exited ${c}`))));
});
const { ViewTree } = await import(pathToFileURL(out).href);

// The action reader, bundled the same way so the real implementation is used.
const actionsOut = join(scratchDir("sill-actions-"), "actions.mjs");
await new Promise((done, fail) => {
  const p = spawn(
    process.execPath,
    [
      join(root, "host", "node_modules", "esbuild", "bin", "esbuild"),
      join(root, "src", "lib", "exthost", "actions.ts"),
      "--bundle",
      "--format=esm",
      `--outfile=${actionsOut}`,
      "--log-level=error",
    ],
    { stdio: "inherit" },
  );
  p.on("exit", (c) => (c === 0 ? done() : fail(new Error(`esbuild exited ${c}`))));
});
const { collectActions, shortcutKeys, isRunnable } = await import(pathToFileURL(actionsOut).href);

// ---- host process ----
const child = spawn(process.execPath, [hostJs], { stdio: ["pipe", "pipe", "pipe"] });
child.stderr.on("data", (d) => process.stderr.write(`[host] ${d}`));

const send = (m) => {
  const b = Buffer.from(JSON.stringify(m), "utf8");
  const f = Buffer.allocUnsafe(b.length + 4);
  f.writeUInt32BE(b.length, 0);
  b.copy(f, 4);
  child.stdin.write(f);
};

const storage = new Map(seeds);
const calls = [];
const gaps = new Set();
const tree = new ViewTree();
let session;
let buf = Buffer.alloc(0);

/**
 * Mimics Rust's ApiLayer closely enough to exercise an extension.
 *
 * "Closely enough" is a hazard as well as a convenience, and it bit once: this
 * table answered Clipboard/copy, Clipboard/paste, Clipboard/readContent and
 * UI/confirmAlert for months while `ApiLayer::dispatch` had no arm for any of
 * them, so the gate ran green against a program that did not exist and every
 * extension calling `Clipboard.copy` failed for real users.
 *
 * The guard against that living here is not this file, which cannot see Rust.
 * It is `every_method_the_host_calls_is_answered_or_declared_missing` in
 * `src-tauri/tests/exthost.rs`, which reads the host's own source and dispatches
 * every method it finds. **Adding a case here is not evidence of anything.**
 */
function serveApi(method, params) {
  switch (method) {
    case "UI/render":
      tree.apply(params.ops ?? []);
      return null;
    case "Storage/get":
      return storage.has(params.key) ? storage.get(params.key) : null;
    case "Storage/set":
      storage.set(params.key, params.value);
      return null;
    case "Storage/remove":
      storage.delete(params.key);
      return null;
    case "Storage/list":
      return Object.fromEntries(storage);
    case "UI/showToast":
    case "UI/updateToast":
    case "UI/hideToast":
    case "UI/showHud":
    case "UI/setSearchText":
    case "UI/popToRoot":
    case "UI/closeMainWindow":
    case "Clipboard/copy":
    case "Clipboard/paste":
    case "Clipboard/clear":
    case "Application/open":
      return null;
    case "Clipboard/readContent":
      return { text: "", html: null, file: null };
    case "Application/list":
      return [];
    case "UI/confirmAlert":
      return true;
    default:
      gaps.add(method);
      throw new Error(`not implemented: ${method}`);
  }
}

child.stdout.on("data", (chunk) => {
  buf = Buffer.concat([buf, chunk]);
  while (buf.length >= 4) {
    const len = buf.readUInt32BE(0);
    if (buf.length - 4 < len) break;
    const msg = JSON.parse(buf.subarray(4, 4 + len).toString("utf8"));
    buf = buf.subarray(4 + len);

    if (msg.id === 1) session = msg.result?.session_id;

    if (msg.method === "Manager/extensionCrash") {
      console.error(`\nCRASH: ${msg.params.reason}\n`);
      continue;
    }

    if (msg.method !== "Manager/extensionMessage") continue;

    const call = JSON.parse(msg.params.payload);
    if (!call.method) continue;

    calls.push(call.method);

    let result = null;
    let error = null;
    try {
      result = serveApi(call.method, call.params ?? {});
    } catch (err) {
      error = { code: -32601, message: err.message };
    }

    if (call.id !== undefined) {
      send({
        jsonrpc: "2.0",
        method: "Manager/messageExtension",
        id: 500 + calls.length,
        params: {
          session_id: msg.params.session_id,
          payload: JSON.stringify(
            error
              ? { jsonrpc: "2.0", id: call.id, error }
              : { jsonrpc: "2.0", id: call.id, result },
          ),
        },
      });
    }
  }
});

/**
 * What Rust would send as `preferences` for this entrypoint.
 *
 * Read from the built index rather than hardcoded, because a hardcoded `{}`
 * is what shipped: `getPreferenceValues()` answered with nothing for months
 * while every manifest in the sample set declared defaults, and this harness
 * agreed with the bug instead of catching it.
 */
function manifestPreferences() {
  const indexPath = join(root, "extensions", "build", "index.json");
  if (!existsSync(indexPath)) return {};

  try {
    const index = JSON.parse(readFileSync(indexPath, "utf8"));
    const normalise = (p) => resolve(p).split("\\").join("/").toLowerCase();
    const mine = normalise(entrypoint);
    const record = index.find((entry) => normalise(entry.entrypoint) === mine);
    return record?.preferences ?? {};
  } catch {
    // A fixture built outside the index is legitimate; it simply has none.
    return {};
  }
}

const settle = (ms) => new Promise((r) => setTimeout(r, ms));

send({
  jsonrpc: "2.0",
  id: 1,
  method: "Manager/load",
  params: {
    opts: {
      mode: modeArg,
      env: "Development",
      entrypoint,
      extension_name: extensionName,
      extension_id: extensionName,
      command_name: "cmd",
      is_raycast: true,
      preferences: manifestPreferences(),
      arguments: {},
      launch_type: "User",
    },
  },
});

await settle(500);
send({ jsonrpc: "2.0", id: 2, method: "Manager/ready", params: { session_id: session } });
await settle(2500);
child.kill();

// ---- report ----
const counts = calls.reduce((acc, m) => acc.set(m, (acc.get(m) ?? 0) + 1), new Map());
console.log("API calls made by the extension:");
for (const [method, n] of [...counts].sort()) console.log(`  ${method} x${n}`);

console.log("\nRendered tree:");
const render = (node, depth = 0) => {
  const pad = "  ".repeat(depth + 1);
  if (node.kind === "text") {
    if (node.text.trim()) console.log(`${pad}"${node.text}"`);
    return;
  }
  const title = node.props.title ?? node.props.name ?? "";
  const label = title ? ` ${JSON.stringify(String(title))}` : "";
  console.log(`${pad}<${node.tag}${label}>`);
  for (const child of tree.children(node)) render(child, depth + 1);
};

const top = tree.top();
if (top) render(top);
else console.log("  (nothing rendered)");

// The action set the panel would show for the first selectable item.
let firstActions = [];
if (top) {
  const itemTag = top.tag === "Grid" ? "Grid.Item" : "List.Item";
  const sectionTag = top.tag === "Grid" ? "Grid.Section" : "List.Section";
  const flat = [];
  for (const child of tree.elementChildren(top)) {
    if (child.tag === sectionTag) {
      flat.push(...tree.elementChildren(child).filter((c) => c.tag === itemTag));
    } else if (child.tag === itemTag) {
      flat.push(child);
    }
  }
  const subject = flat[0] ?? top;
  firstActions = collectActions(tree, subject);

  if (firstActions.length) {
    console.log("");
    console.log("Actions on the first item:");
    for (const a of firstActions) {
      const keys = a.shortcut ? `  [${shortcutKeys(a.shortcut).join(" ")}]` : "";
      const section = a.section ? `  (${a.section})` : "";
      const inert = a.handler ? "" : isRunnable(a) ? "  <built-in>" : "  <no handler>";
      console.log(`  ${a.title}${section}${keys}${inert}`);
    }
  }
}

if (gaps.size) {
  console.log("\nUnimplemented API surface this extension needs:");
  for (const g of [...gaps].sort()) console.log(`  ${g}`);
}

// Optional assertions, so this doubles as a regression gate rather than only
// a reporting tool.
const flag = (name) => {
  const i = args.indexOf(name);
  return i === -1 ? undefined : args[i + 1];
};

const expectRoot = flag("--expect-root");
const expectItems = flag("--expect-items");
const expectActions = flag("--expect-actions");
let failed = false;

const check = (cond, msg) => {
  console.log(`${cond ? "ok  " : "FAIL"} ${msg}`);
  if (!cond) failed = true;
};

if (expectRoot !== undefined) {
  check(top?.tag === expectRoot, `root view is <${expectRoot}>, got <${top?.tag}>`);
}

if (expectItems !== undefined) {
  const itemTag = `${top?.tag}.Item`;
  const count = top
    ? tree.elementChildren(top).filter((c) => c.tag === itemTag).length
    : 0;
  check(count === Number(expectItems), `rendered ${expectItems} ${itemTag}, got ${count}`);
}

if (expectActions !== undefined) {
  check(
    firstActions.length === Number(expectActions),
    `first item has ${expectActions} actions, got ${firstActions.length}`,
  );
  // A built-in legitimately has no handler: Raycast performs it, and so does
  // Sill. Requiring a callback on every action would fail on the single most
  // common action in the ecosystem, Action.CopyToClipboard.
  check(
    firstActions.every((a) => isRunnable(a)),
    "every action is runnable, by callback or as a built-in",
  );
}

if (expectRoot !== undefined || expectItems !== undefined || expectActions !== undefined) {
  check(gaps.size === 0, `no unimplemented API was needed, ${gaps.size} gap(s)`);
}

console.log(top ? "\nextension rendered" : "\nextension produced no view");
process.exit(top && !failed ? 0 : 1);
