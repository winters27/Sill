<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import Instead from "../Instead.svelte";
  import { standing } from "$lib/instead";
  import { hint } from "$lib/hint";

  interface Props {
    paths: string[];
    onchange: (paths: string[]) => void;
    /**
     * Pick files rather than folders.
     *
     * The list of scripts allowed to run as administrator is the one place
     * that has to name a file. Allowing a folder there would allow every file
     * dropped into it afterwards, which is the standing grant the list exists
     * to avoid, so the picker has to refuse to offer one.
     */
    files?: boolean;
    /** What an empty list says, and what the button offers. */
    headline?: string;
    hint?: string;
    add?: string;
    /** What removing a row is called, for a screen reader. */
    removes?: string;
  }

  let {
    paths = $bindable(),
    onchange,
    files = false,
    headline = "No folders chosen",
    hint: nothing = "Sill is searching everywhere Everything indexes.",
    add: adds = "Add a folder…",
    removes = "Stop searching",
  }: Props = $props();

  async function pick() {
    const picked = await open({ directory: !files, multiple: true });
    if (!picked) return;

    const chosen = Array.isArray(picked) ? picked : [picked];
    // A duplicate would narrow nothing and read as a mistake.
    const next = [...paths, ...chosen.filter((p) => !paths.includes(p))];
    paths = next;
    onchange(next);
  }

  function remove(path: string) {
    const next = paths.filter((p) => p !== path);
    paths = next;
    onchange(next);
  }
</script>

<div class="list">
  {#each paths as path (path)}
    <div class="entry">
      <!-- A long path is truncated to the row, and the folder it names is the
           half that gets cut. -->
      <span class="path" use:hint={path}>{path}</span>
      <button class="remove" aria-label="{removes} {path}" onclick={() => remove(path)}>
        <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
        </svg>
      </button>
    </div>
  {/each}

  <Instead
    tone={standing({ failed: false, loading: false, count: paths.length })}
    inline
    {headline}
    hint={nothing}
  />

  <button class="add" onclick={pick}>{adds}</button>
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    align-items: flex-start;
  }

  .entry {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--fill-1);
  }

  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    /* A path is a machine string; reading one in the body face is harder. */
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--text-1);
    direction: rtl;
    text-align: left;
  }

  .remove {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--icon-tile-sm);
    height: var(--icon-tile-sm);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .remove:hover {
    background-color: var(--danger-fill);
    color: var(--danger);
  }

  .add {
    padding: var(--space-1) var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
    transition: background-color var(--motion-state) var(--ease);
  }

  .add:hover {
    background: var(--fill-3);
  }
</style>
