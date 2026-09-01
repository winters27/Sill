import { invoke } from "@tauri-apps/api/core";

/**
 * Saved window arrangements.
 *
 * The launcher only ever names one and puts one back; everything about which
 * window is which, and what to do when the displays have changed since, is
 * decided in Rust where the window list lives.
 */

/** Saves where everything is now. Returns how many windows were remembered. */
export function saveWorkspace(name: string): Promise<number> {
  return invoke<number>("save_workspace", { name });
}

/** Puts one back. Returns how many windows were actually moved. */
export function restoreWorkspace(name: string): Promise<number> {
  return invoke<number>("restore_workspace", { name });
}

export function forgetWorkspace(name: string): Promise<void> {
  return invoke("forget_workspace", { name });
}
