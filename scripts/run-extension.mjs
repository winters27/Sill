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
 * Usage: node scripts/run-extension.mjs <entrypoint.js> [extensionName]
 *          [--seed key=json] [--no-view] [--grant fileRead,network]
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

/*
 * What the extension is allowed to reach.
 *
 * Empty by default, which is what the launcher does for an extension nobody
 * has granted anything to. That default is worth knowing about: the worker
 * refuses `fs`, `net` and `child_process` at `require`, which is module load,
 * so an ungranted extension dies before it renders and the run looks
 * identical to a no-view command that did nothing.
 *
 * `--grant fileRead,network` supplies them, in the names Rust serialises, so
 * this runner can exercise what the app actually does after somebody accepts
 * an install.
 */
const grantArg = args.indexOf("--grant");
const granted =
  grantArg === -1 || !args[grantArg + 1]
    ? []
    : args[grantArg + 1].split(",").map((one) => one.trim()).filter(Boolean);

/**
 * Where `environment.assetsPath` points.
 *
 * Rust hands this to every command from the installed extension's own folder,
 * and this harness passed nothing, so an extension that reads a data file it
 * ships with looked to itself like one running against an empty disk. Not
 * defaulted to the extension's directory: the entrypoint given here is a
 * bundle in `extensions/build`, which is not where the assets are.
 */
const assetsArg = args.indexOf("--assets");
const assetsPath = assetsArg === -1 ? "" : resolve(args[assetsArg + 1] ?? "");

/**
 * The thing this command was run on, when it is being run as an action.
 *
 * `--on '{"kind":"file","target":"C:/notes/todo.md"}'`. JSON rather than a
 * pair of flags because a Windows path has a colon in it and every shorthand
 * that seemed readable turned out to be ambiguous with one.
 *
 * `id`, `title` and `mode` are filled in when they are left out, because Rust
 * always sends all five and an extension reading `on.title` should not see an
 * `undefined` that only this harness can produce. What cannot be defaulted is
 * `kind`, which is the whole point of the flag.
 */
const onArg = args.indexOf("--on");
const actedOn = onArg === -1 ? undefined : describeTarget(JSON.parse(args[onArg + 1] ?? "{}"));

function describeTarget(given) {
  if (!given.kind || !given.target) {
    console.error('--on needs at least {"kind":"file","target":"..."}');
    process.exit(1);
  }

  return {
    kind: given.kind,
    id: given.id ?? `${given.kind}:${given.target}`,
    target: given.target,
    title: given.title ?? given.target.split(/[/\\]/).pop() ?? given.target,
    mode: given.mode ?? given.kind,
  };
}

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
const { collectActions, shortcutKeys, isRunnable, toastActions } = await import(
  pathToFileURL(actionsOut).href
);

// The search field's rules, bundled the same way. What this reports about
// `filtering` and about which rows survive a query is what the window would
// draw, rather than a second implementation of it that could agree with a bug.
const searchOut = join(scratchDir("sill-search-"), "search.mjs");
await new Promise((done, fail) => {
  const p = spawn(
    process.execPath,
    [
      join(root, "host", "node_modules", "esbuild", "bin", "esbuild"),
      join(root, "src", "lib", "exthost", "search.ts"),
      "--bundle",
      "--format=esm",
      `--outfile=${searchOut}`,
      "--log-level=error",
    ],
    { stdio: "inherit" },
  );
  p.on("exit", (c) => (c === 0 ? done() : fail(new Error(`esbuild exited ${c}`))));
});
const { itemsOf, rowsOf, searchProps } = await import(pathToFileURL(searchOut).href);

// What the views draw beyond a title: icons, accessories, empty views,
// dropdowns and detail panes. Bundled from the window's own module for the
// same reason as the three above, and it matters more here than anywhere: a
// second reader of `accessories` would agree with itself and with nothing on
// screen.
const presentOut = join(scratchDir("sill-present-"), "present.mjs");
await new Promise((done, fail) => {
  const p = spawn(
    process.execPath,
    [
      join(root, "host", "node_modules", "esbuild", "bin", "esbuild"),
      join(root, "src", "lib", "exthost", "present.ts"),
      "--bundle",
      "--format=esm",
      `--outfile=${presentOut}`,
      "--log-level=error",
    ],
    { stdio: "inherit" },
  );
  p.on("exit", (c) => (c === 0 ? done() : fail(new Error(`esbuild exited ${c}`))));
});
const { accessoriesOf, detailOf, dropdownOf, emptyViewOf, iconOf, showsDetail } = await import(
  pathToFileURL(presentOut).href
);

/**
 * Whether to say what this extension costs as well as what it draws.
 *
 * Off by default, because it opens the command a second time and asks the host
 * for a reading, which is a second or two nobody watching a view gate wants.
 * Turned on, it answers the two questions the Extensions panel answers, from
 * the same host and the same protocol the application uses:
 *
 *   how long from asking for it to seeing it, cold and warm
 *   how much memory the worker is holding once it has settled
 *
 * Cold and warm are genuinely different runs rather than one run reported
 * twice. Cold is this process starting Node, the host bundle evaluating, a
 * worker thread being created and the extension's own modules loading. Warm is
 * the same command opened again in the host that is already up, against the
 * spare worker that was spun up when the first one was claimed.
 */
