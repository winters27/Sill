/**
 * Reading the action set out of a rendered view.
 *
 * Actions arrive as a `$slot` subtree rather than a prop, because element
 * props are lifted into children by the API layer. Both the action panel and
 * the Enter key need the same list, so the walk lives here rather than being
 * written twice with slightly different rules.
 */

import { isHandlerRef, type ElementNode, type ViewTree } from "./tree";

export interface Shortcut {
  modifiers: string[];
  key: string;
}

export interface ActionEntry {
  /** Node id, unique and stable enough to key a list on. */
  id: number;
  title: string;
  /** Handler id to activate, absent for actions with no callback. */
  handler?: string;
  /** The section it was declared in, if any. */
  section?: string;
  /** "destructive" renders differently. */
  style?: string;
  shortcut?: Shortcut;
  /** The component tag, e.g. "Action.CopyToClipboard". */
  tag: string;
  /**
   * The action's own props.
   *
   * Built-in actions do their work from these rather than from a callback:
   * Action.CopyToClipboard carries `content`, Action.OpenInBrowser carries
   * `url`. Without them a built-in has nothing to act on.
   */
  props: Record<string, unknown>;
}

/**
 * Actions Raycast performs itself, so the extension supplies no handler.
 *
 * Treating a missing handler as "broken" is wrong: for these it is correct,
 * and the launcher is the one expected to do the work.
 */
export const BUILTIN_ACTIONS = new Set([
  "Action.CopyToClipboard",
  "Action.Paste",
  "Action.OpenInBrowser",
  "Action.Open",
]);

/**
 * Whether an action can be run at all, by callback or by us.
 *
 * `no action` beside a row means an extension declared something with nothing
 * behind it, which is worth saying because clicking it would do nothing.
 *
 * **Sill's own actions are never that.** Everything the launcher puts in this
 * panel is dispatched by tag, and until this looked for that, every one of
 * them was labelled as doing nothing: eleven rows on a file, all of them
 * working, all of them saying otherwise.
 */
export function isRunnable(action: ActionEntry): boolean {
  return (
    action.handler !== undefined ||
    BUILTIN_ACTIONS.has(action.tag) ||
    action.tag.startsWith("Sill.")
  );
}

const CONTAINERS = new Set(["ActionPanel", "ActionPanel.Section", "ActionPanel.Submenu"]);

function readShortcut(value: unknown): Shortcut | undefined {
  if (!value || typeof value !== "object") return undefined;
  const record = value as Record<string, unknown>;
  const key = typeof record.key === "string" ? record.key : undefined;
  if (!key) return undefined;

  const modifiers = Array.isArray(record.modifiers)
    ? record.modifiers.filter((m): m is string => typeof m === "string")
    : [];

  return { modifiers, key };
}

/**
 * Every action inside a node's `actions` slot, flattened but remembering which
 * section each came from.
 *
 * Order is declaration order, which Raycast treats as significant: the first
 * action is the primary one that Enter runs.
 */
export function collectActions(tree: ViewTree, node: ElementNode): ActionEntry[] {
  const panel = tree.slot(node, "actions");
  if (!panel) return [];

  const out: ActionEntry[] = [];

  const walk = (parent: ElementNode, section: string | undefined) => {
    for (const child of tree.elementChildren(parent)) {
      if (CONTAINERS.has(child.tag)) {
        const title = child.props.title;
        walk(child, typeof title === "string" ? title : section);
        continue;
      }

      if (!child.tag.startsWith("Action")) continue;

      const onAction = child.props.onAction ?? child.props.onSubmit;
      const title = child.props.title;

      out.push({
        id: child.id,
        // Some actions carry no title and are named by their kind, e.g.
        // Action.CopyToClipboard, which Raycast labels automatically.
        title:
          typeof title === "string" && title
            ? title
            : child.tag.replace(/^Action\.?/, "").replace(/([a-z])([A-Z])/g, "$1 $2") || "Action",
        handler: isHandlerRef(onAction) ? onAction.$handler : undefined,
        section,
        style: typeof child.props.style === "string" ? child.props.style : undefined,
        shortcut: readShortcut(child.props.shortcut),
        tag: child.tag,
        props: child.props,
      });
    }
  };

  walk(panel, undefined);
  return out;
}

/** A button on an extension's toast, as the worker sends it. */
export interface ToastButton {
  title: string;
  /** The handler id to activate. A button without one is never sent. */
  handler: string;
  shortcut?: unknown;
}

/**
 * A toast's buttons, as ordinary actions.
 *
 * `showToast({ primaryAction: { title: "Retry", onAction } })` is a button on a
 * message and nothing more than that. What it runs is a callback in the worker,
 * registered in the same registry every other callback uses, so once it is an
 * `ActionEntry` the window runs it with the code that already runs an
 * extension's actions. Nothing about a toast reaches the running of one.
 *
 * That is deliberate rather than tidy. A second way to run an extension's code
 * would be a second place to get the session check, the error message and the
 * dead-button rule right, and this project has already paid for one of those:
 * `Action.CopyToClipboard` reached the clipboard through a door that asked no
 * permission for months because it was the second path.
 *
 * ## The ids
 *
 * Negative, because `ActionEntry.id` keys a list and a real one is a node id
 * from the tree, which counts up from one. A toast button is in no tree, so it
 * has no node to take an id from, and borrowing a positive number is how two
 * things end up with one key.
 */
