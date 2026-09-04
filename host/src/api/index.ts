/**
 * The module that extensions get when they `require("@raycast/api")`.
 *
 * Coverage is deliberately partial and grows as extensions demand it. Anything
 * missing is surfaced by the throwing proxy in patch-require rather than
 * arriving as `undefined`, so a gap looks like a gap instead of a crash three
 * frames later.
 */

import { getBridge } from "./bridge";

// `KNOWN_TAGS` deliberately not re-exported. Everything this module holds is
// a name an extension can reach and a row `docs/extensions.md` has to explain,
// and that list is the host's own note to itself about what the window draws.
export { List, Grid, Detail, Form, Action, ActionPanel } from "./components";

export {
  Toast,
  ToastHandle,
  showToast,
  showHUD,
  popToRoot,
  closeMainWindow,
  open,
  getSelectedText,
  getApplications,
  getDefaultApplication,
  getPreferenceValues,
  environment,
  Clipboard,
  LocalStorage,
} from "./runtime";

/** Drives the worker's view stack. */
export function useNavigation(): { push: (view: unknown) => void; pop: () => void } {
  const { navigation } = getBridge();
  return {
    push: (view: unknown) => navigation.push(view),
    pop: () => navigation.pop(),
  };
}

export const Icon = new Proxy(
  {},
  {
    // Icons are plain string identifiers on the wire and the UI resolves them,
    // so any name an extension asks for round-trips rather than failing here.
    get: (_t, prop: string) => (prop === "$$typeof" ? undefined : prop),
  },
) as Record<string, string>;

export const Color = {
  Blue: "raycast-blue",
  Green: "raycast-green",
  Magenta: "raycast-magenta",
  Orange: "raycast-orange",
  Purple: "raycast-purple",
  Red: "raycast-red",
  Yellow: "raycast-yellow",
  PrimaryText: "raycast-primary-text",
  SecondaryText: "raycast-secondary-text",
} as const;

export const Keyboard = {
  Shortcut: {
    Common: {
      Copy: { modifiers: ["cmd", "shift"], key: "c" },
      CopyDeeplink: { modifiers: ["cmd", "shift"], key: "c" },
      Duplicate: { modifiers: ["cmd"], key: "d" },
      Edit: { modifiers: ["cmd"], key: "e" },
      MoveDown: { modifiers: ["cmd", "shift"], key: "arrowDown" },
      MoveUp: { modifiers: ["cmd", "shift"], key: "arrowUp" },
      New: { modifiers: ["cmd"], key: "n" },
      Open: { modifiers: ["cmd"], key: "o" },
      OpenWith: { modifiers: ["cmd", "shift"], key: "o" },
      Pin: { modifiers: ["cmd", "shift"], key: "p" },
      Refresh: { modifiers: ["cmd"], key: "r" },
      Remove: { modifiers: ["ctrl"], key: "x" },
      RemoveAll: { modifiers: ["ctrl", "shift"], key: "x" },
    },
  },
} as const;

export const Alert = {
  ActionStyle: { Default: "default", Destructive: "destructive", Cancel: "cancel" },
} as const;

/** Controls what happens to the root search after a HUD or window close. */
export const PopToRootType = {
  Default: "default",
  Immediate: "immediate",
  Suspended: "suspended",
} as const;

/** Where a launch came from. Extensions branch on this in their entrypoints. */
export const LaunchType = {
  UserInitiated: "userInitiated",
  Background: "background",
} as const;

export async function confirmAlert(options: Record<string, unknown>): Promise<boolean> {
  return getBridge().request<boolean>("UI/confirmAlert", { payload: options });
}

export const Image = {
  Mask: { Circle: "circle", RoundedRectangle: "roundedRectangle" },
} as const;

// Raycast's synchronous cache, over storage that is not. The trade it makes is
// in the file itself; it was the second most-wanted API the host did not
// answer, stopping 13 of 124 commands across the twelve most-installed
// extensions.
export { Cache, type CacheOptions } from "./cache";