const measuring = args.includes("--measure");

// ---- host process ----
const startedHostAt = performance.now();
const child = spawn(process.execPath, [hostJs], { stdio: ["pipe", "pipe", "pipe"] });

/**
 * Everything the extension wrote to the console, kept as well as shown.
 *
 * The host relays an extension's `console.log` to its own stderr, which makes
 * it the one place a test can see something happen that leaves no mark on the
 * tree. A component that was mounted said so; one that was never mounted said
 * nothing, and the difference between those two is what `Action.Push` is for.
 */
let said = "";
child.stderr.on("data", (d) => {
  said += d;
  process.stderr.write(`[host] ${d}`);
});

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
 * Manager replies this script is waiting for, keyed by the id it asked with.
 *
 * The load is answered by hand above because it arrives before anything else
 * exists. Everything else goes through here, so asking the host a question and
 * reading the answer is one shape rather than a special case per question.
 */
const waiting = new Map();

/** Asks the host something and waits for the answer. */
function ask(id, method, params = {}) {
  const answer = new Promise((resolve) => waiting.set(id, resolve));
  send({ jsonrpc: "2.0", id, method, params });
  return answer;
}

/**
 * When each session first put something on screen.
 *
 * The first render and nothing earlier, which is the same moment Sill's own
 * measurement ends. An extension reading its saved settings has not appeared
 * yet, and counting that as arrival would report the slow ones as fast.
 */
const firstRender = new Map();

/** The last thing the command said about its own view stack. */
let navigation = { depth: 1, pop: "" };

/**
 * The toast on screen, if there is one.
 *
 * Kept the way the window keeps it, buttons and all, because a toast is the
 * only thing a command can put in front of somebody that leaves no mark on the
 * tree. Its buttons go through `toastActions`, which is the window's own
 * reader, so what this reports is what the chin would draw rather than a
 * second opinion about the same payload.
 *
 * Null after `UI/hideToast`, which is how an extension takes its own message
 * away and is the half a button that closes the toast is testing.
 */
let toast = null;

/**
 * The last thing the command flashed across the launcher.
 *
 * Counted before this existed and never read, so a command whose whole result
 * is a HUD ("Copied the path of todo.md") could only be checked as far as
 * "something was shown". The text is the result for a `no-view` command.
 */
let hud = null;

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
      renders += 1;
      lastRenderAt = Date.now();
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
    case "UI/navigation":
      // What the window keeps: how deep the command is in its own stack, and
      // the handler id that takes it back one. Recorded rather than ignored,
      // because "Escape pops" is only testable if this half arrives.
      navigation = { depth: params.depth ?? 1, pop: params.pop ?? "" };
      return null;
    case "UI/showToast":
    case "UI/updateToast":
      /*
       * Filtered the way Rust filters it, because a button with no handler is
       * one the window refuses to draw and asserting on the raw payload would
       * count a button nobody can press.
       */
      toast = {
        title: params.title ?? "",
        message: params.message ?? "",
        style: params.style ?? "",
        actions: toastActions(
          (Array.isArray(params.actions) ? params.actions : []).filter(
            (one) => typeof one.handler === "string" && one.handler,
          ),
        ),
      };
      return null;
    case "UI/hideToast":
      toast = null;
      return null;
    case "UI/showHud":
      hud = String(params.text ?? params.title ?? "");
      return null;
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

    const waiter = waiting.get(msg.id);
    if (waiter) {
      waiting.delete(msg.id);
      waiter(msg.result);
    }

    if (msg.method === "Manager/extensionCrash") {
      console.error(`\nCRASH: ${msg.params.reason}\n`);
      continue;
    }

    if (msg.method !== "Manager/extensionMessage") continue;

    const call = JSON.parse(msg.params.payload);
    if (!call.method) continue;

    if (call.method === "UI/render" && !firstRender.has(msg.params.session_id)) {
      firstRender.set(msg.params.session_id, performance.now());
    }

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

/**
 * How many times the extension has drawn, and when it last did.
 *
 * Written where the renders arrive rather than inferred, because the point of
 * waiting is to know whether one has happened and a wait cannot ask the tree:
 * an extension that renders an empty list looks exactly like one that has not
 * rendered at all.
 */
let renders = 0;
let lastRenderAt = 0;
const waits = [];

