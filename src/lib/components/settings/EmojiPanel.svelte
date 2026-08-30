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
        {#each tones as tone (tone.id)}
          <button
            class="tone"
            class:on={prefs.emoji.tone === tone.id}
            role="radio"
            aria-checked={prefs.emoji.tone === tone.id}
            aria-label={tone.id}
            onclick={() => {
              prefs.emoji.tone = tone.id;
              commit();
            }}
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
    gap: 2px;
  }

  /* No border until it is the chosen one. Six bordered swatches read as six
     buttons competing rather than one choice with six answers. */
  .tone {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    font-size: var(--text-query);
    line-height: 1;
    background: transparent;
    border: none;
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .tone:hover {
    background: var(--surface-raised);
  }

  .tone.on {
    background: var(--fill-3);
  }
</style>
