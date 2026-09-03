/**
 * What the search field means while an extension is on screen.
 *
 * Raycast's `<List>` and `<Grid>` declare four props about the field, and all
 * four were read by nobody: `filtering`, `onSearchTextChange`, `throttle` and
 * `isLoading`. Typing in an extension's list did nothing at all. An extension
 * whose whole job is to search a remote service rendered its first, empty
 * answer and then sat there.
 *
 * The rules, in Raycast's own terms:
 *
 * - `filtering` says who narrows the rows. True and Sill does it over what the
 *   extension rendered. False and only the extension can, so it had better be
 *   hearing what was typed.
 * - `onSearchTextChange` is how it hears. Its presence decides the default for
 *   `filtering`: an extension that registered one is saying it intends to
 *   search, so `filtering` defaults to false, and otherwise to true. Getting
 *   that default backwards gives either a list that cannot be searched or two
 *   filters fighting over the same rows.
 * - `throttle` is a promise to the author about how often they are called.
 * - `isLoading` is the difference between "nothing matched" and "not yet".
 *
 * The whole of it is a pure function over the rendered tree plus one small
 * object that owns a timer, so the behaviour is testable without a window.
 * That matters more here than usual: every one of these runs on a keystroke.
 */

import type { ElementNode, ViewTree } from "./tree";
import { isHandlerRef } from "./tree";

/**
 * How often an extension that asked to be throttled is called, in
 * milliseconds.
 *
 * The number has to sit above a typist and below a pause. Somebody typing
 * quickly puts down a character every 100 to 150 ms, so anything at or under
 * that removes no calls at all and `throttle` becomes a prop that lies. The
 * other end is what the reader feels: the last keystroke of a word is answered
 * this long after it lands, and 200 ms is inside the window where a result
 * still reads as a response to what was typed rather than as an afterthought.
 *
 * At 200 ms a six character word costs three or four calls instead of six, and
 * a held key costs five a second instead of thirty. Sill's own file search
 * waits 120 ms before it runs, and this is deliberately longer: that one is a
 * local index, and this one is usually somebody's network.
 */
export const THROTTLE_MS = 200;

/** What a rendered List or Grid says about the field above it. */
export interface SearchProps {
  /** Whether Sill narrows the rows itself. */
  filtering: boolean;
  /** Whether the extension is told about every keystroke, or at a rate. */
  throttle: boolean;
  /** Whether the extension says an answer is still coming. */
  loading: boolean;
  /** The handler id behind `onSearchTextChange`, when there is one. */
  onChange?: string;
}

function flag(node: ElementNode, key: string): boolean {
  return node.props[key] === true;
}

/**
 * Reads the four props off whatever the extension rendered.
 *
 * `filtering` is also allowed to be an object in Raycast, `{ keepSectionOrder }`,
 * which is still a request to filter. So the test is truthiness rather than
 * `=== true`; an extension that passed the object and got no filtering would
 * have asked for the feature and been given its opposite.
 */
export function searchProps(node: ElementNode | undefined): SearchProps {
  if (!node) return { filtering: false, throttle: false, loading: false };

  const onSearchTextChange = node.props.onSearchTextChange;
  const onChange = isHandlerRef(onSearchTextChange) ? onSearchTextChange.$handler : undefined;

  const declared = node.props.filtering;
  const filtering = declared === undefined ? onChange === undefined : Boolean(declared);

  return {
    filtering,
    throttle: flag(node, "throttle"),
    loading: flag(node, "isLoading"),
    onChange,
  };
}

/** A heading between rows, or one selectable row. */
export type Row =
  | { kind: "section"; node: ElementNode }
  | { kind: "item"; node: ElementNode; index: number };

/**
 * A title as an extension is allowed to write it.
 *
 * Raycast accepts `title="Foo"` and `title={{ value: "Foo", tooltip: "..." }}`,
 * and an extension using the second form would otherwise be filtered against
 * an empty string, which means every row it drew disappeared on the first
 * keystroke.
 */
function words(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (Array.isArray(value)) return value.flatMap(words);
  if (value && typeof value === "object") {
    return words((value as { value?: unknown }).value);
  }
  return [];
}

/**
 * Whether a row survives the filter.
 *
 * Title, subtitle and keywords, which is what Raycast matches on, and a
 * case-insensitive substring, which is what every other list in Sill that
 * narrows rows it already holds does: the clipboard, the action panel, the
 * settings index. Fuzzy ranking is Rust's job and these rows never go there;
 * they are an extension's own tree and they arrive already ordered by the
 * extension, which is an order this has no business rewriting.
 */
export function matches(node: ElementNode, needle: string): boolean {
  if (!needle) return true;

  const wanted = needle.toLowerCase();
  const haystack = [
    ...words(node.props.title),
    ...words(node.props.subtitle),
    ...words(node.props.keywords),
  ];

  return haystack.some((one) => one.toLowerCase().includes(wanted));
}

