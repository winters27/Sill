/**
 * Reading the parts of a Raycast component that are drawn rather than run.
 *
 * Icons, accessories, empty views and metadata are all props with several
 * legal shapes and no runtime to check them: `icon` is a string, or an object
 * with a source, or a pair of sources for light and dark, or a file to take a
 * picture of. An extension is allowed to pass any of them and a launcher that
 * understands one shape draws a broken image for the other four.
 *
 * Every one of these is a pure function over a rendered node, for the reason
 * the search rules are: they run on a keystroke, over every row on screen,
 * and they have to be testable without a window.
 */

import { isHandlerRef, type ElementNode, type ViewTree } from "./tree";

/**
 * What a colour name means here.
 *
 * Raycast names nine and Sill's palette has four hues plus its neutrals, so
 * five of the nine have no colour of their own. They are drawn in the text
 * colour rather than in the nearest hue: an extension asking for purple and
 * getting blue has been told something untrue about its own row, while one
 * getting the ordinary colour has simply not been given a colour.
 *
 * Orange is the exception and it is not a compromise. The palette used to
 * carry `--accent-orange` holding the same value as `--accent-yellow`, and it
 * was removed for exactly that reason: they were two names for one colour.
 *
 * A name with no row here is not a failure. Themes ship their own colour names
 * and an extension may pass a raw CSS colour, which is why the fallback is the
 * text colour rather than nothing at all.
 */
const COLOURS: Record<string, string> = {
  "raycast-blue": "var(--info)",
  "raycast-green": "var(--success)",
  "raycast-red": "var(--danger)",
  "raycast-yellow": "var(--warning)",
  "raycast-orange": "var(--warning)",
  "raycast-primary-text": "var(--text-1)",
  "raycast-secondary-text": "var(--text-3)",
};

export function colourOf(value: unknown): string | undefined {
  if (typeof value !== "string") {
    // `{ light, dark }` is Raycast's per-appearance colour. The window follows
    // the system the theme does, so this asks the theme rather than choosing.
    if (value && typeof value === "object") {
      return colourOf((value as { dark?: unknown }).dark);
    }
    return undefined;
  }

  return COLOURS[value];
}

/**
 * An icon, in the one shape a component can draw.
 *
 * Three kinds, because there are three genuinely different things to draw and
 * conflating them is what produced broken image glyphs down the side of a
 * list: a picture, a mark from Sill's own set, and a character the extension
 * supplied itself.
 */
export type ExtIcon =
  | { kind: "image"; src: string; tint?: string }
  | { kind: "mark"; name: string; tint?: string }
  | { kind: "glyph"; text: string; tint?: string };

/** Whether a string is something the window can actually put in an `<img>`. */
function drawable(src: string): boolean {
  return /^(https?:|data:)/.test(src);
}

/**
 * Whether a string is a character to print rather than a name to look up.
 *
 * Emoji are the common case and an extension passes them bare. Length is the
 * test rather than a unicode range: Raycast's own names are words, and
 * anything one or two code points long that is not a word is something to
 * print. Counted with the spread, so a flag or a family emoji made of several
 * code units is one character rather than four.
 */
function printable(value: string): boolean {
  return [...value].length <= 2 && !/^[A-Za-z0-9._-]+$/.test(value);
}

/**
 * Reads whatever an extension put in an `icon` prop.
 *
 * Returns undefined for an icon that cannot be drawn at all, which a row is
 * expected to treat as "no icon" rather than as an empty picture. A relative
 * path into the extension's own assets is one of those today: the window has
 * no idea where an installed extension lives on disk, and inventing a URL for
 * it would draw a broken image on every row instead of none.
 */
