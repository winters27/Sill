/**
 * Drives the built host over stdio exactly as the Rust side will, and checks
 * the whole round trip: load, ready handshake, op stream out, handler
 * activation in, and the extension's own API call coming back.
 *
 * Run: node test/integration.mjs
 */
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");

let failures = 0;
function assert(cond, msg) {
  if (cond) {
    console.log(`ok   ${msg}`);
  } else {
    console.error(`FAIL ${msg}`);
    failures++;
  }
}

const child = spawn(process.execPath, [resolve(root, "dist/host.js")], {
  stdio: ["pipe", "pipe", "pipe"],
});

child.stderr.on("data", (d) => process.stderr.write(`[host stderr] ${d}`));

// ---- framing ----
function send(msg) {
  const body = Buffer.from(JSON.stringify(msg), "utf8");
  const frame = Buffer.allocUnsafe(body.length + 4);
  frame.writeUInt32BE(body.length, 0);
  body.copy(frame, 4);
  child.stdin.write(frame);
}

const inbox = [];
const waiters = [];
let buf = Buffer.alloc(0);

child.stdout.on("data", (chunk) => {
  buf = Buffer.concat([buf, chunk]);
  while (buf.length >= 4) {
    const len = buf.readUInt32BE(0);
    if (buf.length - 4 < len) break;
    const msg = JSON.parse(buf.subarray(4, 4 + len).toString("utf8"));
    buf = buf.subarray(4 + len);
    inbox.push(msg);
    for (const w of waiters.splice(0)) w();
  }
});

function waitFor(predicate, label, timeoutMs = 10000) {
  return new Promise((resolvePromise, reject) => {
    const deadline = setTimeout(() => reject(new Error(`timed out waiting for ${label}`)), timeoutMs);
    const check = () => {
      const found = inbox.find(predicate);
      if (found) {
        clearTimeout(deadline);
        resolvePromise(found);
      } else {
        waiters.push(check);
      }
    };
    check();
  });
}

let nextId = 1;
function request(method, params) {
  const id = nextId++;
  send({ jsonrpc: "2.0", id, method, params });
  return waitFor((m) => m.id === id && m.method === undefined, method);
}

/** Unwraps the extension's nested JSON-RPC out of Manager/extensionMessage. */
function extensionCalls() {
  return inbox
    .filter((m) => m.method === "Manager/extensionMessage")
    .map((m) => JSON.parse(m.params.payload));
}

try {
  // ---- load ----
  const loadRes = await request("Manager/load", {
    opts: {
      mode: "View",
      env: "Development",
      entrypoint: resolve(root, "test/fixture/list-command.js"),
      extension_name: "fixture",
      command_name: "list",
      is_raycast: true,
      preferences: {},
      arguments: {},
      launch_type: "User",
    },
  });

  const sessionId = loadRes.result?.session_id;
  assert(typeof sessionId === "string" && sessionId.length > 0, "load returned a session id");

  // ---- ready handshake releases buffered messages ----
  await request("Manager/ready", { session_id: sessionId });

  const renderMsg = await waitFor(
    (m) =>
      m.method === "Manager/extensionMessage" &&
      JSON.parse(m.params.payload).method === "UI/render",
    "UI/render",
  );
  assert(renderMsg.params.session_id === sessionId, "render arrived on the right session");

  const ops = JSON.parse(renderMsg.params.payload).params.ops;
  assert(Array.isArray(ops) && ops.length > 0, `render carried ops (${ops?.length})`);

  const creates = ops.filter((o) => o.op === "create");
  const tags = creates.map((o) => o.$t);
  assert(tags.includes("List"), `created a List (tags: ${tags.join(", ")})`);
  assert(tags.filter((t) => t === "List.Item").length === 2, "created both List.Items");
  assert(tags.includes("Action"), "created the Action inside the ActionPanel");

  const apple = creates.find((o) => o.props?.title === "Apple");
  assert(apple !== undefined, "the Apple item carried its title prop");

  const action = creates.find((o) => o.$t === "Action");
  const handlerId = action?.props?.onAction?.$handler;
  assert(typeof handlerId === "string", `the action's onAction became a handler id (${handlerId})`);

  // ---- activate the handler, which should make the extension call back ----
  // Sent as a request, not a notification. messageExtension is `fn ... => bool`
  // in the protocol, so a notification would be routed to event listeners and
  // silently dropped.
  await request("Manager/messageExtension", {
    session_id: sessionId,
    payload: JSON.stringify({
      jsonrpc: "2.0",
      id: 9001,
      method: "EventCore/handlerActivated",
      params: { id: handlerId, args: [] },
    }),
  });

  await waitFor(
    (m) =>
      m.method === "Manager/extensionMessage" &&
      JSON.parse(m.params.payload).method === "UI/showToast",
    "UI/showToast",
  );

  const toast = extensionCalls().find((c) => c.method === "UI/showToast");
  assert(toast.params.title === "Picked Apple", `toast carried the title (${toast.params.title})`);
  assert(toast.params.style === "success", `toast carried the style (${toast.params.style})`);

  // ---- unload ----
  const unloadRes = await request("Manager/unload", { session_id: sessionId });
  assert(unloadRes.result === true, "unload succeeded");
} catch (err) {
  console.error(`FAIL ${err.message}`);
  failures++;
} finally {
  child.kill();
}

console.log(failures === 0 ? "\nall integration checks passed" : `\n${failures} failure(s)`);
process.exit(failures === 0 ? 0 : 1);
