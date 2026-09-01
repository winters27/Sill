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

const raycastApi = new Proxy(api as Record<string, unknown>, {
  get(target, prop, receiver) {
    if (typeof prop === "symbol" || prop in target) {
      return Reflect.get(target, prop, receiver);
    }
    if (INTEROP_KEYS.has(prop)) return undefined;
    return unsupported(prop);
  },
  has() {
    // Report every key as present so `"x" in api` guards do not silently pick
    // a fallback path; the throwing getter is what reports a real gap.
    return true;
  },
});

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
  http: { needs: ["network"], plainly: "make web requests" },
  https: { needs: ["network"], plainly: "make web requests" },
  http2: { needs: ["network"], plainly: "make web requests" },
  // Not networking or files itself, but it starts code that is neither gated
  // nor visible from here, which is the same thing with an extra step.
  worker_threads: { needs: ["processLaunch"], plainly: "start other programs" },
  // Reaches the Node internals the rest of this table is built on.
  inspector: { needs: ["processLaunch"], plainly: "start other programs" },
};

/** What a module id asks for, if anything. */
function gateOf(id: string): { needs: string[]; plainly: string } | undefined {
  const bare = id.startsWith("node:") ? id.slice(5) : id;
  const root = bare.split("/")[0];
  return root ? GATED[root] : undefined;
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
 * It is **not** a sandbox in the sense of containing hostile code. An
 * extension determined to get out has `process.binding`, `eval` and
 * `module.createRequire` to try, and this stops none of them. Saying so
 * plainly matters more than the feature does: the honest claim is that Sill
 * shows you what extensions reach and refuses what you have not allowed, not
 * that a malicious extension is powerless.
 */
export function patchRequire(granted: readonly string[] = []): void {
  const overrides: Record<string, () => unknown> = {
    react: () => React,
    "react/jsx-runtime": () => jsxRuntime,
    "react/jsx-dev-runtime": () => jsxDevRuntime,
    "react-reconciler": () => ReactReconciler,
    "@raycast/api": () => raycastApi,
    // Extensions built against Raycast's utils package are common enough that
    // resolving it to a clear error beats a module-not-found stack.
    "@raycast/utils": () => unsupported("@raycast/utils"),
  };

  const originalRequire = Module.prototype.require;

  const held = new Set(granted);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (Module.prototype as any).require = function patched(this: unknown, id: string) {
    const override = overrides[id];
    if (override) return override();

    // Checked before the module is resolved, so a refused one is never even
    // loaded. Node caches a module the first time it is required, and a check
    // written after resolution would leave it in that cache for whatever asks
    // next.
    const gate = gateOf(id);

    if (gate && !gate.needs.every((need) => held.has(need))) {
      throw new Error(
        `sill: this extension is not allowed to ${gate.plainly}, so "${id}" is unavailable. ` +
          `Grant it in Settings, under Extensions, then run the command again.`,
      );
    }

    return originalRequire.call(this as never, id);
  };
}
