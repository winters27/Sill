/**
 * What an extension costs, asked of a real host over the real protocol.
 *
 * Three claims, and they are the three the Extensions panel rests on.
 *
 *   A command that is running reports its own memory and its share of a core.
 *   A command that will not answer is reported as not answering, not as zero.
 *   A command that keeps allocating is stopped, and what is said about it is
 *   a sentence rather than a stack trace.
 *
 * Its own host process, with a heap cap of a few megabytes rather than the
 * real 512, because proving the third claim at the real cap means allocating
 * half a gigabyte and waiting. Those constants are read once at startup, which
 * is why this cannot share the integration host.
 *
 * Run: node test/resources.mjs
 */
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");

/** Small enough that the leaking fixture reaches it in about a second. */
const CAP_MB = 48;

let failures = 0;
function assert(cond, msg) {
  console.log(`${cond ? "ok  " : "FAIL"} ${msg}`);
  if (!cond) failures++;
}

const child = spawn(process.execPath, [resolve(root, "dist/host.js")], {
  stdio: ["pipe", "pipe", "pipe"],
  env: { ...process.env, SILL_WORKER_HEAP_MB: String(CAP_MB) },
});

let saidOnStderr = "";
child.stderr.on("data", (d) => {
  saidOnStderr += d;
  process.stderr.write(`[host stderr] ${d}`);
});

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

function waitFor(predicate, label, timeoutMs = 20000) {
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
function request(method, params = {}) {
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

const readingFor = async (session) => {
  const answer = await request("Manager/diagnostics");
  return (answer.result?.workers ?? []).find((one) => one.session_id === session);
};

try {
  // ---- an ordinary command reports what it holds ----
  const calm = await load("list-command.js", "calm");
  const calmSession = calm.result?.session_id;
  send({ jsonrpc: "2.0", id: 9000, method: "Manager/ready", params: { session_id: calmSession } });

  await waitFor(
    (m) =>
      m.method === "Manager/extensionMessage" &&
      m.params.session_id === calmSession &&
      JSON.parse(m.params.payload).method === "UI/render",
    "an ordinary extension drew",
  );

  const calmReading = await readingFor(calmSession);

  assert(calmReading !== undefined, "a loaded command has a reading");
  assert(calmReading?.answering === true, "and it answered");
  assert(
    calmReading?.heap_bytes > 1024 * 1024,
    `and said what it is holding (${Math.round((calmReading?.heap_bytes ?? 0) / 1024 / 1024)} MB)`,
  );
  assert(
    calmReading?.heap_limit_bytes === CAP_MB * 1024 * 1024,
    "the limit reported is the one the host set, not the one V8 reports back",
  );
  assert(
    typeof calmReading?.core_percent === "number" && calmReading.core_percent >= 0,
    `and its share of a core (${calmReading?.core_percent}%)`,
  );

  // ---- a command that will not answer is still described ----
  //
  // The one the panel exists for. An extension in a loop holds its own event
  // loop, so it cannot answer how much memory it is using; the share of a core
  // is measured from this thread and arrives regardless. Reporting nothing
  // here, or waiting for it, would fail on exactly the extension somebody
  // opened the panel to find.
  const stuck = await load("spins-forever.js", "spinner");
  const stuckSession = stuck.result?.session_id;
  send({ jsonrpc: "2.0", id: 9001, method: "Manager/ready", params: { session_id: stuckSession } });

  // Long enough for the loop to be measurably the whole of its event loop.
  await new Promise((r) => setTimeout(r, 1000));

  const stuckReading = await readingFor(stuckSession);

  assert(stuckReading !== undefined, "a command stuck in a loop still has a reading");
  assert(stuckReading?.answering === false, "reported as not answering");
  assert(stuckReading?.heap_bytes === null, "with no memory figure invented for it");
  assert(
    stuckReading?.core_percent > 50,
    `and its share of a core says why (${stuckReading?.core_percent}%)`,
  );

  await request("Manager/unload", { session_id: stuckSession });

  // ---- one that has gone is not reported at all ----
  assert(
    (await readingFor(stuckSession)) === undefined,
    "an unloaded command leaves no reading behind",
  );

  // ---- one that keeps allocating is stopped, in words ----
  const hungry = await load("eats-memory.js", "hungry");
  const hungrySession = hungry.result?.session_id;
  send({
    jsonrpc: "2.0",
    id: 9002,
    method: "Manager/ready",
    params: { session_id: hungrySession },
  });

  const crash = await waitFor(
    (m) => m.method === "Manager/extensionCrash" && m.params.session_id === hungrySession,
    "an extension that keeps allocating was stopped",
  );

  const reason = crash.params.reason ?? "";

  assert(reason.includes(`${CAP_MB} MB`), `the reason says the limit (${reason})`);
  assert(/memory/.test(reason), "and says what ran out");
  assert(
    !/ERR_WORKER_OUT_OF_MEMORY|at Worker|node:internal/.test(reason),
    "and is a sentence rather than a stack trace",
  );
  assert(
    /leaked/.test(reason),
    "and says what that usually means, so somebody knows whether to keep it",
  );

  // A `no-view` command has no screen for the launcher to put that on, so it
  // is written where a person would look afterwards as well.
  assert(
    saidOnStderr.includes("hungry/run: stopped after using more than"),
    "and the log names which extension and which command it was",
  );

  // ---- and the one that behaved is still running ----
  assert(
    !inbox.some(
      (m) => m.method === "Manager/extensionCrash" && m.params.session_id === calmSession,
    ),
    "the extension that behaved was not touched by any of it",
  );
} catch (err) {
  console.error(`FAIL ${err.message}`);
  failures++;
} finally {
  child.kill();
}

console.log(failures === 0 ? "\nresource checks passed" : `\n${failures} failure(s)`);
process.exit(failures === 0 ? 0 : 1);
