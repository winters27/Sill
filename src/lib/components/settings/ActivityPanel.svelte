<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Section from "./Section.svelte";
  import Button from "./Button.svelte";
  import Row from "./Row.svelte";
  import Instead from "../Instead.svelte";

  type Done = {
    id: number;
    action: string;
    target: string;
    message: string;
    at: number;
    undoable: boolean;
  };

  let entries = $state<Done[]>([]);
  let said = $state("");
  let working = $state<number | null>(null);

  async function refresh() {
    try {
      entries = await invoke<Done[]>("activity");
    } catch (err) {
      said = `${err}`;
    }
  }

  async function undo(entry: Done) {
    working = entry.id;
    said = "";
    try {
      said = await invoke<string>("undo_activity", { id: entry.id });
    } catch (err) {
      said = `${err}`;
    } finally {
      working = null;
      await refresh();
    }
  }

  async function clear() {
    await invoke("clear_activity");
    said = "";
    await refresh();
  }

  /** "just now", "4 minutes ago", "18:42". */
  function when(at: number): string {
    const seconds = Math.max(0, Math.floor(Date.now() / 1000) - at);
    if (seconds < 45) return "just now";
    if (seconds < 3600) {
      const minutes = Math.round(seconds / 60);
      return `${minutes} ${minutes === 1 ? "minute" : "minutes"} ago`;
    }
    return new Date(at * 1000).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  onMount(refresh);
</script>

<Section
  label="What Sill has done"
  description="This run only. An undo describes a change rather than holding what changed, and a window or a clipboard from before a restart is not there to be put back."
>
  {#if entries.length === 0}
    <Instead
      tone="empty"
      inline
      headline="Nothing yet"
      hint="Anything you run from the launcher shows up here, and whatever can be taken back says so."
    />
  {:else}
    <ul class="log">
      {#each entries as entry (entry.id)}
        <li class="entry">
          <span class="text">
            <span class="what">
              <span class="action">{entry.action}</span>
              <span class="target">{entry.target}</span>
            </span>
            <span class="said">{entry.message} &middot; {when(entry.at)}</span>
          </span>

          {#if entry.undoable}
            <Button
              label="Undo"
              busy={working === entry.id}
              onclick={() => undo(entry)}
            />
          {/if}
        </li>
      {/each}
    </ul>

    <!-- not a setting: the one thing this list can be told to do -->
    <Row title="Clear this list" description="Forgets what was done. Nothing done is undone.">
      {#snippet control()}
        <Button label="Clear" tone="danger" onclick={clear} />
      {/snippet}
    </Row>
  {/if}

  {#if said}
    <p class="note">{said}</p>
  {/if}
</Section>

<style>
  .note {
    margin: 0;
    padding: var(--space-2) 0;
    color: var(--text-2);
    font-size: var(--text-meta);
    line-height: 1.5;
  }

  .log {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .entry {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
  }

  /* A hairline between rows, not around them: this is a list of one kind of
     thing, and a box per row reads as a set of cards. */
  .entry + .entry {
    box-shadow: var(--ring-top);
  }

  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-half);
    min-width: 0;
  }

  .what {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }

  .action {
    flex: none;
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
  }

  /* The thing it was done to, which is the half somebody scans for. */
  .target {
    color: var(--text-2);
    font-size: var(--text-body);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .said {
    color: var(--text-3);
    font-size: var(--text-meta);
  }

</style>
