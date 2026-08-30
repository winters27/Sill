<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import ClipKindIcon from "./ClipKindIcon.svelte";
  import {
    clipboardDelete,
    clipboardEntry,
    clipboardPaste,
    clipboardPin,
    clipboardSearch,
    dayLabel,
    formatBytes,
    KIND_FILTERS,
    kindName,
    preview,
    timeLabel,
    type ClipDetail,
    type ClipEntry,
    type ClipKind,
    clipboardLastSkipped,
    clipboardKeepCurrent,
    clipboardCollections,
    clipboardCollectionEntries,
    clipboardCreateCollection,
    clipboardAddToCollection,
    clipboardRemoveFromCollection,
    clipboardDeleteCollection,
    type Collection,
    type Skipped,
  } from "$lib/clipboard";

  interface Props {
    /** The launcher's query field drives the filter. */
    query: string;
    selected: number;
    onselect: (index: number) => void;
    oncount: (count: number) => void;
    /**
     * What is picked for merging, whenever it changes.
     *
     * The launcher draws the action panel, so it has to know whether merging
     * applies. Picks can be made here with the mouse as well as from a key up
     * there, so telling it is this component's job rather than something it
     * can poll for.
     */
    onpick: (ids: number[]) => void;
    /** Whether the highlighted entry kept a formatted version. */
    onrich: (rich: boolean) => void;
    /** Which collection is being looked at, if any. */
    oncollection: (open: Collection | null) => void;
  }

  let {
    query,
    selected,
    onselect,
    oncount,
    onpick,
    onrich,
    oncollection,
  }: Props = $props();

  let entries = $state<ClipEntry[]>([]);
  let kind = $state<ClipKind | "all">("all");
  let detail = $state<ClipDetail | null>(null);
  let filterOpen = $state(false);
  let listEl = $state<HTMLDivElement | null>(null);

  const current = $derived(entries[selected] ?? null);

  /**
   * The list with a day heading before each new day.
   *
   * Headings are not selectable, so they live alongside the rows rather than
   * inside the selection index: `selected` counts entries, and this only
   * decides where a heading is drawn.
   */
  const rows = $derived.by(() => {
    const out: { entry: ClipEntry; index: number; heading: string | null }[] = [];
    let previous: string | null = null;

    entries.forEach((entry, index) => {
      // Pinned entries are lifted out of the timeline, so dating them would
      // put "Yesterday" above something deliberately kept at the top.
      const label = entry.pinned ? "Pinned" : dayLabel(entry.lastSeen);
      out.push({ entry, index, heading: label === previous ? null : label });
      previous = label;
    });

    return out;
  });

  /**
   * The collection being looked at, or null for the whole history.
   *
   * A separate axis from the kind filter rather than another value in it. A
   * collection is a set somebody arranged and the kinds are a property of the
   * content, so "images in Release notes" is a sensible thing to ask for and
   * folding the two into one dropdown would make it unaskable.
   */
  let inside = $state<Collection | null>(null);
  let collections = $state<Collection[]>([]);

  async function loadCollections() {
    collections = await clipboardCollections();

    // The one being looked at may have been renamed or removed elsewhere.
    if (inside) inside = collections.find((c) => c.id === inside?.id) ?? null;
  }

  async function refresh() {
    if (inside) {
      // Arranged order, and filtered here rather than in SQL: a collection is
      // a set somebody curated by hand, so it is small, and a second query
      // shape for it would be two orderings to keep in step.
      const all = await clipboardCollectionEntries(inside.id);
      const needle = query.trim().toLowerCase();
      entries = all.filter(
        (entry) =>
          (kind === "all" || entry.kind === kind) &&
          (!needle || entry.text.toLowerCase().includes(needle)),
      );
    } else {
      entries = await clipboardSearch(query, kind);
    }

    oncount(entries.length);
  }

  // The list re-queries on every keystroke and whenever the filter changes.
  // SQLite with an index answers this in well under a frame.
  $effect(() => {
    query;
    kind;
    inside;
    void refresh();
  });

  // The detail is a second round trip on purpose: it carries the image, and
  // a listing of four hundred rows must not.
  $effect(() => {
    const id = current?.id;
    if (id === undefined) {
      detail = null;
      return;
    }

    let stale = false;
    void clipboardEntry(id).then((found) => {
      if (!stale) detail = found;
    });
    return () => {
      stale = true;
    };
  });

  $effect(() => {
    selected;
    listEl
      ?.querySelector<HTMLElement>(".row.selected")
      ?.scrollIntoView({ block: "nearest" });
  });

  /**
   * Entries picked for merging, in the order they were picked.
   *
   * An array rather than a set, and the order is the whole reason. Merging is
   * composing something, and a list sorted newest-first would silently
   * assemble it backwards.
   *
   * Ids rather than rows, so a pick survives the list being filtered or a new
   * copy arriving underneath it.
   */
  let picked = $state<number[]>([]);
  let collectionsOpen = $state(false);

  const pickedCount = $derived(picked.length);

  export function togglePick() {
    if (!current) return;
    const id = current.id;
    picked = picked.includes(id) ? picked.filter((p) => p !== id) : [...picked, id];
    onpick(picked);
  }

  export function clearPicks(): boolean {
    if (picked.length === 0) return false;
    picked = [];
    onpick(picked);
    return true;
  }

  export function picks(): number[] {
    return picked;
  }

  export async function paste(alsoPaste = true, plainText = false) {
    if (!current) return;
    await clipboardPaste(current.id, alsoPaste, plainText);
  }

  // The launcher draws the action panel, so it has to be told rather than
  // asked. Reported on every change of selection, which is when it can differ.
  $effect(() => {
    onrich(current?.rich ?? false);
  });

  $effect(() => {
    oncollection(inside);
  });

  export async function togglePin() {
    if (!current) return;
    await clipboardPin(current.id, !current.pinned);
    await refresh();
  }

  export async function remove() {
    if (!current) return;
    const at = selected;
    await clipboardDelete(current.id);
    await refresh();
    // Stay where you were rather than jumping to the top, so deleting a run
    // of entries is one key held down.
    onselect(Math.min(at, Math.max(0, entries.length - 1)));
  }

  /** The row the actions apply to, for the panel the parent draws. */
  export function selection(): { id: number; text: string; kind: string } | null {
    return current && { id: current.id, text: current.text, kind: current.kind };
  }

  /** Puts the picked entries into a collection, making it if it is new. */
  export async function addPickedTo(name: string): Promise<number> {
    if (picked.length === 0) return 0;

    const id = await clipboardCreateCollection(name);
    const added = await clipboardAddToCollection(id, picked);

    picked = [];
    onpick(picked);
    await loadCollections();
    return added;
  }

  /** Takes the highlighted entry out of the collection being looked at. */
  export async function removeFromCollection() {
    if (!inside || !current) return;
    await clipboardRemoveFromCollection(inside.id, current.id);
    await loadCollections();
    await refresh();
  }

  export async function forgetCollection() {
    if (!inside) return;
    await clipboardDeleteCollection(inside.id);
    inside = null;
    await loadCollections();
  }

  /** Which collection is open, for the launcher's action panel. */
  export function openCollection(): Collection | null {
    return inside;
  }

  export function haveCollections(): boolean {
    return collections.length > 0;
  }

  export function cycleFilter(by: number) {
    const at = KIND_FILTERS.findIndex((f) => f.id === kind);
    kind = KIND_FILTERS[(at + by + KIND_FILTERS.length) % KIND_FILTERS.length].id;
    onselect(0);
  }

  interface Fact {
    name: string;
    value: string;
    /** A data URI, drawn before the value. Only the source row has one. */
    icon?: string | null;
  }

  /** The metadata table, which differs by what the entry is. */
  const facts = $derived.by((): Fact[] => {
    const entry = detail;
    if (!entry) return [];

    const out: Fact[] = [];
    if (entry.app) out.push({ name: "Source", value: entry.app, icon: entry.appIcon });
    out.push({ name: "Type", value: kindName(entry.kind) });

    if (entry.kind === "link") out.push({ name: "URL", value: entry.text });
    if (entry.kind === "color") out.push({ name: "Value", value: entry.text });
    if (entry.kind === "file") out.push({ name: "Path", value: entry.text });
    if (entry.kind !== "image") {
      out.push({ name: "Characters", value: entry.text.length.toLocaleString() });
      out.push({
        name: "Words",
        value: String(entry.text.trim().split(/\s+/).filter(Boolean).length),
      });
    }

    out.push({ name: "Size", value: formatBytes(entry.bytes) });
    out.push({ name: "Copied", value: timeLabel(entry.lastSeen) });
    if (entry.uses > 1) out.push({ name: "Times used", value: String(entry.uses) });
    return out;
  });

  /**
   * The last thing declined for looking like a credential.
   *
   * Shown here rather than as a notification, because here is where a missing
   * entry is noticed. It costs nothing when nothing was skipped, and there is
   * no notification to dismiss when the guess was right.
   */
  let skipped = $state<Skipped | null>(null);

  async function keepSkipped() {
    try {
      await clipboardKeepCurrent();
      skipped = null;
      await refresh();
    } catch (err) {
      // The clipboard has usually moved on by the time this fails, which is
      // the honest thing to say rather than a generic failure.
      skipped = null;
      console.error(err);
    }
  }

  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    let refused: UnlistenFn | undefined;

    (async () => {
      await refresh();
      skipped = await clipboardLastSkipped();
      await loadCollections();

      // Something copied while this is open has to appear, or the history
      // looks broken at the exact moment it is being watched.
      unlisten = await listen("clipboard:changed", () => {
        skipped = null;
        void refresh();
      });

      refused = await listen<Skipped>("clipboard:skipped", ({ payload }) => {
        skipped = payload;
      });
    })();

    return () => {
      unlisten?.();
      refused?.();
    };
  });
