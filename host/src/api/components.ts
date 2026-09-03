/**
 * The Raycast component surface.
 *
 * Every component is a host element whose tag is its dotted name, so
 * <List.Item /> reaches the reconciler as `$t: "List.Item"`. The UI renders by
 * dispatching on that tag, which keeps the component set declarative on both
 * sides: adding one here and one in the Svelte renderer is the whole job.
 *
 * Prop shapes are not re-declared. `@raycast/api` ships the MIT type
 * declarations and those are the spec, so the real types are borrowed from it
 * and the runtime only has to produce the right element.
 */

import { createElement, isValidElement, type FunctionComponent, type ReactNode } from "react";

import { getBridge } from "./bridge";

/** Wraps an element-valued prop so it becomes part of the rendered tree. */
export const SLOT_TAG = "$slot";

function holdsElement(value: unknown): boolean {
  if (isValidElement(value)) return true;
  return Array.isArray(value) && value.some((entry) => isValidElement(entry));
}

/**
 * Builds a host component for one tag.
 *
 * Raycast passes React elements through props, not just children:
 * `actions={<ActionPanel/>}`, `detail={<List.Item.Detail/>}`,
 * `searchBarAccessory={<List.Dropdown/>}`. Left in the prop bag those would be
 * fatal twice over. React elements are circular, so serialising them throws,
 * and the reconciler never descends into props, so any handler inside would
 * never be registered and any update never diffed.
 *
 * So element-valued props are lifted into children wrapped in a `$slot` node
 * carrying the prop name. React then owns the entire tree, which makes
 * diffing, handler registration and cleanup work the same everywhere.
 */
function host<P extends object>(tag: string): FunctionComponent<P> {
  const component: FunctionComponent<P> = (props) => {
    const source = props as Record<string, unknown>;
    const plain: Record<string, unknown> = {};
    const slots: ReactNode[] = [];

    for (const [key, value] of Object.entries(source)) {
      if (key === "children") continue;
      if (holdsElement(value)) {
        slots.push(createElement(SLOT_TAG, { key: `$slot:${key}`, name: key }, value as ReactNode));
      } else {
        plain[key] = value;
      }
    }

    return createElement(tag, plain, ...slots, source.children as ReactNode);
  };

  component.displayName = tag;
  return component;
}

/**
 * Attaches sub-components, matching Raycast's `FunctionComponent & Members`
 * shape so `List.Item` and `<List>` are the same object.
 */
function withMembers<C extends FunctionComponent<never>, M extends object>(
  component: C,
  members: M,
): C & M {
  return Object.assign(component, members);
}

type AnyProps = Record<string, unknown> & { children?: ReactNode };

// ---------------------------------------------------------------- Action

const ActionBase = host<AnyProps>("Action");

const PushHost = host<AnyProps>("Action.Push");

/**
 * The one action the host performs itself, and the reason it is not a `host`.
 *
 * `<Action.Push target={<Detail/>}/>` hands over a React element, and the rule
 * above would lift it into the tree as a `$slot` child like every other
 * element prop. That is wrong twice.
 *
 * **It renders a screen nobody asked for.** A list of two hundred rows each
 * offering to push a detail view would mount two hundred detail views on the
 * first frame, run every `useEffect` in them, and start every fetch they make,
 * for the one the reader might eventually press. Raycast mounts a pushed view
 * when it is pushed, and so does this.
 *
 * **It leaves the stack nowhere.** A pushed view is the top of a navigation
 * stack, and the stack lives in the worker, next to the React root that has to
 * render it. A subtree hanging off an action in the previous screen is not a
 * stack, it is two screens drawn at once with the UI asked to guess.
 *
 * So the target stays an unrendered element and Push becomes an ordinary
 * callback: `onAction` is synthesised here, gets a handler id from the same
 * registry every other callback uses, and rides the same activation the UI
 * already sends. Nothing new crosses the wire and nothing in the UI has to
 * know that this action is different from a copy or an open.
 */
const ActionPush: FunctionComponent<AnyProps> = (props) => {
  const { target, onPop, onAction, ...rest } = props as AnyProps & {
    target?: ReactNode;
    onPop?: () => void;
    onAction?: () => void;
  };

  return createElement(PushHost, {
    ...rest,
    onAction: () => {
      getBridge().navigation.push(target, onPop);
      // Raycast runs the author's own `onAction` as well as pushing, and
      // extensions use it to record that the screen was opened.
      onAction?.();
    },
  });
};

ActionPush.displayName = "Action.Push";

export const Action = withMembers(ActionBase, {
  CopyToClipboard: host<AnyProps>("Action.CopyToClipboard"),
  OpenInBrowser: host<AnyProps>("Action.OpenInBrowser"),
  Open: host<AnyProps>("Action.Open"),
  Paste: host<AnyProps>("Action.Paste"),
  Push: ActionPush,
  SubmitForm: host<AnyProps>("Action.SubmitForm"),
  Trash: host<AnyProps>("Action.Trash"),
  ShowInFinder: host<AnyProps>("Action.ShowInFinder"),
  OpenWith: host<AnyProps>("Action.OpenWith"),
  CreateSnippet: host<AnyProps>("Action.CreateSnippet"),
  CreateQuicklink: host<AnyProps>("Action.CreateQuicklink"),
  ToggleQuickLook: host<AnyProps>("Action.ToggleQuickLook"),
  PickDate: host<AnyProps>("Action.PickDate"),
  Style: { Regular: "regular", Destructive: "destructive" } as const,
});

