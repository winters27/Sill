<!--
  The keyboard reference.

  Every chord here comes from `keyboard_reference`, which assembles it from the
  movement preset, the action shortcuts and the summon key. Nothing on this
  page is written down: a reference somebody types out is wrong the first time
  a key changes, and the person reading it has no way to tell.

  That is also why there is a `verify:source` rule refusing a literal chord in
  this file.
-->
<script lang="ts">
  import { keyboardReference, type KeySection } from "$lib/exthost/commands";
  import Instead from "$lib/components/Instead.svelte";
  import Chord from "$lib/components/Chord.svelte";
  import { standing } from "$lib/instead";
  import { PAGE_KEYS } from "$lib/keys";

  /**
   * What Rust says can be bound, then what the launcher answers itself.
   *
   * The second half is written beside the handler that implements it, in
   * `PAGE_KEYS`, because those keys are decided in the page and never reach
   * Rust as a binding.
   */
  let bound = $state<KeySection[]>([]);
  const sections = $derived<KeySection[]>([
    ...bound,
    ...PAGE_KEYS.map((section) => ({
      title: section.title,
      keys: section.keys.map((key) => ({ ...key, changed: false, contested: false, refused: false })),
    })),
  ]);
  let sheet = $state<HTMLDivElement | null>(null);

  /**
   * Scrolls the reference by a step, for the arrow keys.
   *
   * The sheet is a page to read rather than a list to walk, so the keys that
   * walk a list move the page instead. The search field keeps focus the whole
   * time, which is why this is asked for rather than left to the browser.
   */
  export function scrollBy(steps: number, page = false) {
    if (!sheet) return;
    const step = page ? sheet.clientHeight * 0.9 : 48;
    sheet.scrollBy({ top: steps * step });
  }
  let failed = $state<string | null>(null);
  let loading = $state(true);

  $effect(() => {
    let current = true;

    keyboardReference()
      .then((found) => {
        if (!current) return;
        bound = found;
        loading = false;
      })
      .catch((err) => {
        if (!current) return;
        failed = `${err}`;
        loading = false;
      });

    return () => {
      current = false;
    };
  });

  const count = $derived(
    sections.reduce((all: number, one: KeySection) => all + one.keys.length, 0),
  );
  const tone = $derived(standing({ failed: failed !== null, loading, count }));
</script>

<div class="sheet" bind:this={sheet}>
  {#if tone === "content"}
    {#each sections as section (section.title)}
      <section>
        <h2>{section.title}</h2>
        <dl>
          {#each section.keys as key (key.chord + key.does)}
            <div class="line" class:contested={key.contested}>
              <dt><Chord chord={key.chord} /></dt>
              <dd>
                {key.does}
                {#if key.refused}
                  <!-- Set and doing nothing, which the sheet must not present
                       as a key that works. -->
                  <span class="note">Windows would not register this key</span>
                {:else if key.contested}
                  <span class="note">another action takes this key</span>
                {:else if key.changed}
                  <span class="note">changed by you</span>
                {/if}
              </dd>
            </div>
          {/each}
        </dl>
      </section>
    {/each}
  {:else}
    <Instead
      {tone}
      headline={failed ? "The keys could not be read" : "No keys are bound yet"}
      hint={failed ?? ""}
    />
  {/if}
</div>

<style>
  .sheet {
    overflow-y: auto;
    padding: var(--space-3) var(--space-4) var(--space-4);
  }

  section {
    margin-bottom: var(--space-4);
  }

  h2 {
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
    color: var(--text-3);
    margin: 0 0 var(--space-2);
  }

  dl {
    display: grid;
    /* The keys share one column so the chords line up down the page, which is
       what makes it scannable rather than a list of sentences. */
    grid-template-columns: max-content 1fr;
    gap: var(--space-1) var(--space-3);
    margin: 0;
  }

  .line {
    display: contents;
  }

  dt,
  dd {
    margin: 0;
    align-self: center;
  }

  dd {
    font-size: var(--text-body);
    color: var(--text-2);
  }

  .note {
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .contested .note {
    color: var(--danger);
  }
</style>
