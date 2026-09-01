/**
 * An extension that only spins is stopped, and one that behaves is not.
 *
 * Its own host process, because it runs with a budget measured in seconds
 * rather than the real half minute, and those constants are read once at
 * startup. A test that waited for the real budget is a test nobody runs.
 *
 * Run: node test/runaway.mjs
 */
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");

let failures = 0;
function assert(cond, msg) {
  console.log(`${cond ? "ok  " : "FAIL"} ${msg}`);
  if (!cond) failures++;
}

const child = spawn(process.execPath, [resolve(root, "dist/host.js")], {
  stdio: ["pipe", "pipe", "pipe"],
  env: { ...process.env, SILL_RUNAWAY_MS: "1500", SILL_RUNAWAY_CHECK_MS: "250" },
});

child.stderr.on("data", (d) => process.stderr.write(`[host stderr] ${d}`));

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
    inbox.push(JSON.parse(buf.subarray(4, 4 + len).toString("utf8")));
    buf = buf.subarray(4 + len);
    for (const w of waiters.splice(0)) w();
  }
});

function waitFor(predicate, label, timeoutMs = 15000) {
  return new Promise((ok, no) => {
    const deadline = setTimeout(() => no(new Error(`timed out waiting for ${label}`)), timeoutMs);
    const check = () => {
      const found = inbox.find(predicate);
      if (found) {
        clearTimeout(deadline);
        ok(found);
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

const load = (file, name) =>
  request("Manager/load", {
    opts: {
      mode: "View",
      env: "Development",
      entrypoint: resolve(root, `test/fixture/${file}`),
      extension_name: name,
      command_name: "run",
      is_raycast: true,
      preferences: {},
      arguments: {},
      launch_type: "User",
      capabilities: [],
    },
  });

try {
  // ---- the one that spins ----
  const spinning = await load("spins-forever.js", "spinner");
  const spinningSession = spinning.result?.session_id;

  const crash = await waitFor(
    (m) => m.method === "Manager/extensionCrash" && m.params.session_id === spinningSession,
    "an extension that only spins was stopped",
  );

  assert(
    /without pausing/.test(crash.params.reason),
    `the reason says what it did (${crash.params.reason})`,
  );
  assert(
    /looping rather than working/.test(crash.params.reason),
    "and says what that means",
  );

  // ---- the one that behaves ----
  //
  // The half that stops this being a watchdog that kills everything. Loaded
  // after the budget has already fired once, and left alone for several times
  // the budget.
  const calm = await load("list-command.js", "calm");
  const calmSession = calm.result?.session_id;

  send({
    jsonrpc: "2.0",
    id: 9000,
    method: "Manager/ready",
    params: { session_id: calmSession },
  });

  await waitFor(
    (m) =>
      m.method === "Manager/extensionMessage" &&
      m.params.session_id === calmSession &&
      JSON.parse(m.params.payload).method === "UI/render",
    "an ordinary extension drew",
  );

  await new Promise((r) => setTimeout(r, 2500));

  assert(
    !inbox.some(
      (m) => m.method === "Manager/extensionCrash" && m.params.session_id === calmSession,
    ),
    "and was still running well past the budget",
  );
} catch (err) {
  console.error(`FAIL ${err.message}`);
  failures++;
} finally {
  child.kill();
}

console.log(failures === 0 ? "\nrunaway checks passed" : `\n${failures} failure(s)`);
process.exit(failures === 0 ? 0 : 1);
