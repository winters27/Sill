/**
 * Redirects an extension's imports to the host's own modules.
 *
 * Extensions are bundled against `@raycast/api` and `react`. Both must resolve
 * to the copies the host already has: the API because our implementation is
 * what talks to the host, and React because two React instances in one worker
 * break hooks in ways that are miserable to diagnose.
 */

import Module from "node:module";
import * as React from "react";
import * as jsxRuntime from "react/jsx-runtime";
import * as jsxDevRuntime from "react/jsx-dev-runtime";
import ReactReconciler from "react-reconciler";
import * as api from "../api";
import * as utils from "../utils";

/** Keys a bundler probes for module interop. They must stay undefined. */
const INTEROP_KEYS = new Set(["__esModule", "default", "then", "module.exports"]);

/**
 * Unimplemented APIs throw on access rather than reading as `undefined`.
 * Undefined propagates silently and surfaces as a confusing failure much later;
 * a throw names the missing symbol at the moment it is touched.
 */
function unsupported(name: string): never {
  throw new Error(
    `sill: "${name}" is not implemented yet. ` +
      `It is part of the Raycast API surface Sill has not covered. ` +
      `Please report which extension needed it.`,
  );
}

/**
 * The same treatment for both packages: anything present is handed over,
 * anything absent throws its own name.
 *
 * Written once rather than twice because the second copy is the one that stops
 * matching when the first is changed.
 */
function throwingProxy(module: Record<string, unknown>): Record<string, unknown> {
  return new Proxy(module, {
    get(target, prop, receiver) {
      if (typeof prop === "symbol" || prop in target) {
        return Reflect.get(target, prop, receiver);
      }
      if (INTEROP_KEYS.has(prop)) return undefined;
      return unsupported(prop);
    },
    has() {
      // Report every key as present so `"x" in api` guards do not silently
      // pick a fallback path; the throwing getter reports the real gap.
      return true;
    },
  });
}

const raycastApi = throwingProxy(api as Record<string, unknown>);
const raycastUtils = throwingProxy(utils as unknown as Record<string, unknown>);

/**
 * Node built-ins that hand an extension something outside its own process.
 *
 * Keyed by the first segment, so `fs/promises` and `node:fs` are both `fs`.
 * The capability names are the ones Rust serialises, so a permission denied
 * here is the same permission listed in Settings and asked for on the card.
 *
 * `fs` needs read **and** write together, on purpose. Handing an extension the
 * module hands it both, and there is no honest way to offer one without the
 * other, so the permission asked for is the permission actually given.
 */
const GATED: Record<string, { needs: string[]; plainly: string }> = {
  fs: { needs: ["fileRead", "fileWrite"], plainly: "read and change files directly" },
  child_process: { needs: ["processLaunch"], plainly: "start other programs" },
  net: { needs: ["network"], plainly: "open network connections" },
  tls: { needs: ["network"], plainly: "open network connections" },
  dgram: { needs: ["network"], plainly: "open network connections" },
  // A name lookup is a packet to somebody else's server, and a query is a
  // place to put whatever you wanted to send them.
  dns: { needs: ["network"], plainly: "open network connections" },
  http: { needs: ["network"], plainly: "make web requests" },
  https: { needs: ["network"], plainly: "make web requests" },
  http2: { needs: ["network"], plainly: "make web requests" },
  // Not networking or files itself, but it starts code that is neither gated
  // nor visible from here, which is the same thing with an extra step.
  worker_threads: { needs: ["processLaunch"], plainly: "start other programs" },
  // Forks the current program, which is `child_process` wearing a hat.
  cluster: { needs: ["processLaunch"], plainly: "start other programs" },
  // Reaches the Node internals the rest of this table is built on.
  inspector: { needs: ["processLaunch"], plainly: "start other programs" },
};

/**
 * Node built-ins an extension may have for nothing.
 *
 * **This list is the security boundary, and the table above is not.** A table
 * of dangerous modules is a denylist: every Node release that adds a built-in
 * adds a hole, and the older version of this file had several already.
 * `node:dns` resolves names, `node:sqlite` opens database files, `node:v8`
 * writes a heap snapshot to any path it is given, `node:wasi` pre-opens
 * directories for a WebAssembly module, and `node:cluster` forks the process.
 * None of them was named, so all five were handed over to an extension with no
 * permissions at all.
 *
 * So the question is inverted. A built-in is handed over if it is on this
 * list; if it is gated it needs its permission; and **anything else is refused
 * whether or not anybody thought of it**. A built-in added in a future Node
 * arrives refused rather than arriving open.
 *
 * What is on it is what an extension computes with: text, time, hashing,
 * compression, streams, paths, and the shape of the machine. Nothing here
 * reads a file, opens a socket or starts a process. Keyed by first segment, so
 * `stream/web`, `util/types` and `timers/promises` are the root beside them.
 */
