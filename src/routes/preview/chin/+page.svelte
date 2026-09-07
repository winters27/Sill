<script lang="ts">
  /**
   * A development route. Reachable with `npm run dev`, never linked from the
   * app, and of no use to anyone running Sill rather than working on it.
   *
   * The footer in the three states a newer Sill puts it in. Each one is a
   * moment that only happens when a release exists and the running build is
   * behind it, so the only other way to look at any of them is to cut a
   * release and wait, and the state that matters most lasts until it is
   * pressed.
   *
   * Drawn at the window's own width, because what went wrong here was width:
   * a sentence and a button together left the sentence clipped mid-word.
   *
   * The pinned readings that ask Rust for their numbers are left out. The
   * clock is the one that answers for itself, so the row here is a little
   * roomier than the same row in the application.
   */
  import "$lib/theme/theme.css";
  import Footer from "$lib/components/Footer.svelte";
  import { chinLine, type Progress } from "$lib/update";
  import type { Preferences } from "$lib/settings";

  const PINNED = {
    widgets: { pinned: ["clock"], seconds: false, clocks: [] },
  } as unknown as Preferences;

  const STATES: { of: string; progress: Progress }[] = [
    {
      of: "There is a newer one",
      progress: { kind: "available", version: "0.1.4", notes: null },
    },
    {
      of: "The bytes are arriving",
      progress: { kind: "downloading", version: "0.1.4", percent: 42 },
    },
    {
      of: "Downloaded, waiting on a restart",
      progress: { kind: "ready", version: "0.1.4" },
    },
    { of: "Nothing newer, which is most of the time", progress: { kind: "upToDate" } },
  ];
</script>

<div class="page">
  {#each STATES as state (state.of)}
    <section>
      <h2>{state.of}</h2>
      <div class="window">
        <Footer
          mode="root"
          toast={null}
          status=""
          update={chinLine(state.progress)}
          prefs={PINNED}
          viewTag={undefined}
          hasActions={true}
          onbuiltin={() => {}}
          onrun={() => {}}
          onactions={() => {}}
          ontoastaction={() => {}}
          onupdate={() => {}}
        />
      </div>
    </section>
  {/each}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    padding: var(--space-6);
    background: var(--fill-0);
    min-height: 100vh;
  }

  h2 {
    margin: 0 0 var(--space-2);
    color: var(--text-3);
    font-size: var(--text-meta);
    font-weight: var(--weight-body);
  }

  /* The launcher's own width, since that is the constraint under test. */
  .window {
    width: 750px;
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    overflow: hidden;
  }
</style>
