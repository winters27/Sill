/**
 * Closes the loop between the host and the UI without needing a window.
 *
 * Runs the real extension host, captures the op stream it emits, applies it
 * with the UI's own ViewTree, and asserts the shape the Svelte components
 * depend on: the flattened item order, and the handler id behind an item's
 * primary action.
 *
 * A GUI test can only tell you it looked right. This tells you the tree the
 * UI is drawing from is the tree the extension described.
 *
 * Run: node scripts/verify-tree.mjs
 */
import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const root = resolve(import.meta.dirname, "..");
const hostJs = join(root, "host", "dist", "host.js");
const fixture = join(root, "host", "test", "fixture", "list-command.js");

let failures = 0;
const assert = (cond, msg) => {
  console.log(`${cond ? "ok  " : "FAIL"} ${msg}`);
  if (!cond) failures++;
};

// ---- bundle the UI's tree module so this script can use the real thing ----
const out = join(mkdtempSync(join(tmpdir(), "sill-tree-")), "tree.mjs");
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
  p.on("exit", (code) => (code === 0 ? done() : fail(new Error(`esbuild exited ${code}`))));
});

const { ViewTree, isHandlerRef } = await import(pathToFileURL(out).href);

// ---- drive the host to get a real op stream ----
const child = spawn(process.execPath, [hostJs], { stdio: ["pipe", "pipe", "pipe"] });
child.stderr.on("data", (d) => process.stderr.write(`[host] ${d}`));

const send = (m) => {
  const b = Buffer.from(JSON.stringify(m), "utf8");
  const f = Buffer.allocUnsafe(b.length + 4);
  f.writeUInt32BE(b.length, 0);
  b.copy(f, 4);
  child.stdin.write(f);
};

const inbox = [];
let buf = Buffer.alloc(0);
child.stdout.on("data", (chunk) => {
  buf = Buffer.concat([buf, chunk]);
  while (buf.length >= 4) {
    const len = buf.readUInt32BE(0);
    if (buf.length - 4 < len) break;
    inbox.push(JSON.parse(buf.subarray(4, 4 + len).toString("utf8")));
    buf = buf.subarray(4 + len);
  }
});

const settle = (ms) => new Promise((r) => setTimeout(r, ms));

send({
  jsonrpc: "2.0",
  id: 1,
  method: "Manager/load",
  params: {
    opts: {
      mode: "View",
      env: "Development",
      entrypoint: fixture,
      extension_name: "fixture",
      extension_id: "fixture",
      command_name: "list",
      is_raycast: true,
      preferences: {},
      arguments: {},
      launch_type: "User",
    },
  },
});

await settle(400);
const sessionId = inbox.find((m) => m.id === 1)?.result?.session_id;
send({ jsonrpc: "2.0", id: 2, method: "Manager/ready", params: { session_id: sessionId } });
await settle(1200);

const ops = inbox
  .filter((m) => m.method === "Manager/extensionMessage")
  .map((m) => JSON.parse(m.params.payload))
  .filter((c) => c.method === "UI/render")
  .flatMap((c) => c.params.ops);

child.kill();

assert(ops.length > 0, `captured a real op stream (${ops.length} ops)`);

// ---- apply it exactly as the UI does ----
const tree = new ViewTree();
tree.apply(ops);

const top = tree.top();
assert(top !== undefined, "the tree has a root view");
assert(top?.tag === "List", `the root view is a List (got ${top?.tag})`);

assert(
  top?.props.searchBarPlaceholder === "Search fruit",
  "the List kept its searchBarPlaceholder prop",
);

// Flatten items the way ListView does.
const items = [];
for (const child of tree.elementChildren(top)) {
  if (child.tag === "List.Section") {
    items.push(...tree.elementChildren(child).filter((c) => c.tag === "List.Item"));
  } else if (child.tag === "List.Item") {
    items.push(child);
  }
}

assert(items.length === 2, `both items are selectable (${items.length})`);
assert(items[0]?.props.title === "Apple", `first item is Apple (${items[0]?.props.title})`);
assert(items[1]?.props.title === "Banana", `second item is Banana (${items[1]?.props.title})`);

// The element-prop-to-slot round trip is the piece most likely to break.
const panel = tree.slot(items[0], "actions");
assert(panel !== undefined, "the actions element prop came back through its $slot");
assert(panel?.tag === "ActionPanel", `the slot held an ActionPanel (got ${panel?.tag})`);

const action = panel ? tree.elementChildren(panel)[0] : undefined;
assert(action?.tag === "Action", `the panel held an Action (got ${action?.tag})`);
assert(action?.props.title === "Pick it", "the action kept its title");
assert(
  isHandlerRef(action?.props.onAction),
  `onAction survived as a handler reference (${JSON.stringify(action?.props.onAction)})`,
);

// The second item has no actions, which must read as absent, not as a crash.
assert(tree.slot(items[1], "actions") === undefined, "an item without actions has no slot");

console.log(failures === 0 ? "\ntree verification passed" : `\n${failures} failure(s)`);
process.exit(failures === 0 ? 0 : 1);