const FREE = new Set([
  "assert",
  "async_hooks",
  "buffer",
  "console",
  "constants",
  "crypto",
  "diagnostics_channel",
  "domain",
  "events",
  "module",
  "os",
  "path",
  "perf_hooks",
  "process",
  "punycode",
  "querystring",
  "readline",
  "stream",
  "string_decoder",
  "sys",
  "timers",
  "tty",
  "url",
  "util",
  "zlib",
]);

/**
 * `process.binding` under the module it is the inside of.
 *
 * `process.binding("fs")` hands back the same filesystem the `fs` module is
 * written on top of, and it is neither a module nor a require, so nothing in
 * this file used to see it. It is deprecated and has been for years, which is
 * why it was easy to forget and why no honest extension calls it.
 *
 * Mapped rather than refused outright so it answers to the same permission the
 * module does: a binding is never more than the module built on it, so an
 * extension allowed `fs` is not handed anything new by `binding("fs")`. A
 * binding with no module beside it is refused, because there is nothing to
 * weigh it against.
 */
const BINDINGS: Record<string, string> = {
  fs: "fs",
  fs_event_wrap: "fs",
  spawn_sync: "child_process",
  process_wrap: "child_process",
  tcp_wrap: "net",
  pipe_wrap: "net",
  stream_wrap: "net",
  tls_wrap: "tls",
  udp_wrap: "dgram",
  cares_wrap: "dns",
  http_parser: "http",
  inspector: "inspector",
};

/**
 * What this worker is allowed to reach, right now.
 *
 * **A `Set` built once at launch was the bug.** The gates below closed over
 * the array that arrived with the command, so taking a permission away in
 * Settings reached the file on disk, reached the next launch, and reached
 * nothing at all in the worker already running: an extension somebody had just
 * revoked went on reading the disk and reaching the network until it was
 * unloaded. A permission that can be revoked and does not take effect is worse
 * than one that cannot, because the person believes they have taken it away.
 *
 * So the gates hold this object and ask it per call, and Rust replaces its
 * contents whenever what the extension holds changes. That costs one property
 * read on a path that was already a set lookup.
 *
 * An object owned by `workerMain` and handed to the gates rather than a module
 * variable they both reach for. One worker runs one command, so the difference
 * never shows up at runtime, and it shows up immediately in a test: this can be
 * made, driven and thrown away, which a module global cannot.
 */
export class Held {
  private granted: Set<string>;

  constructor(granted: readonly string[] = []) {
    this.granted = new Set(granted);
  }

  has(need: string): boolean {
    return this.granted.has(need);
  }

  /** What the extension holds now, replacing what it held before. */
  replace(granted: readonly string[]): void {
    this.granted = new Set(granted);
  }
}

/** The refusal every gate in this file raises, so all of them read alike. */
function refusal(plainly: string, what: string): Error {
  return new Error(
    `sill: this extension is not allowed to ${plainly}, so ${what} is unavailable. ` +
      `Grant it in Settings, under Extensions, then run the command again.`,
  );
}

/**
 * What a module specifier is, as far as this gate is concerned.
 *
 * `pass` is everything that is not a Node built-in: the extension's own
 * bundle, its relative files, and anything it brought with it. Those are code
 * it already has, and gating them would gate the extension against itself.
 */
type Verdict =
  | { kind: "pass" }
  | { kind: "free" }
  | { kind: "gated"; needs: string[]; plainly: string }
  | { kind: "unlisted" };

/**
 * Whether an extension may have this module.
 *
 * `Module.isBuiltin` is asked rather than the `node:` prefix being stripped by
 * hand, because Node's own answer is the one that gets the corners right:
 * `sqlite` is an ordinary package on npm and `node:sqlite` is a built-in, and
 * a hand-rolled strip would refuse the first for being the second.
 */
function decide(id: string): Verdict {
  if (!Module.isBuiltin(id)) return { kind: "pass" };

  const bare = id.startsWith("node:") ? id.slice(5) : id;
  const root = bare.split("/")[0] ?? "";

  const gate = GATED[root];
  if (gate) return { kind: "gated", needs: gate.needs, plainly: gate.plainly };
  if (FREE.has(root)) return { kind: "free" };
  return { kind: "unlisted" };
}