/**
 * Waits for the extension to finish drawing, rather than for a fixed time.
 *
 * This used to be `settle(2500)`, which is generous on a developer's machine
 * and not on a shared build agent. **It is why CI was red**: the gate read the
 * tree before the first render landed and reported zero rows, zero
 * accessories, an undefined root. Always zero, never a wrong number, and a
 * different extension each run, which is what looking too early looks like
 * rather than what a broken renderer looks like.
 *
 * Quiet rather than merely present, because several extensions draw an empty
 * list and fill it once a promise settles. So: wait for at least one render,
 * then for `quiet` with no further one. An extension that never draws at all
 * still ends at `patience`, and the assertions then fail on what it drew,
 * which is the honest failure.
 *
 * Faster than the sleep it replaces in the ordinary case, which is the point
 * of a condition over a guess.
 */
function rowsDrawnNow() {
  const top = tree.top();
  if (!top || (top.tag !== "List" && top.tag !== "Grid")) return 0;
  return itemsOf(rowsOf(tree, top, "")).length;
}

async function untilDrawn({
  patience = 20_000,
  quiet = 1_500,
  // What this run is about to assert on: enough rows, enough dropdown options,
  // a toast whose title is about to be read. Answered once, the wait is over.
  arrived = undefined,
  called = "it",
} = {}) {
  const began = Date.now();
  const until = began + patience;

  /*
   * Renders from **this** point on, not renders ever.
   *
   * The first version counted every render since the worker started, so the
   * second call returned instantly: the initial draw had long since gone
   * quiet, and `Action.Push` read the tree before the pushed screen existed.
   * It reported the list it had pushed from, which reads exactly like a push
   * that did not happen.
   */
  const before = renders;

  while (Date.now() < until) {
    // Enough is enough, quiet or not: a list still redrawing has already
    // answered the question about to be asked of it.
    if (arrived && arrived()) {
      waits.push(`${called} after ${Date.now() - began}ms`);
      return true;
    }

    /*
     * Quiet only counts when nothing specific was asked for.
     *
     * `visual-studio-code-recent-projects` draws an empty list, goes quiet for
     * well past the quiet period while it reads the recent-projects file, then
     * fills in the dropdown. Letting quiet end a wait that is looking for five
     * dropdown options returned it after 1 ms with none of them, which is the
     * same "read it too early" failure this function exists to prevent, coming
     * back in through the fallback.
     *
     * When the caller named a condition, the condition or the deadline ends
     * the wait. Nothing else.
     */
    if (!arrived && renders > before && Date.now() - lastRenderAt >= quiet) {
      waits.push(`${renders - before} render(s) in ${lastRenderAt - began}ms`);
      return true;
    }
    await settle(50);
  }

  // Out of patience. Said rather than swallowed: a gate that fails on "got 0"
  // without saying whether anything ever drew sends the next person to read
  // the renderer, which is where I looked first and it was not there.
  waits.push(
    arrived
      ? `never reached ${called} in ${patience}ms (${renders - before} render(s) meanwhile)`
      : renders > before
        ? `${renders - before} render(s), still drawing after ${patience}ms`
        : `nothing drew in ${patience}ms`,
  );

  return renders > before;
}

/** Text to type into the search field once the first render has landed. */
const typedArg = args.indexOf("--type");
const typed = typedArg === -1 ? undefined : args[typedArg + 1];

/**
 * Types into the field the way the window does.
 *
 * The window has no special channel for this: `onSearchTextChange` is an
 * ordinary prop, so it arrives as a handler id like every other callback and
 * is fired through the same `EventCore/handlerActivated` request. Doing it
 * here the same way is the point, because an extension that answers this is an
 * extension the window can search.
 *
 * The handler comes from `searchProps`, which is the window's own reader,
 * rather than off the prop bag directly. Reaching past it made this gate pass
 * while Sill read the prop nowhere: the host answered, and the half that
 * decides whether the window ever asks was never exercised.
 */
function type(handler, text) {
  return activate(handler, [text]);
}

/**
 * Fires one handler, which is the only thing the window can say to a worker.
 *
 * Typing, pressing an action and going back a screen are all this call with a
 * different id, and that is the design rather than a coincidence: the stack
 * lives in the worker and the window drives it through the same stable ids it
 * already had. Anything here that needed a channel of its own would be a
 * second protocol.
 */
let activations = 0;

function activate(handler, args = []) {
  if (!handler) return false;

  send({
    jsonrpc: "2.0",
    method: "Manager/messageExtension",
    id: 900 + activations,
    params: {
      session_id: session,
      payload: JSON.stringify({
        jsonrpc: "2.0",
        id: 9000 + activations++,
        method: "EventCore/handlerActivated",
        params: { id: handler, args },
      }),
    },
  });

  return true;
}

/** The value after a named argument, or undefined when it was not given. */
function argAfter(name) {
  const at = args.indexOf(name);
  return at === -1 ? undefined : args[at + 1];
}

/** The first Action.Push offered by an item, if it offers one. */
function pushAction(subject) {
  return collectActions(tree, subject).find((a) => a.tag === "Action.Push");
}

/**
 * The row a person would be on when the view first appears.
 *
 * Sections included, because an extension that groups its rows still has a
 * first one and it is the one under the highlight.
 */
