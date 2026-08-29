<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";

  interface Props {
    paths: string[];
    onchange: (paths: string[]) => void;
  }

  let { paths = $bindable(), onchange }: Props = $props();

  async function add() {
    const picked = await open({ directory: true, multiple: true });
    if (!picked) return;

    const chosen = Array.isArray(picked) ? picked : [picked];
    // A duplicate folder would narrow nothing and read as a mistake.
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
      <span class="path" title={path}>{path}</span>
      <button class="remove" aria-label="Stop searching {path}" onclick={() => remove(path)}>
        <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
        </svg>
      </button>
    </div>
  {/each}

  {#if paths.length === 0}
    <p class="empty">Searching everywhere Everything indexes.</p>
  {/if}

  <button class="add" onclick={add}>Add a folder…</button>
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 5px;
    align-items: flex-start;
  }

  .entry {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 6px 6px 10px;
    border-radius: var(--radius-sm);
    background: rgba(var(--accent-rgb), 0.06);
  }

  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    /* A path is a machine string; reading one in the body face is harder. */
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--core-foreground);
    direction: rtl;
    text-align: left;
  }

  .remove {
    flex: none;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    transition: background-color 0.15s var(--ease), color 0.15s var(--ease);
  }

  .remove:hover {
    background-color: rgba(var(--accent-red-rgb), 0.15);
    color: var(--accent-red);
  }

  .empty {
    margin: 0;
    font-size: 12px;
    color: var(--text-faint);
  }

  .add {
    padding: 6px 12px;
    border: 0;
    border-radius: var(--radius-sm);
    background: rgba(var(--accent-rgb), 0.1);
    color: var(--core-foreground);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: background-color 0.15s var(--ease);
  }

  .add:hover {
    background: rgba(var(--accent-rgb), 0.18);
  }
</style>
