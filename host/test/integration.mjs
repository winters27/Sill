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

/**
 * Everything the host has said, kept as well as echoed.
 *
 * The host's stderr is where an extension's own `console.log` comes out, so
 * this is the only place a test can see it. Rust reads the same stream and
 * writes it into Sill's log.
 */
let said = "";

child.stderr.on("data", (d) => {
  said += d;
  process.stderr.write(`[host stderr] ${d}`);
});

function waitForSaid(pattern, label, timeoutMs = 10000) {
  return new Promise((ok, no) => {
    const until = Date.now() + timeoutMs;
    const look = () => {
      if (pattern.test(said)) return ok(said);
      if (Date.now() > until) return no(new Error(`timed out waiting for ${label}`));
      setTimeout(look, 25);
    };
    look();
  });
}

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
  assert(unloadRes.result?.ok === true, "unload succeeded");
  // Asked on the way out, because this is the last moment it exists: the
  // worker is gone by the time this reply is written. It is also the only
  // reading that lets two extensions be compared after the fact, since a
  // launcher has one command loaded at a time.
  assert(
    unloadRes.result?.heap_bytes > 1024 * 1024,
    `and said what it was holding at the end (${Math.round((unloadRes.result?.heap_bytes ?? 0) / 1024 / 1024)} MB)`,
  );

  // ---- what the extension itself says ----
  //
  // Workers are spawned with `stdout` and `stderr` set, which hands the host
  // two streams rather than forwarding the output. Nothing read them, so an
  // extension's console output went into a buffer nobody would ever drain:
  // invisible to its author wherever they looked, and held in memory for as
  // long as the worker lived.
  const talker = await request("Manager/load", {
    opts: {
      mode: "View",
      env: "Development",
      entrypoint: resolve(root, "test/fixture/talks-to-the-console.js"),
      extension_name: "chatty",
      command_name: "fruit",
      is_raycast: true,
      preferences: {},
      arguments: {},
      launch_type: "User",
      capabilities: [],
    },
  });

  await waitForSaid(
    /chatty\/fruit: the fruit list is being drawn/,
    "an extension's console.log reached the host's stderr, named",
  );
  assert(
    /chatty\/fruit: the fruit list is being drawn/.test(said),
    "console.log arrives, under the extension and command that wrote it",
  );

  // Waited for separately: it comes up the other stream, and the two do not
  // arrive in the order they were written.
  await waitForSaid(
    /chatty\/fruit: and something about it went wrong/,
    "an extension's console.error",
  );
  assert(
    /chatty\/fruit: and something about it went wrong/.test(said),
    "and so does console.error, which is on the other stream",
  );

  // The long line, cut. Whole, it is four thousand characters of one
  // extension's output in a log somebody else has to read.
  await waitForSaid(/start-of-a-long-line/, "the long line");
  assert(
    /start-of-a-long-line/.test(said) && !said.includes("end-of-a-long-line"),
    "a line longer than the limit is carried cut rather than whole",
  );
  assert(/ \(cut\)/.test(said), "and says that it was cut");

  await request("Manager/unload", { session_id: talker.result?.session_id });

  // ---- the sandbox, both ways round ----
  //
  // Loading the same fixture twice, once with the permission and once
  // without, because only the pair proves anything. Refused-on-its-own could
  // be an extension that never worked, and allowed-on-its-own could be a gate
  // that never closes.
  const loadDisk = (capabilities) =>
    request("Manager/load", {
      opts: {
        mode: "View",
        env: "Development",
        entrypoint: resolve(root, "test/fixture/reads-disk.js"),
        extension_name: "disky",
        command_name: "read",
        is_raycast: true,
        preferences: {},
        arguments: {},
        launch_type: "User",
        capabilities,
      },
    });

  const refused = await loadDisk([]);
  const refusedSession = refused.result?.session_id;

  const crash = await waitFor(
    (m) => m.method === "Manager/extensionCrash" && m.params.session_id === refusedSession,
    "an extension with no permission crashed rather than reading the disk",
  );

  assert(
    /not allowed to read and change files/.test(crash.params.reason),
    `the crash names the permission (${crash.params.reason})`,
  );
  assert(
    /Settings/.test(crash.params.reason),
    "the crash says where to grant it",
  );

  const allowed = await loadDisk(["fileRead", "fileWrite"]);
  const allowedSession = allowed.result?.session_id;

  send({
    jsonrpc: "2.0",
    id: 9100,
    method: "Manager/ready",
    params: { session_id: allowedSession },
  });

  const drew = await waitFor(
    (m) =>
      m.method === "Manager/extensionMessage" &&
      m.params.session_id === allowedSession &&
      JSON.parse(m.params.payload).method === "UI/render",
    "the same extension drew once the permission was granted",
  );

  assert(
    JSON.stringify(JSON.parse(drew.params.payload).params.ops).includes("reached the disk"),
    "and it really did reach the disk",
  );

  // ---- the ways round the front door ----
  //
  // The gate used to sit on `Module.prototype.require`, and three supported
  // things reach a builtin without going near it. `process.getBuiltinModule`
  // is the one that mattered: documented, in Node since 22.3, and one line.
  // The gate now sits on `Module._load`, which all three end up in.
  const sneaky = await request("Manager/load", {
    opts: {
      mode: "View",
      env: "Development",
      entrypoint: resolve(root, "test/fixture/sneaks-to-disk.js"),
      extension_name: "sneaky",
      command_name: "read",
      is_raycast: true,
      preferences: {},
      arguments: {},
      launch_type: "User",
      capabilities: [],
    },
  });

  const sneakyCrash = await waitFor(
    (m) =>
      m.method === "Manager/extensionCrash" &&
      m.params.session_id === sneaky.result?.session_id,
    "an extension trying the back doors crashed rather than reading the disk",
  );

  assert(
    /every escape was refused/.test(sneakyCrash.params.reason),
    `no back door reached the disk (${sneakyCrash.params.reason})`,
  );

  for (const door of ["Module._load", "createRequire", "getBuiltinModule"]) {
    assert(
      new RegExp(`${door.replace(".", "\\.")}: sill:`).test(sneakyCrash.params.reason),
      `${door} was refused by the gate rather than failing for some other reason`,
    );
  }

  /*
   * And the same fixture with the permission, which is what makes the check
   * above worth anything.
   *
   * A refusal proves nothing unless the thing being refused would otherwise
   * work: a fixture whose escapes were simply broken would produce exactly the
   * same "every escape was refused" and pass while testing nothing.
   */
  const sneakyAllowed = await request("Manager/load", {
    opts: {
      mode: "View",
      env: "Development",
      entrypoint: resolve(root, "test/fixture/sneaks-to-disk.js"),
      extension_name: "sneaky-allowed",
      command_name: "read",
      is_raycast: true,
      preferences: {},
      arguments: {},
      launch_type: "User",
      capabilities: ["fileRead", "fileWrite"],
    },
  });

  const sneakyReached = await waitFor(
    (m) =>
      m.method === "Manager/extensionCrash" &&
      m.params.session_id === sneakyAllowed.result?.session_id,
    "the same fixture ran with the permission granted",
  );

  assert(
    /reached the disk through Module\._load/.test(sneakyReached.params.reason),
    `the escapes really do work when allowed, so the refusal above means ` +
      `something (${sneakyReached.params.reason})`,
  );

  /*
   * Everything below drives one fixture twice, refused and then allowed, so
   * this is written once. `where` is a path under `test/fixture`, `who` names
   * the session in the log, and `holds` is what Rust would have granted.
   */
  const loadFixture = async (where, who, holds) => {
    const loaded = await request("Manager/load", {
      opts: {
        mode: "View",
        env: "Development",
        entrypoint: resolve(root, "test/fixture", where),
        extension_name: who,
        command_name: "run",
        is_raycast: true,
        preferences: {},
        arguments: {},
        launch_type: "User",
        capabilities: holds,
      },
    });

    return loaded.result?.session_id;
  };

  /** What a fixture that throws at module load said on the way out. */
  const crashOf = async (session, label) => {
    const crash = await waitFor(
      (m) => m.method === "Manager/extensionCrash" && m.params.session_id === session,
      label,
    );
    return crash.params.reason;
  };

  // ---- under the module system rather than round it ----
  //
  // `process.binding` is the one that had been open the whole time: it is the
  // C++ binding `fs` is written on, it never touches `Module`, and it is one
  // deprecated call. Beside it, three built-ins nobody had named. A table of
  // dangerous modules is a denylist, and every Node release adds to it; the
  // gate now hands over a listed built-in and refuses the rest.
  const dug = await crashOf(
    await loadFixture("digs-into-node.js", "digger", []),
    "an extension digging under the module system crashed",
  );

  assert(
    /every way under was refused/.test(dug),
    `nothing got under the gate (${dug})`,
  );

  const ways = [
    "binding",
    "linkedBinding",
    "dlopen",
    "kill",
    "report",
    "v8",
    "sqlite",
    "vm",
    "registerHooks",
  ];

  for (const way of ways) {
    assert(
      new RegExp(`${way}: sill:`).test(dug),
      `${way} was refused by Sill rather than failing for some other reason`,
    );
  }

  assert(
    /kill: sill: this extension is not allowed to start other programs/.test(dug),
    "signalling somebody else's process names the permission it would need",
  );

  assert(
    /v8: sill: "node:v8" is one of Node's own modules and Sill does not hand it/.test(dug),
    "a built-in nobody listed is refused as not offered rather than as unpaid for",
  );
  assert(
    /dlopen: sill: this extension tried to load a native addon/.test(dug),
    "and a native addon is refused with a reason rather than a permission",
  );

  /*
   * The same fixture with everything granted.
   *
   * Three of the nine go through, and which three is the point.
   * `process.binding("fs")` is answered under the module it is the inside of,
   * so granting `fs` grants it, and the same for signalling a process and
   * writing a report. The other six are refused **however much is granted**,
   * because a built-in nobody listed and a native addon are not permissions
   * anybody can hold. A refusal only means something if the same call succeeds
   * when it is allowed to, and six of these never do.
   */
  const dugAllowed = await crashOf(
    await loadFixture("digs-into-node.js", "digger-allowed", [
      "fileRead",
      "fileWrite",
      "processLaunch",
    ]),
    "the same fixture ran with everything granted",
  );

  const under = dugAllowed.match(/got under the gate through ([^\n]*)/)?.[1];

  assert(
    under === "binding, kill, report",
    `the three that answer to a permission really do work when it is held, ` +
      `and the six that answer to nothing still do not (${dugAllowed})`,
  );

  // ---- the other loader ----
  //
  // A dynamic `import()` resolves through the ESM loader and never through
  // `Module._load`, so every gate on that side was beside the point for one
  // line of ordinary code. `module.registerHooks` is the same-thread hook, so
  // it can ask this worker's own permissions rather than a copy.
  const importing = async (holds, who) => {
    const session = await loadFixture("imports-dynamically.js", who, holds);
    send({ jsonrpc: "2.0", id: 9500, method: "Manager/ready", params: { session_id: session } });

    const drew = await waitFor(
      (m) =>
        m.method === "Manager/extensionMessage" &&
        m.params.session_id === session &&
        JSON.parse(m.params.payload).method === "UI/render" &&
        !JSON.stringify(JSON.parse(m.params.payload).params.ops).includes("waiting"),
      `${who} reported what its imports did`,
    );

    return JSON.stringify(JSON.parse(drew.params.payload).params.ops);
  };

  const refusedImports = await importing([], "no-import");

  for (const shape of ["prefixed", "bare", "dataurl"]) {
    assert(
      refusedImports.includes(`${shape}-refused`),
      `import() of ${shape} node:fs is refused when the permission is not held ` +
        `(${refusedImports})`,
    );
  }

  const allowedImports = await importing(["fileRead", "fileWrite"], "yes-import");

  for (const shape of ["prefixed", "bare", "dataurl"]) {
    assert(
      allowedImports.includes(`${shape}-reached`),
      `and reaches the disk once it is, so the refusal means something (${allowedImports})`,
    );
  }

  // ---- code an extension writes itself ----
  //
  // `eval`, `new Function` and `WebAssembly` were written down as holes in the
  // gate. They are not: none of them reaches a built-in, because `require` is
  // not in scope for generated code and every global that is has a gate on it.
  // What they defeat is the store's scan, which is a different claim and is
  // now made separately.
  const generated = await crashOf(
    await loadFixture("builds-its-own-code.js", "generator", []),
    "an extension generating its own code crashed",
  );

  assert(
    /generated code reached nothing/.test(generated),
    `nothing an extension can write for itself reaches a built-in (${generated})`,
  );
  assert(
    /function-require: unavailable|function-require: nothing/.test(generated),
    `\`require\` is not visible to a generated function at all (${generated})`,
  );
  assert(
    /direct-eval-require: refused/.test(generated),
    "a direct eval sees the module's own require, which is the gated one",
  );
  assert(
    /function-getBuiltinModule: refused/.test(generated),
    "and the global route out of generated code is wrapped",
  );
  assert(
    /wasm-import: refused/.test(generated),
    "WebAssembly reaches only what JavaScript hands it, and that is gated too",
  );

  const generatedAllowed = await crashOf(
    await loadFixture("builds-its-own-code.js", "generator-allowed", ["fileRead", "fileWrite"]),
    "the same fixture ran with the files permission",
  );

  assert(
    /reached the disk through .*direct-eval-require/.test(generatedAllowed),
    `the generated escapes really do work when allowed (${generatedAllowed})`,
  );

  // ---- @raycast/utils ----
  //
  // The package the store is written against. Loading an extension that uses
  // its hooks is the only check that means anything: the hooks have to resolve,
  // run inside the real React reconciler, and produce a tree.
  const utilsLoad = await request("Manager/load", {
    opts: {
      mode: "View",
      env: "Development",
      entrypoint: resolve(root, "test/fixture/uses-utils.js"),
      extension_name: "utils-user",
      command_name: "fruit",
      is_raycast: true,
      preferences: {},
      arguments: {},
      launch_type: "User",
      capabilities: [],
    },
  });

  const utilsSession = utilsLoad.result?.session_id;

  send({
    jsonrpc: "2.0",
    id: 9200,
    method: "Manager/ready",
    params: { session_id: utilsSession },
  });

  const utilsDrew = await waitFor(
    (m) =>
      m.method === "Manager/extensionMessage" &&
      m.params.session_id === utilsSession &&
      JSON.parse(m.params.payload).method === "UI/render" &&
      JSON.stringify(JSON.parse(m.params.payload).params.ops).includes("Apple"),
    "an extension using usePromise drew its rows",
  );

  const utilsOps = JSON.stringify(JSON.parse(utilsDrew.params.payload).params.ops);
  assert(utilsOps.includes("Pear"), "usePromise resolved and both rows arrived");
  assert(utilsOps.includes("avatar-ok"), "getAvatarIcon produced an inline SVG");
  assert(utilsOps.includes("never"), "useCachedState fell back to its initial value");
  // The published `@raycast/utils` hands React `cache.subscribe` without the
  // cache, so a command that rendered at all is a command whose `subscribe`
  // survived being detached.
  assert(
    utilsOps.includes("detached-ok"),
    "Cache.subscribe worked when React called it without its cache",
  );

  // ---- something utils does not have here ----
  const missing = await request("Manager/load", {
    opts: {
      mode: "View",
      env: "Development",
      entrypoint: resolve(root, "test/fixture/utils-missing.js"),
      extension_name: "utils-missing",
      command_name: "apple",
      is_raycast: true,
      preferences: {},
      arguments: {},
      launch_type: "User",
      capabilities: [],
    },
  });

  const missingCrash = await waitFor(
    (m) =>
      m.method === "Manager/extensionCrash" &&
      m.params.session_id === missing.result?.session_id,
    "an extension reaching for AppleScript failed",
  );

  assert(
    /runAppleScript/.test(missingCrash.params.reason),
    `and the error names what it wanted (${missingCrash.params.reason})`,
  );
  assert(
    /macOS only/.test(missingCrash.params.reason),
    "and says why it cannot work here",
  );
  // ---- the network, which no require ever mentions ----
  //
  // `fetch` is a global, so the module gate never sees it. Without this an
  // extension reaches the internet with no permission at all, which would have
  // made the whole gate a fig leaf.
  const netLoad = async (capabilities, name) => {
    const loaded = await request("Manager/load", {
      opts: {
        mode: "View",
        env: "Development",
        entrypoint: resolve(root, "test/fixture/wants-network.js"),
        extension_name: name,
        command_name: "get",
        is_raycast: true,
        preferences: {},
        arguments: {},
        launch_type: "User",
        capabilities,
      },
    });

    const id = loaded.result?.session_id;
    send({ jsonrpc: "2.0", id: 9300, method: "Manager/ready", params: { session_id: id } });

    const drew = await waitFor(
      (m) =>
        m.method === "Manager/extensionMessage" &&
        m.params.session_id === id &&
        JSON.parse(m.params.payload).method === "UI/render" &&
        !JSON.stringify(JSON.parse(m.params.payload).params.ops).includes("waiting"),
      `${name} reported what happened to its request`,
    );

    return JSON.stringify(JSON.parse(drew.params.payload).params.ops);
  };

  assert(
    (await netLoad([], "no-net")).includes("gated"),
    "fetch is refused when the network was not granted",
  );
  assert(
    (await netLoad(["network"], "yes-net")).includes("reachable"),
    "and is the real one once it is",
  );

  // ---- taking a permission away from something already running ----
  //
  // The test that matters for a revoke, and the one nothing could pass before.
  // Capabilities arrived once, in the launch payload, so revoking in Settings
  // wrote the file, satisfied the next launch, and reached the worker on
  // screen not at all: the extension somebody had just revoked went on reading
  // the disk and reaching the network until something unloaded it.
  //
  // One worker throughout. It is granted, it uses both gates, the permissions
  // are taken away, and it is asked to use them again without being reloaded.
  const living = await request("Manager/load", {
    opts: {
      mode: "View",
      env: "Development",
      entrypoint: resolve(root, "test/fixture/keeps-using-what-it-was-given.js"),
      extension_name: "second-thoughts",
      command_name: "use",
      is_raycast: true,
      preferences: {},
      arguments: {},
      launch_type: "User",
      capabilities: ["fileRead", "fileWrite", "network"],
    },
  });

  const livingSession = living.result?.session_id;
  await request("Manager/ready", { session_id: livingSession });

  /*
   * What this session draws next, ignoring what it has already drawn.
   *
   * `waitFor` searches the whole inbox, which is right everywhere else here
   * and wrong for one worker asked the same question three times: the render
   * from before a revoke is still sitting there and matches. So each call
   * starts from where the last one ended, and a stale match cannot answer for
   * a fresh one. Without it, the "granted again" check passes on the render
   * from before the revoke and proves nothing at all.
   */
  let readFrom = 0;
  const drawn = async (pattern, label) => {
    const from = readFrom;

    const message = await waitFor(
      (m, at) =>
        at >= from &&
        m.method === "Manager/extensionMessage" &&
        m.params.session_id === livingSession &&
        JSON.parse(m.params.payload).method === "UI/render" &&
        pattern.test(JSON.stringify(JSON.parse(m.params.payload).params.ops)),
      label,
    );

    readFrom = inbox.indexOf(message) + 1;
    return JSON.stringify(JSON.parse(message.params.payload).params.ops);
  };

  /*
   * The handler the launcher activates to make it try again.
   *
   * Read off the first render rather than off the one being waited for below.
   * A render is a patch: the first carries the whole tree including the
   * action, and the one that follows the state change carries a changed title
   * and nothing else. The handler id survives, because it is the same worker
   * and the same callback on both sides of the revoke.
   */
  const created = await waitFor(
    (m) =>
      m.method === "Manager/extensionMessage" &&
      m.params.session_id === livingSession &&
      JSON.parse(m.params.payload).method === "UI/render" &&
      JSON.stringify(JSON.parse(m.params.payload).params.ops).includes("$handler"),
    "the fixture drew an action to activate",
  );

  const tryAgainId = JSON.stringify(JSON.parse(created.params.payload).params.ops).match(
    /"\$handler":"([^"]+)"/,
  )?.[1];
  assert(typeof tryAgainId === "string", `found the action's handler (${tryAgainId})`);

  const whileAllowed = await drawn(/disk-/, "the granted worker reported what it reached");

  assert(
    whileAllowed.includes("disk-reached") && whileAllowed.includes("net-reached"),
    `a granted worker really does reach both, so a later refusal means ` +
      `something (${whileAllowed})`,
  );

  const askAgain = (id) =>
    request("Manager/messageExtension", {
      session_id: livingSession,
      payload: JSON.stringify({
        jsonrpc: "2.0",
        id,
        method: "EventCore/handlerActivated",
        params: { id: tryAgainId, args: [] },
      }),
    });

  // **The revoke.** Nothing is reloaded and nothing is unloaded.
  const told = await request("Manager/setCapabilities", {
    session_id: livingSession,
    capabilities: [],
  });
  assert(told.result === true, "the host accepted a capability change for a live session");

  await askAgain(9400);

  const afterRevoke = await drawn(
    /disk-refused|net-refused/,
    "the same worker reported again after the revoke",
  );

  assert(
    afterRevoke.includes("disk-refused"),
    `the running worker is refused the disk it was reading a moment ago (${afterRevoke})`,
  );
  assert(
    afterRevoke.includes("net-refused"),
    `and the network it was reaching, which is the other gate (${afterRevoke})`,
  );
  assert(
    /not allowed to read and change files/.test(afterRevoke),
    "the refusal names the permission rather than saying no",
  );
  assert(/Settings/.test(afterRevoke), "and says where to turn it back on");

  // Given back, to the same worker, without a reload. A gate that can only
  // ever close would make every revoke permanent by accident.
  await request("Manager/setCapabilities", {
    session_id: livingSession,
    capabilities: ["fileRead", "fileWrite", "network"],
  });

  await askAgain(9401);

  const afterRegrant = await drawn(
    /disk-reached|disk-refused/,
    "the same worker again after the permission came back",
  );

  assert(
    afterRegrant.includes("disk-reached") && afterRegrant.includes("net-reached"),
    `granting reaches a running worker too (${afterRegrant})`,
  );

  assert(
    (await request("Manager/setCapabilities", { session_id: "no-such-session", capabilities: [] }))
      .result === false,
    "a revoke arriving after the command closed is nothing to tell rather than a failure",
  );

  await request("Manager/unload", { session_id: livingSession });
} catch (err) {
  console.error(`FAIL ${err.message}`);
  failures++;
} finally {
  child.kill();
}

console.log(failures === 0 ? "\nall integration checks passed" : `\n${failures} failure(s)`);
process.exit(failures === 0 ? 0 : 1);
