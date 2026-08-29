<script lang="ts">
  import type { AudioInputDevice } from "$lib/dictation";

  interface Props {
    devices: AudioInputDevice[];
    priority: string[];
    onchange: (priority: string[]) => void;
  }

  let { devices, priority, onchange }: Props = $props();

  /**
   * The list to draw: the chosen order first, then everything else.
   *
   * A device that has been unplugged since it was ranked stays in the list
   * rather than silently vanishing, or the order would quietly rewrite itself
   * every time a headset came out.
   */
  const rows = $derived.by(() => {
    const known = new Map(devices.map((device) => [device.id, device]));
    const ranked = priority.map((id) => ({
      id,
      name: known.get(id)?.name ?? id,
      present: known.has(id),
      ranked: true,
    }));
    const rest = devices
      .filter((device) => !priority.includes(device.id))
      .map((device) => ({ id: device.id, name: device.name, present: true, ranked: false }));
    return [...ranked, ...rest];
  });

  function move(id: string, by: number) {
    const next = [...priority];
    const from = next.indexOf(id);
    if (from === -1) return;

    const to = from + by;
    if (to < 0 || to >= next.length) return;

    [next[from], next[to]] = [next[to], next[from]];
    onchange(next);
  }

  function rank(id: string) {
    onchange([...priority, id]);
  }

  function unrank(id: string) {
    onchange(priority.filter((entry) => entry !== id));
  }
</script>

<div class="list">
  {#each rows as row, index (row.id)}
    <div class="row" class:absent={!row.present}>
      <span class="position">{row.ranked ? index + 1 : "–"}</span>
      <span class="name" title={row.id}>{row.name}</span>
      {#if !row.present}
        <span class="tag">not connected</span>
      {/if}

      <span class="spacer"></span>

      {#if row.ranked}
        <button
          aria-label="Move {row.name} up"
          disabled={index === 0}
          onclick={() => move(row.id, -1)}
        >
          <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true">
            <path d="M2 7.5 6 3.5l4 4" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
        <button
          aria-label="Move {row.name} down"
          disabled={index === priority.length - 1}
          onclick={() => move(row.id, 1)}
        >
          <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true">
            <path d="M2 4.5 6 8.5l4-4" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
        <button class="text" onclick={() => unrank(row.id)}>Remove</button>
      {:else}
        <button class="text" onclick={() => rank(row.id)}>Add</button>
      {/if}
    </div>
  {/each}

  {#if priority.length === 0}
    <p class="hint">Add a microphone to give it priority. Sill uses the first one connected.</p>
  {/if}
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 8px 7px 10px;
    border-radius: var(--radius-sm);
    background: rgba(var(--accent-rgb), 0.05);
  }

  .absent {
    opacity: 0.55;
  }

  .position {
    width: 14px;
    flex: none;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
    text-align: center;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12.5px;
  }

  .tag {
    flex: none;
    font-size: 11px;
    color: var(--text-faint);
  }

  .spacer {
    flex: 1;
  }

  button {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    flex: none;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    cursor: pointer;
    transition:
      background-color 0.15s var(--ease),
      color 0.15s var(--ease);
  }

  button:hover:not(:disabled) {
    background: rgba(var(--accent-rgb), 0.14);
    color: var(--core-foreground);
  }

  button:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .text {
    width: auto;
    padding: 0 8px;
    font-size: 12px;
  }

  .hint {
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--text-faint);
  }
</style>
