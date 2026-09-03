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

  .empty {
    margin: 0;
    font-size: var(--text-meta);
    color: var(--text-3);
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
