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
  } from "$lib/clipboard";

  interface Props {
    /** The launcher's query field drives the filter. */
    query: string;
    selected: number;
    onselect: (index: number) => void;
    oncount: (count: number) => void;
  }

  let { query, selected, onselect, oncount }: Props = $props();

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

  async function refresh() {
    entries = await clipboardSearch(query, kind);
    oncount(entries.length);
  }

  // The list re-queries on every keystroke and whenever the filter changes.
  // SQLite with an index answers this in well under a frame.
  $effect(() => {
    query;
    kind;
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

  export async function paste(alsoPaste = true) {
    if (!current) return;
    await clipboardPaste(current.id, alsoPaste);
  }

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

  onMount(() => {
    let unlisten: UnlistenFn | undefined;

    (async () => {
      await refresh();
      // Something copied while this is open has to appear, or the history
      // looks broken at the exact moment it is being watched.
      unlisten = await listen("clipboard:changed", () => void refresh());
    })();

    return () => unlisten?.();
  });
</script>

<div class="clipboard">
  <div class="bar">
    <span class="count">
      {entries.length.toLocaleString()}
      {entries.length === 1 ? "entry" : "entries"}
    </span>
    <span class="spacer"></span>

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
          onmousemove={() => onselect(row.index)}
          onclick={() => void paste()}
          onkeydown={(e) => e.key === "Enter" && void paste()}
        >
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
