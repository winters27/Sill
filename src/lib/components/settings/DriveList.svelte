<script lang="ts">
  /**
   * The drives on this machine, and which of them Sill reads.
   *
   * Drives rather than a path field, because a drive is a thing somebody can
   * point at. Typing `D:\` into a box requires knowing it is there; a list of
   * what is mounted does not.
   */
  import { onMount } from "svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import { listDrives, indexFolder, type Drive } from "$lib/exthost/commands";
  import Instead from "../Instead.svelte";
  import { couldNot, standing } from "$lib/instead";

  interface Props {
    /** Told after a change, so the panel showing the roots can catch up. */
    onchange?: (roots: string[]) => void;
  }

  let { onchange }: Props = $props();

  let drives = $state<Drive[]>([]);
  let working = $state<string | null>(null);
  let trouble = $state("");

  async function refresh() {
    drives = await listDrives();
  }

  onMount(refresh);

  /** What a drive is called on the row. */
  function name(drive: Drive): string {
    const letter = drive.root.replace(/[\\/]+$/, "");

    return drive.label ? `${letter} ${drive.label}` : letter;
  }

  /**
   * What indexing this one is likely to cost.
   *
   * Said plainly rather than left to be discovered. A network or cloud drive
   * can mean a round trip for every folder, and on some of them reading a file
   * downloads it.
   */
  function caution(drive: Drive): string {
    switch (drive.kind) {
      case "network":
        return "Over a network. Reading it can be slow, and on a cloud drive it can download what it reads.";
      case "removable":
        return "Removable, so what Sill remembers is wrong once it is unplugged.";
      case "optical":
        return "A disc.";
      default:
        return "Skips Windows and installed programs, which nobody searches for by name.";
    }
  }

  async function set(drive: Drive, wanted: boolean) {
    working = drive.root;
    trouble = "";

    try {
      const roots = await indexFolder(drive.root, wanted);
      onchange?.(roots);
      await refresh();
    } catch (err) {
      // The errand rather than the call. `${err}` here was the Rust side of
      // `index_folder` reported to somebody who pressed a switch labelled with
      // a drive letter, and it named neither the drive nor the switch.
      console.error("[sill] could not change which drives are indexed", err);
      trouble = couldNot(
        `${wanted ? "start" : "stop"} indexing ${name(drive)}. Try the switch again.`,
      );
      await refresh();
    } finally {
      working = null;
    }
  }
</script>

<div class="list">
  {#each drives.filter((d) => d.kind !== "optical") as drive (drive.root)}
    <div class="entry">
      <div class="what">
        <span class="name">{name(drive)}</span>
        <span class="note">{caution(drive)}</span>
      </div>
      <Toggle
        checked={drive.indexed}
        disabled={working === drive.root}
        label="Index {name(drive)}"
        onchange={(next) => set(drive, next)}
      />
    </div>
  {/each}

  <Instead
    tone={standing({ failed: false, loading: false, count: drives.length })}
    inline
    headline="No drives found"
    hint="Sill asks Windows which volumes are mounted, and it answered with none."
  />

  {#if trouble}
    <p class="trouble">{trouble}</p>
  {/if}
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: var(--space-half);
  }

  .entry {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-2) 0;
  }

  /* One hairline between rows and none around the group: the section already
     has an edge, and a second one inside it reads as a box in a box. */
  .entry + .entry {
    border-top: 1px solid var(--hairline);
  }

  .what {
    display: flex;
    flex-direction: column;
    gap: var(--space-half);
    min-width: 0;
  }

  .name {
    font-size: var(--text-body);
    color: var(--text-1);
  }

  .note {
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  /* An action that failed, not a state the list is in: the drives are still
     listed and one switch did not move. */
  .trouble {
    margin: 0;
    padding: var(--space-1) 0;
    font-size: var(--text-meta);
    color: var(--danger);
  }
</style>
