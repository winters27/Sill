<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import Section from "./Section.svelte";
  import Button from "./Button.svelte";
  import {
    clearDictationHistory,
    dictationHistory,
    forgetTranscription,
    type HistoryEntry,
  } from "$lib/dictation";

  let entries = $state<HistoryEntry[]>([]);
  let filter = $state("");
  let status = $state("");
  let confirmingClear = $state(false);

  const shown = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    if (!needle) return entries;
    return entries.filter(
      (entry) =>
        entry.text.toLowerCase().includes(needle) ||
        (entry.app ?? "").toLowerCase().includes(needle),
    );
  });

  /** "3 minutes ago", "yesterday", "14 Mar". */
  function when(at: number): string {
    const seconds = Math.max(0, Math.floor(Date.now() / 1000) - at);
    if (seconds < 60) return "just now";
    if (seconds < 3600) {
      const minutes = Math.floor(seconds / 60);
      return `${minutes} ${minutes === 1 ? "minute" : "minutes"} ago`;
    }
    if (seconds < 86_400) {
      const hours = Math.floor(seconds / 3600);
      return `${hours} ${hours === 1 ? "hour" : "hours"} ago`;
    }
    if (seconds < 172_800) return "yesterday";

    return new Date(at * 1000).toLocaleDateString(undefined, {
      day: "numeric",
      month: "short",
    });
  }

  function say(message: string) {
    status = message;
    setTimeout(() => (status = ""), 1600);
  }

  async function refresh() {
    entries = await dictationHistory();
  }

  async function copy(entry: HistoryEntry) {
    await writeText(entry.text);
    say("Copied");
  }

  async function forget(entry: HistoryEntry) {
    await forgetTranscription(entry.at);
    await refresh();
  }

  async function clearAll() {
    if (!confirmingClear) {
      confirmingClear = true;
      // Reverts on its own, so an unconfirmed press does not leave the
      // button armed for the next person to walk past.
      setTimeout(() => (confirmingClear = false), 4000);
      return;
    }
    confirmingClear = false;
    const gone = await clearDictationHistory();
    await refresh();
    say(`Deleted ${gone} ${gone === 1 ? "transcript" : "transcripts"}`);
  }

  onMount(() => {
    let unlisten: UnlistenFn | undefined;

    (async () => {
      await refresh();
      // A dictation can finish while this panel is open, and a history that
      // does not show it looks broken.
      unlisten = await listen("dictation:recorded", () => void refresh());
    })();

    return () => unlisten?.();
  });
</script>

<Section
  label="Transcripts"
  description="Every dictation, newest first, kept on this machine only. Turn off Keep a history above to stop recording them."
  bare
>
  <div class="tools">
    <div class="search">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" aria-hidden="true">
        <circle cx="11" cy="11" r="7" />
        <path d="m20 20-3.5-3.5" stroke-linecap="round" />
      </svg>
      <input bind:value={filter} placeholder="Search transcripts" spellcheck="false" />
    </div>

    {#if status}<span class="status">{status}</span>{/if}
    <span class="spacer"></span>

    {#if entries.length}
      <Button
        label={confirmingClear ? "Delete everything?" : "Clear history"}
        tone="danger"
        onclick={clearAll}
      />
    {/if}
  </div>

  {#if shown.length === 0}
    <p class="empty">
      {entries.length === 0
        ? "Nothing dictated yet. Every transcript will appear here."
        : "No transcript matches that."}
    </p>
  {:else}
    <div class="sill-card">
      {#each shown as entry (entry.at)}
        <div class="sill-setting entry">
          <div class="meta">
            <span class="at">{when(entry.at)}</span>
            {#if entry.app}<span class="dot">·</span><span>{entry.app}</span>{/if}
            <span class="dot">·</span>
            <span>{entry.words} {entry.words === 1 ? "word" : "words"}</span>
            <span class="spacer"></span>
            <button onclick={() => void copy(entry)}>Copy</button>
            <button class="danger" onclick={() => void forget(entry)}>Delete</button>
          </div>
          <p class="text">{entry.text}</p>
        </div>
      {/each}
    </div>
  {/if}
</Section>

<style>
  .tools {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-bottom: var(--space-3);
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 260px;
    padding: 0 var(--space-2);
    height: 30px;
    border-radius: var(--radius-sm);
    background: var(--fill-1);
    box-shadow: inset 0 0 0 1px var(--hairline);
    color: var(--text-3);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    outline: none;
    user-select: text;
  }

  .search input::placeholder {
    color: var(--text-3);
  }

  .status {
    font-size: var(--text-meta);
    color: var(--core-accent);
  }

  .spacer {
    flex: 1;
  }

  .entry {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .meta {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--text-group);
    color: var(--text-3);
  }

  .dot {
    opacity: 0.6;
  }

  .at {
    color: var(--text-2);
  }

  /* Revealed on hover, so a long list is transcripts rather than a wall of
     buttons. They stay reachable by keyboard because focus shows them too. */
  .meta button {
    padding: 2px var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-group);
    cursor: pointer;
    opacity: 0;
    transition:
      opacity 0.15s var(--ease),
      background-color 0.15s var(--ease),
      color 0.15s var(--ease);
  }

  .entry:hover .meta button,
  .meta button:focus-visible {
    opacity: 1;
  }

  .meta button:hover {
    background: var(--fill-3);
    color: var(--text-1);
  }

  .meta button.danger:hover {
    background: rgba(var(--accent-red-rgb), 0.14);
    color: var(--accent-red);
  }

  .text {
    margin: 0;
    /* Selectable: reading a transcript back and taking half of it is the
       point of keeping them. */
    user-select: text;
    font-size: var(--text-body);
    line-height: 1.55;
    color: var(--text-1);
  }

  .empty {
    margin: 0;
    padding: var(--space-5) 0;
    max-width: 56ch;
    font-size: var(--text-body);
    line-height: 1.7;
    color: var(--text-2);
  }
</style>