function firstItemOf(root) {
  const itemTag = root.tag === "Grid" ? "Grid.Item" : "List.Item";
  const sectionTag = root.tag === "Grid" ? "Grid.Section" : "List.Section";

  for (const child of tree.elementChildren(root)) {
    if (child.tag === itemTag) return child;
    if (child.tag !== sectionTag) continue;
    const inside = tree.elementChildren(child).find((c) => c.tag === itemTag);
    if (inside) return inside;
  }

  // A Detail or a Form has no rows and hangs its actions off itself.
  return root;
}

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
      assets_path: assetsPath,
      preferences: manifestPreferences(),
      arguments: {},
      launch_type: "User",
      capabilities: granted,
      // Absent unless `--on` was given, exactly as Rust leaves it out for a
      // command somebody picked off the root list.
      ...(actedOn ? { on: actedOn } : {}),
    },
  },
});

await settle(500);
send({ jsonrpc: "2.0", id: 2, method: "Manager/ready", params: { session_id: session } });

/*
 * Wait for what is about to be asserted, not merely for the tree to go quiet.
 *
 * Quiet was the first fix and it was not enough. `kill-process` draws its list
 * and its "Processes" section immediately and fills the section once it has
 * asked the machine what is running. On a build agent that answer takes longer
 * than any quiet period worth waiting, so the gate read an empty section and
 * reported zero rows: the same "looked too early" failure one level down.
 *
 * The caller already says how many rows it expects. Waiting for that number
 * turns the deadline into the assertion: the rows arrive and it goes on, or
 * patience runs out and the count it prints is the real one.
 */
// A dropdown is not a row, so waiting for rows never covers it, and
// `hacker-news` is asserted on its dropdown alone. Same wait, different count.
const wantsDropdown = Number(argAfter("--expect-dropdown") ?? 0);

function dropdownDrawnNow() {
  const top = tree.top();
  return top ? (dropdownOf(tree, top)?.options.length ?? 0) : 0;
}

const wantsRows = Math.max(
  Number(argAfter("--expect-icons") ?? 0),
  Number(argAfter("--expect-accessories") ?? 0) > 0 ? 1 : 0,
  Number(argAfter("--expect-rows") ?? 0),
);

/*
 * Everything this run is about to claim about the first draw.
 *
 * Not a chosen subset: whatever is asserted has to be waited for, or the
 * assertion is a race. Adding an `--expect-` flag without adding it here is
 * how CI goes red on somebody else's machine and nowhere else.
 */
const wantsRoot = argAfter("--expect-root");
const wantsItems = Number(argAfter("--expect-items") ?? 0);
const wantsActions = Number(argAfter("--expect-actions") ?? 0);
const wantsToastActions = Number(argAfter("--expect-toast-actions") ?? 0);

const wanted = [
  wantsRoot !== undefined && {
    enough: () => tree.top()?.tag === wantsRoot,
    said: `a <${wantsRoot}>`,
  },
  wantsRows > 0 && {
    enough: () => rowsDrawnNow() >= wantsRows,
    said: `${wantsRows} row(s)`,
  },
  wantsItems > 0 && {
    enough: () => {
      const top = tree.top();
      if (!top) return false;
      const tag = `${top.tag}.Item`;
      return tree.elementChildren(top).filter((c) => c.tag === tag).length >= wantsItems;
    },
    said: `${wantsItems} item(s)`,
  },
  wantsActions > 0 && {
    enough: () => {
      const top = tree.top();
      return top ? collectActions(tree, firstItemOf(top)).length >= wantsActions : false;
    },
    said: `${wantsActions} action(s) on the first row`,
  },
  wantsDropdown > 0 && {
    enough: () => dropdownDrawnNow() >= wantsDropdown,
    said: `${wantsDropdown} dropdown option(s)`,
  },
  wantsToastActions > 0 && {
    enough: () => (toast?.actions.length ?? 0) >= wantsToastActions,
    said: `${wantsToastActions} toast button(s)`,
  },
  args.includes("--expect-detail") && {
    enough: () => {
      const top = tree.top();
      return top
        ? itemsOf(rowsOf(tree, top, "")).some((item) => detailOf(tree, item))
        : false;
    },
    said: "a detail pane",
  },
  /*
   * A no-view command draws nothing, so quiet is all it ever had, and the two
   * things the gate then greps out of this log are a clipboard write and a
   * toast that both arrive on their own schedule. Waiting for them is the same
   * fix as everywhere else here.
   */
  argAfter("--expect-called") !== undefined && {
    enough: () => {
      const [method, n] = String(argAfter("--expect-called")).split("=");
      return calls.filter((one) => one === method).length >= Number(n || 1);
    },
    said: `a call to ${argAfter("--expect-called")}`,
  },
  args.includes("--expect-toast") && {
    enough: () => toast !== null,
    said: "a toast",
  },
  argAfter("--expect-toast-title") !== undefined && {
    // These two runs also assert a negative, that nothing reached the
    // clipboard. A negative read before the command got going passes for the
    // wrong reason, which is worse than a red gate.
    enough: () => toast?.title === argAfter("--expect-toast-title"),
    said: `a toast saying ${JSON.stringify(argAfter("--expect-toast-title"))}`,
  },
  argAfter("--expect-hud") !== undefined && {
    enough: () => hud === argAfter("--expect-hud"),
    said: "the HUD",
  },
  args.includes("--expect-empty-view") && {
    enough: () => {
      const top = tree.top();
      return top ? emptyViewOf(tree, top) !== undefined : false;
    },
    said: "an EmptyView",
  },
].filter(Boolean);