export function iconOf(value: unknown, tint?: string): ExtIcon | undefined {
  if (typeof value === "string") {
    if (!value) return undefined;
    if (drawable(value)) return { kind: "image", src: value, tint };
    if (printable(value)) return { kind: "glyph", text: value, tint };
    // Everything else is a name: `Icon.Star` is the string "Star", and a
    // relative asset path is a name Sill has no picture for either.
    return { kind: "mark", name: value, tint };
  }

  if (!value || typeof value !== "object") return undefined;

  const record = value as Record<string, unknown>;

  // `{ source, tintColor, mask }`. The tint travels down with the source so a
  // nested `{ light, dark }` keeps it.
  if ("source" in record) {
    return iconOf(record.source, colourOf(record.tintColor) ?? tint);
  }

  // `{ light, dark }`, one picture per appearance. The dark one, because the
  // launcher's own surface is dark whatever the desktop is doing.
  if ("dark" in record || "light" in record) {
    return iconOf(record.dark ?? record.light, tint);
  }

  // `{ fileIcon }` asks the shell for a file's own icon, which is a question
  // only Rust can answer and one no store extension in the sample set asks.
  return undefined;
}

/**
 * One thing drawn down the right hand side of a row.
 *
 * Raycast's `accessories` is an array of these and a row can carry several.
 * `tag` is the one that looks different: it is a pill, and it is the only
 * part of an accessory that takes a colour of its own.
 */
export interface Accessory {
  text?: string;
  tag?: string;
  /** The pill's colour, when the extension asked for one. */
  tint?: string;
  icon?: ExtIcon;
  /** Raycast shows this on hover; Sill's own tooltips take the same string. */
  tooltip?: string;
}

/** A string an extension is allowed to write as `{ value, color }`. */
function labelOf(value: unknown): { text?: string; tint?: string } {
  if (typeof value === "string") return { text: value };
  if (typeof value === "number") return { text: String(value) };
  // A date accessory. Written out rather than shown as an ISO string, which
  // is what `String(date)` over the wire would give.
  if (typeof value === "object" && value !== null) {
    const record = value as { value?: unknown; color?: unknown };
    if ("value" in record) {
      const inner = labelOf(record.value);
      return { text: inner.text, tint: colourOf(record.color) ?? inner.tint };
    }
  }
  return {};
}

export function accessoriesOf(node: ElementNode): Accessory[] {
  const raw = node.props.accessories;
  if (!Array.isArray(raw)) return [];

  const out: Accessory[] = [];

  for (const entry of raw) {
    if (!entry || typeof entry !== "object") continue;
    const record = entry as Record<string, unknown>;

    const text = labelOf(record.text);
    const tag = labelOf(record.tag);
    const date = labelOf(record.date);
    const icon = iconOf(record.icon);
    const tooltip = typeof record.tooltip === "string" ? record.tooltip : undefined;

    // An accessory with nothing in it is one an extension built from data it
    // did not have. Drawing it would be an empty pill nobody can explain.
    if (!text.text && !tag.text && !date.text && !icon) continue;

    out.push({
      text: text.text ?? date.text,
      tag: tag.text,
      tint: tag.tint ?? text.tint,
      icon,
      tooltip,
    });
  }

  return out;
}

/**
 * The words a `List.EmptyView` or `Grid.EmptyView` supplies.
 *
 * Raycast lets an extension replace the whole empty state, and its own words
 * about its own data are better than anything a launcher can guess. Read as
 * words rather than drawn as a component: the empty state is one recipe for
 * all eleven views and an extension gets to fill it in, not to design a
 * twelfth.
 */
export interface EmptyView {
  headline: string;
  hint: string;
  icon?: ExtIcon;
}

export function emptyViewOf(tree: ViewTree, node: ElementNode): EmptyView | undefined {
  const tag = node.tag === "Grid" ? "Grid.EmptyView" : "List.EmptyView";
  const view = tree.elementChildren(node).find((child) => child.tag === tag);
  if (!view) return undefined;

  const title = view.props.title;
  const description = view.props.description;

  return {
    headline: typeof title === "string" ? title : "",
    hint: typeof description === "string" ? description : "",
    icon: iconOf(view.props.icon),
  };
}

/**
 * One line of a metadata panel.
 *
 * Four shapes rather than a `kind` per component tag, because that is what
 * they look like on screen: a labelled value, a labelled link, a labelled row
 * of pills, and a rule between groups.
 */
export type MetaRow =
  | { kind: "label"; title: string; text: string; icon?: ExtIcon; tint?: string }
  | { kind: "link"; title: string; text: string; url: string }
  | { kind: "tags"; title: string; tags: { text: string; tint?: string; icon?: ExtIcon }[] }
  | { kind: "separator" };

