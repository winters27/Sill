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

/** Whether an action can be run at all, by callback or by us. */
export function isRunnable(action: ActionEntry): boolean {
  return action.handler !== undefined || BUILTIN_ACTIONS.has(action.tag);
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