export function toastActions(buttons: ToastButton[]): ActionEntry[] {
  return buttons.map((button, index) => ({
    id: -1 - index,
    title: button.title,
    handler: button.handler,
    shortcut: readShortcut(button.shortcut),
    // Not `Action`, because these are not in an action panel and must not be
    // mistaken for a row in one. `isRunnable` answers yes for them on the
    // handler, which every one of them has.
    tag: "Toast.Action",
    props: {},
  }));
}

/** Groups actions for display while preserving declaration order. */
export function groupActions(actions: ActionEntry[]): { section?: string; items: ActionEntry[] }[] {
  const groups: { section?: string; items: ActionEntry[] }[] = [];

  for (const action of actions) {
    const last = groups[groups.length - 1];
    if (last && last.section === action.section) last.items.push(action);
    else groups.push({ section: action.section, items: [action] });
  }

  return groups;
}

/** Renders a shortcut as the keys to draw, in the order they are pressed. */
export function shortcutKeys(shortcut: Shortcut): string[] {
  const names: Record<string, string> = {
    // Raycast writes shortcuts for macOS; cmd is Ctrl on Windows.
    cmd: "Ctrl",
    ctrl: "Ctrl",
    opt: "Alt",
    alt: "Alt",
    shift: "Shift",
    // Raycast's own name for the fn-style modifier.
    cmdOrCtrl: "Ctrl",
  };

  const keys = shortcut.modifiers.map((m) => names[m] ?? m);

  const special: Record<string, string> = {
    arrowUp: "↑",
    arrowDown: "↓",
    arrowLeft: "←",
    arrowRight: "→",
    enter: "↵",
    return: "↵",
    delete: "Del",
    backspace: "⌫",
    escape: "Esc",
    space: "Space",
    tab: "Tab",
  };

  keys.push(special[shortcut.key] ?? shortcut.key.toUpperCase());
  return keys;
}

/**
 * Whether this keystroke is the shortcut an action advertises.
 *
 * ## Why this exists
 *
 * Shortcuts were drawn and never read. An extension declares them, Sill's own
 * clipboard actions declare them, and the panel renders them down the right
 * hand side, so "Paste as Plain Text  Ctrl Shift Enter" sat on screen as a
 * promise nothing kept. Every one of those chords did nothing.
 *
 * ## Why the modifiers have to match exactly
 *
 * A held modifier nobody asked for is a different chord. Without that,
 * Ctrl+Shift+Enter would also fire an action bound to Ctrl+Enter, and the one
 * somebody meant would depend on which appeared first in the list.
 */
export function matchesShortcut(event: Keystroke, shortcut: Shortcut): boolean {
  const wanted = new Set(shortcut.modifiers.map((m) => m.toLowerCase()));

  // Raycast writes for macOS. `cmd` is Ctrl here, and the panel already draws
  // it that way, so the two have to agree about it.
  const control = wanted.has("cmd") || wanted.has("ctrl") || wanted.has("cmdorctrl");
  const alt = wanted.has("opt") || wanted.has("alt");
  const shift = wanted.has("shift");

  if (control !== (event.ctrlKey || event.metaKey)) return false;
  if (alt !== event.altKey) return false;
  if (shift !== event.shiftKey) return false;

  return sameKey(event.key, shortcut.key);
}

/**
 * Whether a browser key name and a Raycast key name are the same key.
 *
 * The two vocabularies overlap without matching: the DOM says `ArrowUp`,
 * `Enter` and `Escape`, Raycast says `arrowUp`, `return` and `escape`, and a
 * letter is `k` on one side and `K` on the other depending on Shift.
 */
function sameKey(pressed: string, wanted: string): boolean {
  const named: Record<string, string> = {
    return: "enter",
    esc: "escape",
    space: " ",
    // Raycast writes `delete` for the forward delete key, which the DOM also
    // calls Delete, and `backspace` for the other one.
    del: "delete",
  };

  const want = named[wanted.toLowerCase()] ?? wanted.toLowerCase();
  return pressed.toLowerCase() === want;
}

/**
 * The action this keystroke runs, if any.
 *
 * Returns an index into the list it was given, because that is what running an
 * action takes: the panel and the window both count through the same array.
 *
 * The first match wins and the order is the panel's own. Two actions
 * advertising the same chord is a bug in whatever declared them, and running
 * the one drawn first is at least the one a person would guess.
 */
export function actionFor(event: Keystroke, actions: ActionEntry[]): number {
  return actions.findIndex(
    (action) => action.shortcut && matchesShortcut(event, action.shortcut),
  );
}

/** Enough of a keyboard event to match a shortcut against. */
export interface Keystroke {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}