</script>

<div class="clipboard">
  {#if skipped}
    <!-- Above the list, because it is about something that is not in it. -->
    <div class="skipped">
      <span class="said">
        A {skipped.what} was not saved, {skipped.length.toLocaleString()} characters.
      </span>
      <button class="keep" onclick={keepSkipped}>Save it anyway</button>
    </div>
  {/if}

  <div class="bar">
    <span class="count">
      {#if pickedCount}
        {pickedCount} picked
      {:else}
        {entries.length.toLocaleString()}
        {entries.length === 1 ? "entry" : "entries"}
      {/if}
    </span>
    {#if inside}
      <!-- Which set is being looked at, and the way back out of it. The whole
           history is not a collection, so it is not another entry in a
           dropdown; it is what you get when you leave this one. -->
      <button class="crumb" onclick={() => (inside = null)}>
        {inside.name}
        <svg width="9" height="9" viewBox="0 0 12 12" aria-hidden="true">
          <path
            d="M3 3l6 6M9 3l-6 6"
            stroke="currentColor"
            stroke-width="1.5"
            fill="none"
            stroke-linecap="round"
          />
        </svg>
      </button>
    {/if}

    <span class="spacer"></span>

    {#if collections.length && !inside}
      <div class="filter">
        <button
          class="trigger"
          onclick={() => (collectionsOpen = !collectionsOpen)}
          aria-haspopup="menu"
        >
          Collections
          <svg width="10" height="10" viewBox="0 0 12 12" aria-hidden="true">
            <path
              d="M2.5 4.5 6 8l3.5-3.5"
              stroke="currentColor"
              stroke-width="1.4"
              fill="none"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </button>

        {#if collectionsOpen}
          <div
            class="scrim"
            role="presentation"
            onclick={() => (collectionsOpen = false)}
          ></div>
          <div class="menu" role="menu">
            {#each collections as collection (collection.id)}
              <button
                class="option"
                role="menuitem"
                onclick={() => {
                  inside = collection;
                  collectionsOpen = false;
                  onselect(0);
                }}
              >
                <span class="label">{collection.name}</span>
                <span class="tally">{collection.count}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <div class="filter">
      <button class="trigger" onclick={() => (filterOpen = !filterOpen)} aria-haspopup="menu">
        {KIND_FILTERS.find((f) => f.id === kind)?.label}
        <svg width="10" height="10" viewBox="0 0 12 12" aria-hidden="true">
          <path
            d="M2.5 4.5 6 8l3.5-3.5"
            stroke="currentColor"
            stroke-width="1.4"
            fill="none"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>

      {#if filterOpen}
        <div class="scrim" role="presentation" onclick={() => (filterOpen = false)}></div>
        <div class="menu sill-menu" role="menu">
          {#each KIND_FILTERS as option (option.id)}
            <button
              class="option"
              class:on={option.id === kind}
              role="menuitemradio"
              aria-checked={option.id === kind}
              onclick={() => {
                kind = option.id;
                filterOpen = false;
                onselect(0);
              }}
            >
              {#if option.id === "all"}
                <span class="glyph"></span>
              {:else}
                <span class="glyph"><ClipKindIcon kind={option.id} size={13} /></span>
              {/if}
              {option.label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="panes">
    <div class="list" bind:this={listEl} role="listbox" tabindex="-1" aria-label="Clipboard history">
      {#each rows as row (row.entry.id)}
        {#if row.heading}
          <div class="sill-group">{row.heading}</div>
        {/if}
        <div
          class="row"
          class:selected={row.index === selected}
          role="option"
          aria-selected={row.index === selected}
          tabindex="-1"
          class:picked={picked.includes(row.entry.id)}
          onmousemove={() => onselect(row.index)}
          onclick={(e) => {
            // Ctrl-click picks rather than pastes, which is the one gesture
            // everybody already has for "and this one too".
            if (e.ctrlKey || e.metaKey) {
              onselect(row.index);
              togglePick();
              return;
            }
            void paste();
          }}
          onkeydown={(e) => e.key === "Enter" && void paste()}
        >
          {#if picked.includes(row.entry.id)}
            <!-- The position in the merge, not a tick. Which order they go in
                 is the thing a person needs to see while picking. -->
            <span class="order">{picked.indexOf(row.entry.id) + 1}</span>
          {/if}
          <span class="mark">
            <ClipKindIcon kind={row.entry.kind} swatch={row.entry.text} size={14} />
          </span>
          <span class="line">{preview(row.entry.text)}</span>
          {#if row.entry.pinned}
            <svg class="pin" width="11" height="11" viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M9 3h6l-1 6 4 3v2H6v-2l4-3Z M12 14v7"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linejoin="round"
              />
            </svg>
          {/if}
        </div>
      {/each}

      {#if entries.length === 0}
        <p class="empty">
          {query ? "Nothing matches that." : "Nothing copied yet. Everything you copy lands here."}
        </p>
      {/if}
    </div>

    <div class="detail">
      {#if detail}
        <div class="preview" data-kind={detail.kind}>
          {#if detail.image}
            <img src={detail.image} alt="" />
          {:else if detail.kind === "color"}
            <div class="colour" style:background={detail.text}>
              <span>{detail.text}</span>
            </div>
          {:else}
            <pre>{detail.text}</pre>
          {/if}
        </div>

        <div class="facts">
          <div class="facts-label">Information</div>
          {#each facts as fact (fact.name)}
            <div class="fact">
              <span class="name">{fact.name}</span>
              <span class="value" title={fact.value}>
                {#if fact.icon}
                  <img class="app-icon" src={fact.icon} alt="" width="14" height="14" />
                {/if}
                {fact.value}
              </span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="nothing"></div>
      {/if}
    </div>
  </div>
</div>

<style>
  /* Stated once, quietly, and gone as soon as anything else is copied. It is
     information rather than an alarm: most of the time the guess is right and
     the right response is to read it and carry on. */
  .skipped {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    font-size: 12px;
    color: var(--text-dim);
    background: rgba(var(--accent-rgb), 0.07);
    border-bottom: 1px solid var(--line);
  }

  .said {
    flex: 1;
    min-width: 0;
  }

  .keep {
    flex: none;
    padding: 3px 9px;
    font: inherit;
    font-size: 11px;
    color: var(--text);
    background: transparent;
    border: none;
    border-radius: 5px;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .keep:hover {
    color: var(--accent-bright);
  }

  /* A picked row reads as picked without a checkbox column that would be
     empty on every row the rest of the time. */
  .row.picked {
    background: rgba(var(--accent-rgb), 0.09);
  }

  /* Which collection is open. A button because its whole job is leaving. */
  .crumb {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 7px;
    font: inherit;
    font-size: 11px;
    color: var(--text);
    background: rgba(var(--accent-rgb), 0.12);
    border: none;
    border-radius: 5px;
    cursor: pointer;
  }

  .crumb:hover {
    color: var(--accent-bright);
  }

  .tally {
    margin-left: auto;
    padding-left: 12px;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    color: var(--text-dim);
  }

  .order {
    flex: none;
    display: grid;
    place-items: center;
    width: 15px;
    height: 15px;
    margin-right: 2px;
    font-size: 9px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--core-background, #fff);
    background: var(--accent-bright);
    border-radius: 50%;
  }

  .clipboard {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
    height: 34px;
    padding: 0 10px 0 12px;
    border-bottom: 1px solid var(--hairline);
  }

  .count {
    font-size: var(--text-meta);
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }

  .spacer {
    flex: 1;
  }

  .filter {
    position: relative;
  }

  .trigger {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: rgba(var(--accent-rgb), 0.08);
    color: var(--core-foreground);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
    transition: background-color 0.15s var(--ease);
  }

  .trigger:hover {
    background: rgba(var(--accent-rgb), 0.16);
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 20;
  }

  .menu {
    position: absolute;
    top: 30px;
    right: 0;
    z-index: 21;
    width: 168px;
    padding: 5px;
  }

  .option {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: var(--text-row);
    text-align: left;
    cursor: pointer;
    transition: background-color 0.15s var(--ease), color 0.15s var(--ease);
  }

  .option:hover {
    background: rgba(var(--accent-rgb), 0.1);
    color: var(--core-foreground);
  }

  .option.on {
    color: var(--core-foreground);
  }

  .glyph {
    display: grid;
    place-items: center;
    width: 14px;
    flex: none;
  }

  .panes {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  /* Narrow on purpose. The list is for finding the entry; the pane beside it
     is where the entry is actually read. */
  .list {
    width: 268px;
    flex: none;
    overflow-y: auto;
    padding: 5px;
    border-right: 1px solid var(--hairline);
    scrollbar-width: thin;
    scrollbar-color: rgba(var(--accent-rgb), 0.3) transparent;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 32px;
    padding: 0 9px;
    border-radius: var(--radius-sm);
    cursor: default;
    transition: background-color 0.18s var(--ease);
  }

  .row:hover:not(.selected) {
    background-color: rgba(var(--accent-rgb), 0.07);
  }

  .row.selected {
    background-color: var(--surface);
    box-shadow: var(--bevel-tile);
  }

  .mark {
    display: grid;
    place-items: center;
    width: 16px;
    flex: none;
    color: var(--text-faint);
  }

  .line {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-row);
  }

  .pin {
    flex: none;
    color: var(--core-accent);
  }

  .empty {
    margin: 0;
    padding: 26px 10px;
    font-size: var(--text-row);
    line-height: 1.6;
    color: var(--text-faint);
  }

  .detail {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .preview {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 16px;
    scrollbar-width: thin;
    scrollbar-color: rgba(var(--accent-rgb), 0.3) transparent;
  }

  .preview pre {
    margin: 0;
    /* Selectable: reading a copied thing back and taking half of it is half
       the reason to keep a history. */
    user-select: text;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-family: var(--font);
    font-size: var(--text-row);
    line-height: 1.6;
    color: var(--core-foreground);
  }

  .preview[data-kind="file"] pre,
  .preview[data-kind="link"] pre {
    font-family: var(--font-mono);
    font-size: 12.5px;
  }

  .preview img {
    display: block;
    max-width: 100%;
    max-height: 100%;
    margin: 0 auto;
    border-radius: var(--radius-sm);
    /* A screenshot of a white page needs an edge or it bleeds into the
       window; a dark one needs nothing, and this gives both the same one. */
    box-shadow: 0 0 0 1px var(--hairline);
  }

  .colour {
    display: grid;
    place-items: center;
    height: 100%;
    min-height: 120px;
    border-radius: var(--radius);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.15);
  }

  .colour span {
    padding: 4px 10px;
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.55);
    font-family: var(--font-mono);
    font-size: 12px;
    color: #fff;
  }

  .facts {
    flex: none;
    max-height: 44%;
    overflow-y: auto;
    padding: 12px 16px 14px;
    border-top: 1px solid var(--hairline);
    scrollbar-width: thin;
    scrollbar-color: rgba(var(--accent-rgb), 0.3) transparent;
  }

  .facts-label {
    margin-bottom: 8px;
    font-size: var(--text-group);
    font-weight: 500;
    color: var(--text-faint);
  }

  .fact {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 7px 0;
  }

  /* A hairline BETWEEN rows only, so the table reads as one block rather
     than a boxed grid. The adjacent-sibling selector skips the first. */
  .fact + .fact {
    border-top: 1px solid color-mix(in srgb, var(--hairline) 85%, transparent);
  }

  .fact .name {
    flex: none;
    width: 92px;
    font-size: var(--text-meta);
    color: var(--text-faint);
  }

  .fact .value {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 7px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-meta);
    text-align: right;
    color: var(--core-foreground);
    font-variant-numeric: tabular-nums;
  }

  .app-icon {
    flex: none;
    width: 14px;
    height: 14px;
    border-radius: 3px;
  }

  .nothing {
    flex: 1;
  }
</style>
