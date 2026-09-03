<script lang="ts">
  /**
   * The emoji picker's settings.
   *
   * Its own panel rather than a section under Snippets: it has more than one
   * decision in it, one of them is about how people see themselves, and
   * burying that under somebody else's heading is not the place for it.
   */
  import { onMount } from "svelte";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Segmented from "./Segmented.svelte";
  import { emojiTones, type Preferences, type ToneChoice } from "$lib/settings";
  import { rovingTab, rovingTo } from "$lib/roving";

  interface Props {
    prefs: Preferences;
    commit: () => void;
  }

  let { prefs, commit }: Props = $props();

  /**
   * The tones, each carrying the hand that shows it.
   *
   * Asked for rather than written here, because the swatch is the emoji itself
   * and which characters exist is a fact about Unicode.
   */
  let tones = $state<ToneChoice[]>([]);

  onMount(async () => {
    tones = await emojiTones();
  });

  const ACTIONS = [
    { value: "paste", label: "Paste it" },
    { value: "copy", label: "Copy it" },
  ];

  /** Which tone is on, or -1 before the list has arrived. */
  const chosen = $derived(tones.findIndex((tone) => tone.id === prefs.emoji.tone));

  let swatches: (HTMLButtonElement | null)[] = [];

  /**
   * What to call a tone out loud.
   *
   * The swatch is a hand in that tone, which is the whole reason these are
   * drawn rather than listed, and a hand says nothing to somebody who is not
   * looking at it. The id was standing in for the name, so a reader said
   * "mediumLight": an internal spelling read out as if it were English.
   */
  function nameOf(id: ToneChoice["id"]): string {
    const spaced = id.replace(/([A-Z])/g, " $1").toLowerCase();
    return spaced.charAt(0).toUpperCase() + spaced.slice(1);
  }

  function pick(tone: ToneChoice["id"]) {
    prefs.emoji.tone = tone;
    commit();
  }

  /* The arrows move along the row of swatches, which is what a radio group
     does. Six buttons that were each their own Tab stop and answered no
     arrow key are six buttons pretending to be a group. */
  function onKeydown(event: KeyboardEvent, at: number) {
    const next = rovingTo(event.key, at, tones.length);
    if (next === null) return;

    event.preventDefault();
    pick(tones[next].id);
    swatches[next]?.focus();
  }
</script>

<Section
  label="Emoji"
  description="Search every emoji by name or by the shortcode people actually type, then put one where you were writing."
>
  <Row
    title="Skin tone"
    description="Applies to the emoji that have one. A waving hand does; a birthday cake does not."
  >
    {#snippet control()}
      <div class="tones" role="radiogroup" aria-label="Skin tone">
        {#each tones as tone, index (tone.id)}
          <button
            class="tone"
            class:on={prefs.emoji.tone === tone.id}
            role="radio"
            aria-checked={prefs.emoji.tone === tone.id}
            aria-label={nameOf(tone.id)}
            tabindex={rovingTab(index, chosen)}
            bind:this={swatches[index]}
            onclick={() => pick(tone.id)}
            onkeydown={(event) => onKeydown(event, index)}
          >
            {tone.swatch}
          </button>
        {/each}
      </div>
    {/snippet}
  </Row>

  <Row
    title="What Enter does"
    description="Pasting is what a picker is for: you were writing something and wanted a face in it. Copying is for the places Sill cannot paste into."
  >
    {#snippet control()}
      <Segmented
        label="What Enter does"
        value={prefs.emoji.primary}
        options={ACTIONS}
        onchange={(next) => {
          prefs.emoji.primary = next as "paste" | "copy";
          commit();
        }}
      />
    {/snippet}
  </Row>

  <Row
    title="Learning what you call things"
    description="Always on, and nothing to configure. Search for one by a word of your own, choose it twice, and that word finds it first from then on. This is the same learning the rest of the launcher does, so there is no model involved and nothing leaves the machine."
  />
</Section>

<style>
  .tones {
    display: flex;
    align-items: center;
    gap: var(--space-half);
  }

  /* No border until it is the chosen one. Six bordered swatches read as six
     buttons competing rather than one choice with six answers. */
  .tone {
    display: grid;
    place-items: center;
    width: 30px;
    height: var(--control-height);
    padding: 0;
    font-size: var(--text-query);
    line-height: 1;
    background: transparent;
    border: none;
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .tone:hover {
    background: var(--fill-1);
  }

  .tone.on {
    background: var(--fill-3);
  }
</style>
