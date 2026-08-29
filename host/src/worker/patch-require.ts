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

export function patchRequire(): void {
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

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (Module.prototype as any).require = function patched(this: unknown, id: string) {
    const override = overrides[id];
    if (override) return override();
    return originalRequire.call(this as never, id);
  };
}