await untilDrawn(
  wanted.length
    ? {
        // Every one of them, not the first to land. A run asking for rows and
        // a dropdown that stopped at the dropdown would read the rows early,
        // which is the failure this whole change exists to remove.
        arrived: () => wanted.every((one) => one.enough()),
        called: wanted.map((one) => one.said).join(" and "),
      }
    : {},
);

/**
 * What the field said, and what it reached.
 *
 * `heard` is whether the extension was told, `before`/`after` are how many
 * rows the window would draw either side of the typing. An extension doing its
 * own searching moves `after` by re-rendering; one Sill filters moves it
 * without the extension being involved at all. Both are worth seeing.
 */
const field = { asked: typed, heard: false, before: 0, after: 0, props: undefined };

if (typed !== undefined) {
  const before = tree.top();
  field.props = searchProps(before);
  field.before = before ? itemsOf(rowsOf(tree, before, "")).length : 0;
  field.heard = type(field.props.onChange, typed);

  /*
   * Until the handler has finished re-rendering, rather than for a fixed time.
   * An extension that filters in memory answers in one frame and one that
   * fetches takes as long as it takes.
   *
   * Waiting for a render was wrong in both directions. `List.filtering` hands
   * the narrowing to the host, so the extension never draws again and the wait
   * burned its full patience on every run: 20 s the gate did not need, and
   * invisible, because a wait that is merely wasted looks exactly like a wait
   * that was needed. An extension that refetches instead may take longer than
   * any figure worth hard-coding.
   *
   * So wait for the count this run is about to assert, which is right for both
   * and needs no guess about which kind of extension this is.
   */
  const narrowingTo = Number(argAfter("--expect-rows") ?? 0);
  const narrowedNow = () => {
    const now = tree.top();
    if (!now) return 0;
    return itemsOf(rowsOf(tree, now, field.props?.filtering ? typed : "")).length;
  };

  /*
   * Only when there is something to narrow. If the list already sits at the
   * expected count, "narrowed enough" is true of the tree before typing, and
   * the wait would return on the state it is supposed to be waiting past.
   */
  const willNarrow = narrowingTo > 0 && field.before > narrowingTo;

  await untilDrawn(
    willNarrow
      ? {
          arrived: () => narrowedNow() <= narrowingTo,
          called: `the list down to ${narrowingTo} row(s)`,
        }
      : { quiet: 500 },
  );

  const after = tree.top();
  const narrow = field.props?.filtering ? typed : "";
  field.after = after ? itemsOf(rowsOf(tree, after, narrow)).length : 0;
}

/**
 * Opens the first row's pushed screen, then goes back the way Escape does.
 *
 * Both halves go through the window's own reader: the action comes from
 * `collectActions`, and the pop uses the handler id the worker named in its
 * navigation event. Reaching past either would leave this passing while the
 * window could not do it.
 */
const journey = {
  wanted: args.includes("--push"),
  found: false,
  /** Whether the target had already been mounted before anything was pressed. */
  eager: false,
  pushedTo: undefined,
  depth: 0,
  poppedTo: undefined,
};

const lazyFor = argAfter("--expect-lazy");

if (journey.wanted) {
  const before = tree.top();
  const subject = before ? firstItemOf(before) : undefined;
  const action = subject ? pushAction(subject) : undefined;

  journey.eager = lazyFor !== undefined && said.includes(lazyFor);
  journey.found = activate(action?.handler);

  // A push draws the screen it went to and a pop draws the one underneath, so
  // wait for that screen by name when the run says which one it expects. The
  // tag it is about to read is the whole assertion; quiet is only the fallback
  // for a run that did not say.
  const goingTo = argAfter("--expect-pushed");
  const backTo = argAfter("--expect-popped");
  const wasAt = navigation.depth;

  await untilDrawn(
    goingTo !== undefined
      ? {
          // Depth as well as tag. A screen pushed onto one of its own kind
          // already has the expected tag before the push lands, and the wait
          // would end on the screen it started from.
          arrived: () => navigation.depth > wasAt && tree.top()?.tag === goingTo,
          called: `a <${goingTo}>`,
        }
      : { quiet: 400 },
  );
  journey.pushedTo = tree.top()?.tag;
  journey.depth = navigation.depth;

  const wentTo = navigation.depth;
  activate(navigation.pop);
  await untilDrawn(
    backTo !== undefined
      ? {
          arrived: () => navigation.depth < wentTo && tree.top()?.tag === backTo,
          called: `back to a <${backTo}>`,
        }
      : { quiet: 400 },
  );
  journey.poppedTo = tree.top()?.tag;
}