/**
 * Refuses a module the extension has not been allowed to have.
 *
 * ## What this is and is not
 *
 * It is a permission boundary: an extension cannot read the disk, start a
 * program or open a socket without somebody having agreed to it, and the
 * refusal names the permission so it can be granted.
 *
 * It is **not** a sandbox in the sense of containing hostile code. Saying so
 * plainly matters more than the feature does: the honest claim is that Sill
 * shows you what extensions reach and refuses what you have not allowed, not
 * that a malicious extension is powerless.
 *
 * ## The supported ways to a built-in, and where each one is met
 *
 * Five, and all five arrive at [`decide`]. "Supported" is doing work in that
 * sentence: these are the routes Node documents, and closing them is what
 * makes this a boundary an ordinary extension meets rather than containment.
 *
 * | Route | Where it is met |
 * | --- | --- |
 * | `require`, `Module._load`, `module.createRequire` | `Module._load` |
 * | `process.getBuiltinModule` | wrapped here |
 * | `process.binding`, `process._linkedBinding` | wrapped here, under [`BINDINGS`] |
 * | `import()`, and any `import` inside what it loads | `module.registerHooks` |
 * | `process.dlopen` | refused outright; a native addon is not a module |
 *
 * ## Why this is stubs and not Node's permission model
 *
 * `node --permission` is a **process** flag. Sill runs one Node process for
 * every extension at once, one worker thread per command, plus a warm spare
 * waiting for a command nobody has chosen yet, so a process-wide allowlist
 * would have to be the union of what every installed extension might ever be
 * granted, decided before the first one is launched. Node has no per-thread
 * permission set and no way to narrow one after start except
 * `permission.deny`, which is process-wide and one-way. Worker threads under
 * `--permission` inherit the process's set entire.
 *
 * So a per-worker allowlist is not reachable that way, and the thing that
 * makes it reachable this way is that these gates are ordinary code running
 * **inside** the worker, closed over that worker's own [`Held`]. The same
 * property is what lets a revoke reach a command already on screen, which the
 * permission model could not have done either.
 *
 * ## What still gets out, named
 *
 * A granted permission is total: `fileRead` is every file the person can
 * open, not the extension's own. A dependency does whatever the extension
 * does, since they share a worker. `processLaunch` is the end of the
 * conversation, because the program it starts is outside all of this.
 * `process.env` is readable, and Sill's own environment is in it.
 *
 * `eval`, `new Function` and `WebAssembly` are **not** on that list any more,
 * and it is worth saying why, because they were. None of them reaches a
 * built-in: `require` is a parameter of the module scope rather than a global,
 * so neither `new Function` nor an indirect `eval` can see it, a direct `eval`
 * sees the gated one, and every global route they do have is wrapped above.
 * What they defeat is the **store's scan**, which reads source text and cannot
 * see a module name that is assembled at runtime. That is a limit on the
 * description, not a hole in the gate, and the two are said separately now.
 */