/**
 * The rows of a List or a Grid, flattened, narrowed and numbered.
 *
 * One function for both because selection walks the same flattened sequence in
 * either, and one function for the view and the page because they were two:
 * `ListView` built the rows it drew and `+page.svelte` built the items Enter
 * ran, from the same tree, by the same rules, written twice. Two lists that
 * have to agree with nothing making them agree is how this project has lost a
 * session before, and adding a filter to only one of them is exactly that
 * shape: Enter would have run the row above or below the one under the cursor,
 * and only while something was typed.
 *
 * A section whose every item was filtered out goes with them. A heading over
 * nothing says the extension returned something it did not.
 */
export function rowsOf(tree: ViewTree, node: ElementNode, narrow: string): Row[] {
  const item = node.tag === "Grid" ? "Grid.Item" : "List.Item";
  const section = node.tag === "Grid" ? "Grid.Section" : "List.Section";

  const out: Row[] = [];
  let index = 0;

  for (const child of tree.elementChildren(node)) {
    if (child.tag === section) {
      const kept = tree
        .elementChildren(child)
        .filter((one) => one.tag === item && matches(one, narrow));

      if (kept.length === 0) continue;

      out.push({ kind: "section", node: child });
      for (const one of kept) out.push({ kind: "item", node: one, index: index++ });
    } else if (child.tag === item && matches(child, narrow)) {
      out.push({ kind: "item", node: child, index: index++ });
    }
  }

  return out;
}

/** Just the selectable rows, which is what Enter and the arrow keys count. */
export function itemsOf(rows: Row[]): ElementNode[] {
  const out: ElementNode[] = [];
  for (const row of rows) if (row.kind === "item") out.push(row.node);
  return out;
}

/** What a relay needs from the world, so a test can supply both. */
export interface RelayHooks {
  /** Fires the extension's `onSearchTextChange` with this text. */
  send(text: string): Promise<unknown>;
  /**
   * Says a call failed.
   *
   * Only ever called for the newest one. A slow refusal for `git` landing
   * after `github` was sent would put a message on screen about a query the
   * person has already typed past, which is the same stale-answer rule the
   * root search follows with `searchId` and the store with `generation`.
   */
  failed?(err: unknown): void;
}

/**
 * Carries what was typed to the extension, at the rate it asked for.
 *
 * An instance rather than a module-level pair of variables, because rule 2
 * refuses module state that behaves as a singleton, and because two of these
 * have to be able to exist at once for a test to be able to write one.
 *
 * ## The three fields worth knowing about
 *
 * `sent` is the last text the extension has actually been given. Nothing is
 * sent twice: the effect that drives this re-runs whenever anything else about
 * the rendered node changes, `isLoading` flipping among them, and without this
 * every render an extension made in answer to typing would provoke another
 * call about text it already had.
 *
 * `waiting` is the text a throttled call will carry when its window closes,
 * and it is always the newest text rather than the text that was current when
 * the timer was armed. A timer holding `gi` while the field says `github` is
 * the stale answer arriving by the front door.
 *
 * `generation` is which call is the current one. Nothing about a superseded
 * call may reach the screen.
 */
export class SearchRelay {
  private waiting: string | undefined;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private sent = "";
  private at = 0;
  private generation = 0;

  constructor(private readonly hooks: RelayHooks) {}

  /**
   * The field holds this now.
   *
   * `throttled` comes from the rendered node rather than from construction,
   * because an extension can render a different `throttle` at any time and the
   * prop it declared most recently is the one it means.
   */
  offer(text: string, throttled: boolean): void {
    if (text === this.sent) {
      // Typed back to what the extension already has. Anything queued is now a
      // call that would tell it nothing.
      this.disarm();
      return;
    }

    if (!throttled) {
      this.fire(text);
      return;
    }

    const since = Date.now() - this.at;
    if (since >= THROTTLE_MS && this.timer === undefined) {
      this.fire(text);
      return;
    }

    // Inside the window: the newest text replaces whatever was queued, and a
    // timer that is already armed keeps its own deadline rather than being
    // pushed back by every further keystroke. Pushing it back is a debounce,
    // and a debounce never answers at all while somebody is still typing.
    this.waiting = text;
    if (this.timer === undefined) {
      this.timer = setTimeout(() => this.release(), Math.max(0, THROTTLE_MS - since));
    }
  }

  /**
   * The extension set the text itself, so it does not need telling about it.
   *
   * `UI/setSearchText` writes the field, and relaying that straight back would
   * be Sill reporting the extension's own words to it as news.
   */
  adopt(text: string): void {
    this.disarm();
    this.sent = text;
  }

  /** The command is gone. Nothing queued may fire and nothing may report. */
  cancel(): void {
    this.disarm();
    this.sent = "";
    this.at = 0;
    this.generation += 1;
  }

  private disarm(): void {
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
    this.waiting = undefined;
  }

  private release(): void {
    this.timer = undefined;
    const text = this.waiting;
    this.waiting = undefined;
    if (text !== undefined && text !== this.sent) this.fire(text);
  }

  private fire(text: string): void {
    this.disarm();
    this.sent = text;
    this.at = Date.now();

    const mine = ++this.generation;
    void this.hooks.send(text).catch((err: unknown) => {
      if (mine === this.generation) this.hooks.failed?.(err);
    });
  }
}