// ------------------------------------------------------------ ActionPanel

const ActionPanelBase = host<AnyProps>("ActionPanel");

export const ActionPanel = withMembers(ActionPanelBase, {
  Section: host<AnyProps>("ActionPanel.Section"),
  Submenu: host<AnyProps>("ActionPanel.Submenu"),
});

// ------------------------------------------------------------------ List

const ListBase = host<AnyProps>("List");

const ListItem = withMembers(host<AnyProps>("List.Item"), {
  Detail: withMembers(host<AnyProps>("List.Item.Detail"), {
    Metadata: withMembers(host<AnyProps>("List.Item.Detail.Metadata"), {
      Label: host<AnyProps>("List.Item.Detail.Metadata.Label"),
      Link: host<AnyProps>("List.Item.Detail.Metadata.Link"),
      TagList: withMembers(host<AnyProps>("List.Item.Detail.Metadata.TagList"), {
        Item: host<AnyProps>("List.Item.Detail.Metadata.TagList.Item"),
      }),
      Separator: host<AnyProps>("List.Item.Detail.Metadata.Separator"),
    }),
  }),
});

const ListDropdown = withMembers(host<AnyProps>("List.Dropdown"), {
  Item: host<AnyProps>("List.Dropdown.Item"),
  Section: host<AnyProps>("List.Dropdown.Section"),
});

export const List = withMembers(ListBase, {
  Item: ListItem,
  Section: host<AnyProps>("List.Section"),
  EmptyView: host<AnyProps>("List.EmptyView"),
  Dropdown: ListDropdown,
});

// ------------------------------------------------------------------ Grid

const GridItem = host<AnyProps>("Grid.Item");

const GridDropdown = withMembers(host<AnyProps>("Grid.Dropdown"), {
  Item: host<AnyProps>("Grid.Dropdown.Item"),
  Section: host<AnyProps>("Grid.Dropdown.Section"),
});

export const Grid = withMembers(host<AnyProps>("Grid"), {
  Item: GridItem,
  Section: host<AnyProps>("Grid.Section"),
  EmptyView: host<AnyProps>("Grid.EmptyView"),
  Dropdown: GridDropdown,
  Inset: { Small: "small", Medium: "medium", Large: "large" } as const,
  Fit: { Contain: "contain", Fill: "fill" } as const,
});

// ---------------------------------------------------------------- Detail

export const Detail = withMembers(host<AnyProps>("Detail"), {
  Metadata: withMembers(host<AnyProps>("Detail.Metadata"), {
    Label: host<AnyProps>("Detail.Metadata.Label"),
    Link: host<AnyProps>("Detail.Metadata.Link"),
    TagList: withMembers(host<AnyProps>("Detail.Metadata.TagList"), {
      Item: host<AnyProps>("Detail.Metadata.TagList.Item"),
    }),
    Separator: host<AnyProps>("Detail.Metadata.Separator"),
  }),
});

// ------------------------------------------------------------------ Form

export const Form = withMembers(host<AnyProps>("Form"), {
  TextField: host<AnyProps>("Form.TextField"),
  TextArea: host<AnyProps>("Form.TextArea"),
  Checkbox: host<AnyProps>("Form.Checkbox"),
  DatePicker: withMembers(host<AnyProps>("Form.DatePicker"), {
    Type: { DateTime: "date_time", Date: "date" } as const,
  }),
  Dropdown: withMembers(host<AnyProps>("Form.Dropdown"), {
    Item: host<AnyProps>("Form.Dropdown.Item"),
    Section: host<AnyProps>("Form.Dropdown.Section"),
  }),
  TagPicker: withMembers(host<AnyProps>("Form.TagPicker"), {
    Item: host<AnyProps>("Form.TagPicker.Item"),
  }),
  Separator: host<AnyProps>("Form.Separator"),
  Description: host<AnyProps>("Form.Description"),
  PasswordField: host<AnyProps>("Form.PasswordField"),
  FilePicker: host<AnyProps>("Form.FilePicker"),
  LinkAccessory: host<AnyProps>("Form.LinkAccessory"),
});

/** Every tag the UI is expected to know how to draw. */
export const KNOWN_TAGS: readonly string[] = [
  "List",
  "List.Item",
  "List.Section",
  "List.EmptyView",
  "List.Dropdown",
  "List.Dropdown.Item",
  "List.Dropdown.Section",
  "List.Item.Detail",
  "Grid",
  "Grid.Item",
  "Grid.Section",
  "Grid.EmptyView",
  "Detail",
  "Detail.Metadata",
  "Form",
  "ActionPanel",
  "ActionPanel.Section",
  "ActionPanel.Submenu",
  "Action",
];