export function patchRequire(held: Held): void {
  const overrides: Record<string, () => unknown> = {
    react: () => React,
    "react/jsx-runtime": () => jsxRuntime,
    "react/jsx-dev-runtime": () => jsxDevRuntime,
    "react-reconciler": () => ReactReconciler,
    "@raycast/api": () => raycastApi,
    "@raycast/utils": () => raycastUtils,
  };

  /**
   * Throws unless the extension holds what this module needs.
   *
   * Checked before the module is resolved, so a refused one is never even
   * loaded. Node caches a module the first time it is required, and a check
   * written after resolution would leave it in that cache for whatever asks
   * next. That is also what makes a revoke bite a worker that has already
   * required the module once: the cached copy is still handed out by
   * `_load`, and `_load` asks this first.
   */
  const refuseUnlessAllowed = (id: string) => {
    const verdict = decide(id);

    if (verdict.kind === "pass" || verdict.kind === "free") return;

    if (verdict.kind === "unlisted") {
      throw new Error(
        `sill: "${id}" is one of Node's own modules and Sill does not hand it to ` +
          `extensions. No permission turns it on. If an extension genuinely needs ` +
          `it, that is a gap in Sill rather than a setting.`,
      );
    }

    if (verdict.needs.every((need) => held.has(need))) return;

    throw refusal(verdict.plainly, `"${id}"`);
  };

  /*
   * The gate goes on `_load`, not on `require`.
   *
   * `Module.prototype.require` is one way in and it was the only one guarded.
   * Everything else that loads a builtin ends up in `Module._load`:
   * `require` itself, the `require` that `module.createRequire` hands back,
   * and `Module._load` called directly, which is two words in any extension
   * and completely defeated the older gate. Guarding the place they all reach
   * costs nothing and closes all three at once.
   */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const loader = Module as any;
  const originalLoad = loader._load;

  loader._load = function patchedLoad(
    this: unknown,
    request: string,
    parent: unknown,
    isMain: boolean,
  ) {
    const override = overrides[request];
    if (override) return override();

    refuseUnlessAllowed(request);
    return originalLoad.call(this, request, parent, isMain);
  };

  /*
   * And the one that never goes near `Module` at all.
   *
   * `process.getBuiltinModule("fs")` hands back the builtin directly. It is
   * documented, supported, and has been in Node since 22.3, which is the
   * runtime this host is built for, so it was not an exotic escape: it was the
   * shortest one.
   */
  const process_ = process as NodeJS.Process & {
    getBuiltinModule?: (id: string) => unknown;
    binding?: (name: string) => unknown;
    _linkedBinding?: (name: string) => unknown;
    dlopen?: (module: unknown, path: string, flags?: number) => void;
  };
  const originalBuiltin = process_.getBuiltinModule?.bind(process);

  if (originalBuiltin) {
    process_.getBuiltinModule = (id: string) => {
      refuseUnlessAllowed(id);
      return originalBuiltin(id);
    };
  }

  /*
   * The inside of the modules above, which is not a module.
   *
   * `process.binding("fs")` returns the C++ binding `require("fs")` is built
   * on. It never touches `Module`, it is not a specifier anything here would
   * recognise, and it worked: an extension with no permissions read files with
   * it. Deprecated since Node 10 and still present in Node 25, which is the
   * combination that gets something forgotten.
   *
   * Answered under the module it belongs to, so `binding("fs")` costs the same
   * permission `require("fs")` does and a binding with no module beside it is
   * refused rather than guessed at.
   */
  for (const name of ["binding", "_linkedBinding"] as const) {
    const original = process_[name]?.bind(process);
    if (!original) continue;

    process_[name] = (binding: string) => {
      const module = BINDINGS[binding];
      if (!module) {
        throw new Error(
          `sill: "process.${name}(${JSON.stringify(binding)})" reaches inside Node ` +
            `itself, and Sill does not hand that to extensions. No permission turns ` +
            `it on.`,
        );
      }
      refuseUnlessAllowed(module);
      return original(binding);
    };
  }

  /*
   * And machine code, which no permission describes.
   *
   * `process.dlopen` loads a native addon into this process. Whatever it loads
   * is outside every gate in this file for the rest of the worker's life, and
   * there is no honest permission to hang it on: "run arbitrary native code"
   * is not a thing to put on a card beside "read your files". So it is refused
   * for everybody, the way `runAppleScript` is, and says so rather than
   * naming a permission that would not help.
   */
  if (process_.dlopen) {
    process_.dlopen = () => {
      throw new Error(
        "sill: this extension tried to load a native addon, which Sill does not " +
          "allow. Native code runs outside every permission Sill can offer, so " +
          "there is nothing to grant.",
      );
    };
  }

  gateEsmImports(refuseUnlessAllowed);
}

/**
 * The other loader, which `Module` never sees.
 *
 * `await import("node:fs")` resolves through the ESM loader. It shares nothing
 * with `Module._load`, so every gate above was beside the point for one line
 * of perfectly ordinary code, and this was the escape the last pass wrote down
 * and left open.
 *
 * `module.registerHooks` is the synchronous, same-thread hook API, and
 * same-thread is the whole reason it is the right one: the hook can ask this
 * worker's own [`Held`] at the moment of the call, so a revoke reaches
 * `import()` exactly the way it reaches `require`. `module.register` runs
 * hooks on a separate loader thread and would have needed a message round trip
 * inside a resolve, which is a lock waiting to happen.
 *
 * Registered hooks are a chain, most recent first, and one that never calls
 * `next` decides the answer alone. So an extension could register its own and
 * step in front of this one, which is why the two entry points to that chain
 * are closed behind it. Ours is registered first and last.
 */