/**
 * Presses a button on the toast, the way the chin does.
 *
 * Through `activate`, which is the same call typing and pressing an action
 * use, because a toast button is an ordinary handler id and nothing about it
 * being on a toast changes how it is run. A path of its own here would leave
 * this passing while the window could not press the thing.
 *
 * What is recorded is the toast either side, since that is where the answer
 * shows: Raycast hands the handler the live toast, so a button that rewrites
 * its own message proves the handle it got was the one on screen.
 */
const pressed = {
  wanted: args.includes("--press-toast"),
  before: undefined,
  after: undefined,
  found: false,
};

if (pressed.wanted) {
  pressed.before = toast?.title;
  pressed.found = activate(toast?.actions?.[0]?.handler);
  // The title this is about to read is the whole assertion, so wait for it to
  // change rather than for a second to pass. Same reason as `untilDrawn`: a
  // sleep long enough here is a sleep too short on a slower machine.
  await untilDrawn({
    patience: 10_000,
    arrived: () => toast?.title !== pressed.before,
    called: "the toast retitled",
  });
  pressed.after = toast?.title;
}

/**
 * What this extension costs, when anybody asked.
 *
 * The cold figure is the run that has already happened above: this process
 * started Node, the host evaluated, a worker was created and the extension
 * loaded into it. The warm figure opens the same command again in the same
 * host, which is what somebody gets when they close a command and open it
 * again, or open a second extension while the first is still resident.
 *
 * The memory reading is taken of the warm session, through `Manager/
 * diagnostics`, which is the same call the Extensions panel makes. It is asked
 * once, here, and never sampled: a timer taking readings of an idle worker is
 * the wakeup this whole project refuses to spend.
 */
const cost = { coldMs: undefined, warmMs: undefined, reading: undefined };

if (measuring) {
  cost.coldMs = firstRender.has(session)
    ? Math.round(firstRender.get(session) - startedHostAt)
    : undefined;

  await ask(3, "Manager/unload", { session_id: session });

  const warmFrom = performance.now();
  const again = await ask(4, "Manager/load", {
    opts: {
      mode: modeArg,
      env: "Development",
      entrypoint,
      extension_name: extensionName,
      extension_id: extensionName,
      command_name: "cmd",
      is_raycast: true,
      assets_path: assetsPath,
      preferences: manifestPreferences(),
      arguments: {},
      launch_type: "User",
      capabilities: granted,
    },
  });

  await ask(5, "Manager/ready", { session_id: again.session_id });
  await settle(2500);

  cost.warmMs = firstRender.has(again.session_id)
    ? Math.round(firstRender.get(again.session_id) - warmFrom)
    : undefined;

  const diagnostics = await ask(6, "Manager/diagnostics");
  cost.reading = (diagnostics?.workers ?? []).find(
    (one) => one.session_id === again.session_id,
  );
}

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

if (typed !== undefined) {
  console.log("\nSearch field:");
  console.log(`  filtering: ${field.props?.filtering ? "Sill" : "the extension"}`);
  console.log(`  throttle: ${field.props?.throttle ?? false}`);
  console.log(`  isLoading: ${field.props?.loading ?? false}`);
  console.log(`  onSearchTextChange: ${field.heard ? "fired" : "none registered"}`);
  console.log(`  rows ${field.before} -> ${field.after} after typing ${JSON.stringify(typed)}`);
}

/**
 * What the window would draw beyond the titles.
 *
 * Reported for every run rather than behind a flag, because this is the
 * question P4-01 asks and the answer is different for every extension. A list
 * whose rows carry six accessories and a detail pane is a list Sill either
 * draws or does not, and until this printed it the only way to find out was to
 * install the thing and look.
 */
const drawn = { icons: 0, accessories: 0, dropdown: 0, detail: false, empty: false };

if (top && (top.tag === "List" || top.tag === "Grid")) {
  for (const item of itemsOf(rowsOf(tree, top, ""))) {
    if (iconOf(item.props.icon)) drawn.icons++;
    drawn.accessories += accessoriesOf(item).length;
    if (detailOf(tree, item)) drawn.detail = true;
  }
  drawn.dropdown = dropdownOf(tree, top)?.options.length ?? 0;
  drawn.empty = emptyViewOf(tree, top) !== undefined;

  console.log("\nDrawn beyond the title:");
  console.log(`  rows with an icon: ${drawn.icons}`);
  console.log(`  accessories in total: ${drawn.accessories}`);
  console.log(`  dropdown options: ${drawn.dropdown}`);
  console.log(`  detail pane: ${drawn.detail} (list asks for one: ${showsDetail(top)})`);
  console.log(`  EmptyView: ${drawn.empty}`);
}

if (hud !== null) {
  console.log(`\nHUD: ${JSON.stringify(hud)}`);
}

