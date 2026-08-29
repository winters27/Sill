/**
 * The seam between the extension-facing API and the worker that hosts it.
 *
 * Extension code imports "@raycast/api" and calls functions on it. Those
 * functions need to reach the Rust host, but the API module must not know how
 * that happens, so the worker installs a bridge before running the entrypoint.
 *
 * Vicinae does the equivalent by assigning `globalThis.vicinae`. This is the
 * same idea with a narrower surface and real types.
 */

import type { Renderer } from "../render/renderer";
import type { RpcParams } from "../proto/rpc";

export interface Environment {
  extensionName: string;
  commandName: string;
  commandMode: "view" | "no-view";
  assetsPath: string;
  supportPath: string;
  isDevelopment: boolean;
  raycastVersion: string;
  textSize: "medium" | "large";
  launchType: "userInitiated" | "background";
}

/** The worker owns the view stack; useNavigation just drives it. */
export interface Navigation {
  push(view: unknown): void;
  pop(): void;
}

export interface Bridge {
  /** Calls a host method and waits for its result, e.g. "Clipboard/copy". */
  request<T = unknown>(method: string, params?: RpcParams): Promise<T>;
  /** Fire-and-forget notification to the host. */
  emit(method: string, params?: RpcParams): void;
  renderer: Renderer;
  navigation: Navigation;
  environment: Environment;
  preferences: Record<string, unknown>;
  launchArguments: Record<string, unknown>;
}

let current: Bridge | undefined;

export function setBridge(bridge: Bridge): void {
  current = bridge;
}

export function getBridge(): Bridge {
  if (!current) {
    // Reaching here means API code ran outside a worker, which is a wiring
    // bug rather than anything an extension author can cause.
    throw new Error("sill: extension API used before the host bridge was installed");
  }
  return current;
}

export function hasBridge(): boolean {
  return current !== undefined;
}