function gateEsmImports(refuseUnlessAllowed: (id: string) => void): void {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const loader = Module as any;

  if (typeof loader.registerHooks !== "function") {
    throw new Error(
      "sill: this Node is too old to run extensions safely. Sill needs Node 22.15 " +
        "or newer, which is where module.registerHooks arrived; without it a " +
        "dynamic import() would walk straight past the permission gate. Update " +
        "Node from nodejs.org.",
    );
  }

  loader.registerHooks({
    resolve(
      specifier: string,
      context: unknown,
      nextResolve: (specifier: string, context: unknown) => unknown,
    ) {
      refuseUnlessAllowed(specifier);
      return nextResolve(specifier, context);
    },
  });

  const shut = () => {
    throw new Error(
      "sill: an extension may not install module loader hooks. They would run " +
        "ahead of the ones enforcing its permissions.",
    );
  };

  loader.registerHooks = shut;
  loader.register = shut;
}

/**
 * Puts the network behind the same permission the module gate uses.
 *
 * `patchRequire` gates `require("http")` and its neighbours, and would have
 * been a fig leaf on its own: `fetch` is a global in modern Node, so an
 * extension could reach the network without requiring anything at all. The
 * module gate stopped the older way of doing it and left the current one open.
 *
 * Wrapped rather than deleted, so an extension that tries gets the same
 * sentence naming the permission that a refused `require` gets, instead of
 * "fetch is not a function" from somewhere in a bundled dependency.
 *
 * **The wrappers go on whether or not the network is granted**, and that is
 * the whole of what makes revoking mean anything here. This used to return
 * early for a worker that had the permission, which left the real `fetch` in
 * place for the life of the command: taking the network away in Settings
 * reached the next launch and never the extension that was busy using it. The
 * cost of always wrapping is one set lookup per request, against a call that
 * opens a socket.
 */
export function gateGlobals(held: Held): void {
  const why = (what: string) => refusal("open network connections", what);

  const globals = globalThis as unknown as Record<string, unknown>;

  /*
   * `fetch` rejects; the rest throw.
   *
   * Real `fetch` returns a promise and reports a failure by rejecting it, so a
   * stub that throws synchronously fails in a shape no caller is written for:
   * code that only attaches `.catch` never sees it, and the error escapes as
   * an unhandled throw from module scope instead. The constructors are the
   * other way round, since `new WebSocket(...)` can only fail by throwing.
   */
  const realFetch = globals.fetch;
  if (typeof realFetch === "function") {
    const call = realFetch as (...args: unknown[]) => Promise<unknown>;

    Object.defineProperty(globals, "fetch", {
      configurable: true,
      writable: true,
      value: (...args: unknown[]): Promise<unknown> =>
        held.has("network") ? call.apply(globals, args) : Promise.reject(why("fetch")),
    });
  }

  for (const name of ["WebSocket", "XMLHttpRequest", "EventSource"]) {
    const real = globals[name];
    if (typeof real !== "function") continue;

    const Real = real as new (...args: unknown[]) => object;

    Object.defineProperty(globals, name, {
      configurable: true,
      writable: true,
      // Constructed through the real one rather than subclassing it, so an
      // extension that is allowed the network gets the genuine object with
      // its own prototype, not a wrapper that fails `instanceof` somewhere
      // deep inside a dependency.
      value: function gated(...args: unknown[]): object {
        if (!held.has("network")) throw why(name);
        return Reflect.construct(Real, args, Real);
      },
    });
  }

  /*
   * Two more on `process`, which is a global like the ones above and was
   * treated as though it were only a source of `env` and `platform`.
   *
   * `process.kill(pid)` signals any process the person running Sill could
   * signal, which is what an extension granted "start other programs" is being
   * trusted with anyway and is not something to hand an extension that was
   * granted nothing. `process.report.writeReport(path)` writes a diagnostic
   * report to whatever path it is given, which is a file write however it is
   * described.
   *
   * Neither is a module, so no module gate would ever have seen them, and both
   * are one line to reach.
   */
  const process_ = process as unknown as {
    kill: (pid: number, signal?: string | number) => true;
    report?: { writeReport?: (...args: unknown[]) => string };
  };

  const realKill = process_.kill.bind(process);
  process_.kill = (pid: number, signal?: string | number) => {
    if (!held.has("processLaunch")) throw refusal("start other programs", "process.kill");
    return realKill(pid, signal);
  };

  const report = process_.report;
  const realWrite = report?.writeReport?.bind(report);

  if (report && realWrite) {
    report.writeReport = (...args: unknown[]) => {
      if (!held.has("fileWrite")) {
        throw refusal("read and change files directly", "process.report.writeReport");
      }
      return realWrite(...args);
    };
  }
}