if (toast || pressed.wanted) {
  console.log("\nToast:");
  console.log(`  on screen: ${toast ? JSON.stringify(toast.title) : "none"}`);
  console.log(`  style: ${toast?.style ?? "-"}`);
  for (const button of toast?.actions ?? []) {
    const keys = button.shortcut ? `  [${shortcutKeys(button.shortcut).join(" ")}]` : "";
    console.log(`  button: ${button.title}${keys}`);
  }
  if (pressed.wanted) {
    console.log(`  pressed the first: ${pressed.found}`);
    console.log(`  said ${JSON.stringify(pressed.before)} -> ${JSON.stringify(pressed.after)}`);
  }
}

if (journey.wanted) {
  console.log("\nNavigation:");
  console.log(`  Action.Push found: ${journey.found}`);
  console.log(`  pushed to: <${journey.pushedTo}> at depth ${journey.depth}`);
  console.log(`  popped back to: <${journey.poppedTo}>`);
  if (lazyFor !== undefined) {
    console.log(`  target mounted before it was pushed: ${journey.eager}`);
  }
}

if (measuring) {
  const ms = (value) => (value === undefined ? "never drew" : `${value} ms`);
  const mb = (bytes) =>
    bytes === undefined || bytes === null
      ? "did not answer"
      : `${(bytes / 1024 / 1024).toFixed(1)} MB`;

  console.log("\nWhat it costs:");
  console.log(`  cold, with Node to start: ${ms(cost.coldMs)}`);
  console.log(`  warm, host already up:    ${ms(cost.warmMs)}`);
  console.log(`  heap once settled:        ${mb(cost.reading?.heap_bytes)}`);
  console.log(`  of a limit of:            ${mb(cost.reading?.heap_limit_bytes)}`);
  console.log(`  share of one core since:  ${cost.reading?.core_percent ?? "?"}%`);
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

/*
 * What typing was supposed to do.
 *
 * `--expect-heard` is the wire being connected at all: the extension was told
 * what was typed. `--expect-rows` is the answer arriving, whether the
 * extension narrowed it or Sill did. The pair is what `P4-02` is done when.
 */
const expectHeard = args.includes("--expect-heard");
const expectRows = flag("--expect-rows");

/*
 * Who was supposed to be narrowing the rows, "sill" or "extension".
 *
 * Worth asserting on its own because getting it backwards is silent: a list
 * Sill wrongly filters still draws rows, just the wrong ones, and a list Sill
 * wrongly leaves alone looks like an extension that ignores typing.
 */
const expectFiltering = flag("--expect-filtering");

if (expectFiltering !== undefined) {
  const who = field.props?.filtering ? "sill" : "extension";
  check(who === expectFiltering, `${expectFiltering} filters this list, got ${who}`);
}

if (expectHeard) {
  check(field.heard, "the extension was told what was typed");
  /*
   * The list answering is the half that cannot be faked by firing a handler
   * into a void. Not an exact count: an extension that searches its own data
   * decides how many rows a word matches, and pinning that number here would
   * make this gate fail whenever somebody upstream changed their matcher.
   */
  check(
    field.after !== field.before,
    `the extension re-rendered in answer, ${field.before} rows -> ${field.after}`,
  );
}

if (expectRows !== undefined) {
  check(
    field.after === Number(expectRows),
    `${expectRows} row(s) after typing, got ${field.after}`,
  );
  check(field.after !== field.before, `typing changed the list, was ${field.before}`);
}

/*
 * What pressing a Push action was supposed to do.
 *
 * Three separate claims, because they fail separately. `--expect-pushed` is
 * the second screen arriving; `--expect-popped` is going back, which is the
 * half that turns a one-way door into navigation; `--expect-lazy` is the cost
 * of the first, and it is the one that cannot be seen on screen: a target
 * mounted with every row is a working push and a hundred components nobody
 * asked for.
 */
const expectPushed = flag("--expect-pushed");
const expectPopped = flag("--expect-popped");

/*
 * What the rows carry, as counts rather than as a picture.
 *
 * Each is a lower bound, because the numbers belong to somebody else's
 * extension: pinning them exactly would fail this gate whenever an author
 * added a row. What must not change is that they are drawn at all, which is
 * what every one of these was not doing before.
 */
const atLeast = (name, actual, what) => {
  const wanted = flag(name);
  if (wanted === undefined) return;
  check(actual >= Number(wanted), `at least ${wanted} ${what}, got ${actual}`);
};

atLeast("--expect-icons", drawn.icons, "rows with an icon");
atLeast("--expect-accessories", drawn.accessories, "accessories");
atLeast("--expect-dropdown", drawn.dropdown, "dropdown options");

if (args.includes("--expect-detail")) {
  check(drawn.detail, "the selected row hands over a detail pane");
}

if (args.includes("--expect-empty-view")) {
  check(drawn.empty, "the list declares its own words for being empty");
}

/*
 * A field the root view was supposed to declare, and how many of it.
 *
 * `--expect-field Form.FilePicker=2`. Written as one flag with a count rather
 * than as a pair, because the interesting failure is a form drawing one of a
 * kind and dropping the rest, and two flags could each be right about a
 * different thing.
 *
 * Exact rather than a floor. This is asserted against fixtures, where the
 * number is the fixture's own and changing it is a decision somebody made.
 */
const expectField = flag("--expect-field");

if (expectField !== undefined) {
  const [tag, howMany] = expectField.split("=");
  const found = top ? tree.elementChildren(top).filter((one) => one.tag === tag).length : 0;

  check(
    found === Number(howMany ?? 1),
    `the view declares ${howMany ?? 1} <${tag}>, got ${found}`,
  );
}

/*
 * What the toast was supposed to carry, and what pressing it was supposed to
 * do.
 *
 * Three separate claims because they fail separately. The buttons arriving is
 * the wire; the press is the window reaching the extension's own code through
 * the ordinary handler channel; the words changing is Raycast's contract that
 * the handler is given the live toast, which is the half an extension uses to
 * turn "could not reach the server" into "trying again" without drawing
 * anything.
 */
const expectToastActions = flag("--expect-toast-actions");

if (expectToastActions !== undefined) {
  check(toast !== null, "the extension put a toast on screen");
  check(
    (toast?.actions.length ?? 0) === Number(expectToastActions),
    `the toast carries ${expectToastActions} button(s), got ${toast?.actions.length ?? 0}`,
  );
  check(
    (toast?.actions ?? []).every((one) => one.handler),
    "every button on it is one the window can press",
  );
}

/**
 * What the command flashed across the launcher, in full.
 *
 * The result of a `no-view` command is usually a HUD and nothing else, so this
 * is the only way to assert one did the right thing rather than merely
 * something. Exact, because the words are the extension's own.
 */
const expectCalled = flag("--expect-called");

if (expectCalled !== undefined) {
  const [method, n] = String(expectCalled).split("=");
  const made = calls.filter((one) => one === method).length;
  const times = Number(n || 1);
  check(made === times, `${method} called ${times} time(s), got ${made}`);
}

const expectHud = flag("--expect-hud");

if (expectHud !== undefined) {
  check(hud === expectHud, `the HUD says ${JSON.stringify(expectHud)}, got ${JSON.stringify(hud)}`);
}

/** The same for a toast, for the commands that report failure with one. */
const expectToastTitle = flag("--expect-toast-title");

if (expectToastTitle !== undefined) {
  check(
    toast?.title === expectToastTitle,
    `the toast says ${JSON.stringify(expectToastTitle)}, got ${JSON.stringify(toast?.title ?? null)}`,
  );
}

const expectToastSaid = flag("--expect-toast-said");

if (pressed.wanted) {
  check(pressed.found, "the toast offers a button the window can press");
}

if (expectToastSaid !== undefined) {
  check(
    pressed.after === expectToastSaid,
    `the toast says ${JSON.stringify(expectToastSaid)} after the press, got ` +
      JSON.stringify(pressed.after),
  );
  check(
    pressed.before !== pressed.after,
    `pressing changed the message, was ${JSON.stringify(pressed.before)}`,
  );
}

if (expectPushed !== undefined) {
  check(journey.found, "the first row offers an Action.Push the window can run");
  check(
    journey.pushedTo === expectPushed,
    `pushed view is <${expectPushed}>, got <${journey.pushedTo}>`,
  );
  check(journey.depth === 2, `the command is two screens deep, got ${journey.depth}`);
}

if (expectPopped !== undefined) {
  check(
    journey.poppedTo === expectPopped,
    `back at <${expectPopped}> after popping, got <${journey.poppedTo}>`,
  );
}

if (lazyFor !== undefined) {
  check(!journey.eager, `the pushed target was not mounted before it was pushed`);
  check(said.includes(lazyFor), `the pushed target was mounted when it was pushed`);
}

if (
  expectRoot !== undefined ||
  expectItems !== undefined ||
  expectActions !== undefined ||
  expectHeard ||
  expectRows !== undefined ||
  expectPushed !== undefined ||
  expectPopped !== undefined ||
  flag("--expect-icons") !== undefined ||
  flag("--expect-accessories") !== undefined ||
  flag("--expect-dropdown") !== undefined ||
  args.includes("--expect-detail") ||
  args.includes("--expect-empty-view") ||
  expectFiltering !== undefined ||
  expectToastActions !== undefined ||
  expectToastSaid !== undefined ||
  expectField !== undefined
) {
  check(gaps.size === 0, `no unimplemented API was needed, ${gaps.size} gap(s)`);
}

console.log(top ? "\nextension rendered" : "\nextension produced no view");

// How the waiting went. A failure reading "got 0" without saying whether
// anything ever drew sends the next person to read the renderer, which is
// where I looked first and it was not there.
if (waits.length) console.log(`  waited: ${waits.join("; ")}`);
process.exit(top && !failed ? 0 : 1);