/**
 * Reads a Detail.Metadata or List.Item.Detail.Metadata into rows.
 *
 * The two components carry different tags for the same thing, so the suffix
 * is what is matched rather than the whole name. Written that way because the
 * alternative is two tables that have to agree, which is the shape this
 * project has lost sessions to: a metadata row that worked beside a list and
 * not on a detail page, or the other way round, depending on which table was
 * updated.
 */
export function metadataOf(tree: ViewTree, panel: ElementNode): MetaRow[] {
  const out: MetaRow[] = [];

  for (const child of tree.elementChildren(panel)) {
    const kind = child.tag.split(".").pop() ?? "";
    const title = str(child, "title");

    if (kind === "Separator") {
      out.push({ kind: "separator" });
      continue;
    }

    if (kind === "Link") {
      out.push({
        kind: "link",
        title,
        text: str(child, "text") || str(child, "target"),
        url: str(child, "target"),
      });
      continue;
    }

    if (kind === "TagList") {
      const tags = tree.elementChildren(child).map((item) => ({
        text: str(item, "text") || tree.text(item),
        tint: colourOf(item.props.color),
        icon: iconOf(item.props.icon),
      }));
      out.push({ kind: "tags", title, tags });
      continue;
    }

    if (kind === "Label") {
      out.push({
        kind: "label",
        title,
        text: str(child, "text"),
        icon: iconOf(child.props.icon),
      });
    }
  }

  return out;
}

function str(node: ElementNode, key: string): string {
  const value = node.props[key];
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  return "";
}

/**
 * The picker an extension puts beside the search field.
 *
 * `searchBarAccessory` is how a list says "these rows are one of several sets,
 * and here is how to choose". Two of the first two real store extensions
 * looked at use it, and neither of them worked: Hacker News offered fifteen
 * feeds and Sill drew none of them, so the command was permanently on
 * whichever one the extension happened to default to.
 */
export interface Dropdown {
  /** The node, so a caller can tell one picker from the next one. */
  id: number;
  tooltip: string;
  /** The handler to activate with the chosen value. */
  onChange?: string;
  /** What the extension says is chosen, when it says. */
  value?: string;
  options: { value: string; title: string; section?: string; icon?: ExtIcon }[];
}

export function dropdownOf(tree: ViewTree, node: ElementNode): Dropdown | undefined {
  const picker = tree.slot(node, "searchBarAccessory");
  if (!picker) return undefined;

  const options: Dropdown["options"] = [];

  const walk = (parent: ElementNode, section: string | undefined) => {
    for (const child of tree.elementChildren(parent)) {
      if (child.tag.endsWith(".Section")) {
        // A section groups options without changing what a value means, which
        // is the same rule the form's dropdown follows.
        walk(child, str(child, "title") || section);
        continue;
      }
      if (!child.tag.endsWith(".Item")) continue;
      options.push({
        value: str(child, "value"),
        title: str(child, "title") || str(child, "value"),
        section,
        icon: iconOf(child.props.icon),
      });
    }
  };

  walk(picker, undefined);

  const onChange = picker.props.onChange;
  const value = picker.props.value ?? picker.props.defaultValue;

  return {
    id: picker.id,
    tooltip: str(picker, "tooltip") || str(picker, "placeholder"),
    onChange: isHandlerRef(onChange) ? onChange.$handler : undefined,
    value: typeof value === "string" ? value : undefined,
    options,
  };
}

/** The `List.Item.Detail` a row hands over, when the list shows one. */
export function detailOf(tree: ViewTree, item: ElementNode): ElementNode | undefined {
  const slot = tree.slot(item, "detail");
  return slot?.tag === "List.Item.Detail" ? slot : undefined;
}

/**
 * Whether the list is showing a detail pane beside its rows.
 *
 * Raycast's `isShowingDetail` on the List decides the layout, and a row's own
 * `detail` prop decides what goes in it. Both are needed: a list that declares
 * neither is a plain list, and one that sets the flag but has no detail on the
 * selected row still holds the space, because the pane appearing and vanishing
 * as the highlight moves is worse than an empty one.
 */
export function showsDetail(node: ElementNode): boolean {
  return node.props.isShowingDetail === true;
}
