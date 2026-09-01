<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import TextField from "./TextField.svelte";
  import type { Preferences, TtsEngine } from "$lib/settings";

  interface Props {
    prefs: Preferences;
    commit: () => void;
  }

  let { prefs, commit }: Props = $props();

  type VoiceStatus = {
    id: string;
    label: string;
    locale: string;
    note: string;
    installed: boolean;
    bytes: number;
  };

  let voices = $state<VoiceStatus[]>([]);
  let stage = $state("");
  let fraction = $state<number | null>(null);
  let downloading = $state<string | null>(null);
  let said = $state("");
  let speaking = $state(false);

  /**
   * The three ways to say something, and what each one costs.
   *
   * The blurb is the whole point of this panel: somebody arriving here has
   * decided the system voice is not good enough, and the question they have is
   * which of the other two they want, which is a question about keys and
   * downloads rather than about audio.
   */
  const ENGINES: { id: TtsEngine; name: string; blurb: string; cost: string }[] = [
    {
      id: "system",
      name: "Windows",
      blurb: "The voice Windows ships with. Works offline and needs nothing.",
      cost: "Free, and it sounds it",
    },
    {
      id: "http",
      name: "A service",
      blurb:
        "Anything speaking OpenAI's speech API: OpenAI itself, or a server on your own machine such as Kokoro.",
      cost: "Needs an address, and a key unless it is local",
    },
    {
      id: "piper",
      name: "Offline voice",
      blurb:
        "A neural voice that runs here. No key, no account, and nothing leaves the machine.",
      cost: "One download, then free forever",
    },
  ];

  const chosen = $derived(voices.find((v) => v.id === prefs.tts.piperVoice));
  const anyInstalled = $derived(voices.some((v) => v.installed));

  /** Short, so trying a voice costs a second rather than a paragraph. */
  const SAMPLE = "This is how Sill will read your text out loud.";

  function mb(bytes: number): string {
    return `${Math.round(bytes / 1_048_576)} MB`;
  }

  async function refresh() {
    try {
      voices = await invoke<VoiceStatus[]>("piper_voices");
    } catch (err) {
      said = `Could not read the voice list: ${err}`;
    }
  }

  function use(engine: TtsEngine) {
    prefs.tts.engine = engine;
    commit();
  }

  async function download(id: string) {
    downloading = id;
    said = "";
    stage = "Starting";
    fraction = 0;
    try {
      await invoke("install_piper_voice", { voice: id });
      await refresh();
      // Downloading a voice is choosing it. Nobody fetches sixty megabytes to
      // then pick it from a list.
      prefs.tts.piperVoice = id;
      prefs.tts.engine = "piper";
      commit();
    } catch (err) {
      said = `${err}`;
    } finally {
      downloading = null;
      stage = "";
      fraction = null;
    }
  }

  async function remove(id: string) {
    try {
      await invoke("remove_piper_voice", { voice: id });
      await refresh();
    } catch (err) {
      said = `${err}`;
    }
  }

  /** Speaks in whichever engine is selected right now. */
  async function sample() {
    said = "";
    speaking = true;
    try {
      await invoke("speak_sample", { text: SAMPLE });
    } catch (err) {
      said = `${err}`;
    } finally {
      speaking = false;
    }
  }

  /** Speaks in one downloaded voice without having to select it first. */
  async function preview(id: string) {
    said = "";
    try {
      await invoke("speak_piper_sample", { voice: id, text: SAMPLE });
    } catch (err) {
      said = `${err}`;
    }
  }

  onMount(() => {
    void refresh();

    let stop: UnlistenFn | undefined;
    void listen<{ fraction: number; stage: string }>(
      "sill://tts-download",
      (event) => {
        fraction = event.payload.fraction;
        stage = event.payload.stage;
      },
    ).then((off) => (stop = off));

    return () => stop?.();
  });
</script>

<!--
  Cards rather than a segmented control, and for the reason the AI panel uses
  them: the choice is not between three words, it is between three
  arrangements of key, download and quality, and none of that fits on a
  segment.
-->
<Section
  bare
  label="Which voice reads"
  description="Used by Read Aloud in the action panel, and by any key you bind to it."
>
  <div class="stack" role="radiogroup" aria-label="Which voice reads">
    {#each ENGINES as engine (engine.id)}
      <div class="card" class:on={prefs.tts.engine === engine.id}>
        <button
          type="button"
          class="pick"
          role="radio"
          aria-checked={prefs.tts.engine === engine.id}
          onclick={() => use(engine.id)}
        >
          <span class="text">
            <span class="name">
              {engine.name}
              {#if prefs.tts.engine === engine.id}
                <span class="reading">Reading</span>
              {/if}
            </span>
            <span class="blurb">{engine.blurb}</span>
            <span class="cost">{engine.cost}</span>
          </span>
        </button>
      </div>
    {/each}
  </div>
</Section>

{#if prefs.tts.engine === "http"}
  <Section
    label="The service"
    description="Sill sends the text and plays back what comes home. Anything that answers OpenAI's /v1/audio/speech will do, which includes servers you run yourself."
  >
    <Row title="Address">
      {#snippet control()}
        <TextField
          value={prefs.tts.provider.baseUrl ?? ""}
          oninput={(next) => {
            prefs.tts.provider.baseUrl = next.trim();
            commit();
          }}
          placeholder="https://api.openai.com/v1"
          ariaLabel="Address"
          full
          mono
        />
      {/snippet}
    </Row>

    <Row
      title="Key"
      description="Left empty for a server on this machine, which usually wants none. Encrypted before it is written, never stored as plain text."
    >
      {#snippet control()}
        <TextField
          value={prefs.tts.provider.apiKey ?? ""}
          oninput={(next) => {
            prefs.tts.provider.apiKey = next.trim();
            commit();
          }}
          placeholder="Paste it here"
          ariaLabel="Key"
          full
          mono
          secret
        />
      {/snippet}
    </Row>

    <Row title="Model" description="Empty uses tts-1.">
      {#snippet control()}
        <TextField
          value={prefs.tts.provider.lastModelId ?? ""}
          oninput={(next) => {
            prefs.tts.provider.lastModelId = next.trim();
            commit();
          }}
          placeholder="tts-1"
          ariaLabel="Model"
          mono
        />
      {/snippet}
    </Row>

    <Row title="Voice" description="Whatever that service calls its voices.">
      {#snippet control()}
        <TextField
          value={prefs.tts.voice}
          oninput={(next) => {
            prefs.tts.voice = next.trim();
            commit();
          }}
          placeholder="alloy"
          ariaLabel="Voice"
          mono
        />
      {/snippet}
    </Row>
  </Section>
{/if}

{#if prefs.tts.engine === "piper"}
  <!--
    Every voice is a card that can be listened to before it is chosen, because
    the only question anybody has here is what it sounds like, and a name in a
    dropdown cannot answer it.
  -->
  <Section
    bare
    label="Offline voices"
    description={anyInstalled
      ? "Downloaded once and kept. These run on this machine and work with no network."
      : "Neural voices that run on this machine. The first download brings the engine with it; after that a voice is 60 MB on its own."}
  >
    <div class="voices">
      {#each voices as voice (voice.id)}
        <div
          class="card voice"
          class:on={voice.installed && prefs.tts.piperVoice === voice.id}
        >
          <div class="voice-head">
            <span class="text">
              <span class="name">
                {voice.label}
                {#if voice.installed && prefs.tts.piperVoice === voice.id}
                  <span class="reading">Reading</span>
                {/if}
              </span>
              <span class="blurb">{voice.note}</span>
              <span class="cost">
                {voice.locale}
                {#if !voice.installed}
                  &middot; {mb(voice.bytes)} to download
                {/if}
              </span>
            </span>
          </div>

          <div class="actions">
            {#if voice.installed}
              {#if prefs.tts.piperVoice !== voice.id}
                <Button
                  label="Use this"
                  onclick={() => {
                    prefs.tts.piperVoice = voice.id;
                    commit();
                  }}
                />
              {/if}
              <Button label="Listen" onclick={() => preview(voice.id)} />
              <Button label="Remove" tone="danger" onclick={() => remove(voice.id)} />
            {:else}
              <Button
                label={downloading === voice.id ? "Downloading" : "Download"}
                busy={downloading === voice.id}
                onclick={() => download(voice.id)}
              />
            {/if}
          </div>

          {#if downloading === voice.id}
            <div class="progress">
              <div class="bar">
                <div
                  class="fill"
                  style:width={`${Math.round((fraction ?? 0) * 100)}%`}
                ></div>
              </div>
              <span class="stage">{stage}</span>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </Section>
{/if}

<Section label="Try it">
  <Row
    title="Read a sample"
    description="One sentence, in whichever voice is selected above."
  >
    {#snippet control()}
      <Button label="Read a sample" busy={speaking} onclick={sample} />
    {/snippet}
  </Row>

  {#if said}
    <p class="said">{said}</p>
  {/if}
</Section>

<style>
  /*
   * The same grid the provider cards use, so the two panels read as one
   * product rather than as two people's work.
   */
  .stack {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(236px, 1fr));
    gap: var(--space-2);
  }

  .voices {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(272px, 1fr));
    gap: var(--space-2);
  }

  .card {
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    transition: background-color var(--motion-enter) ease;
  }

  /*
   * The one in use, washed rather than badged. The accent doing the job it is
   * reserved for: saying which of several things is selected.
   */
  .card.on {
    background: var(--accent-fill);
  }

  .pick {
    flex: 1;
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 0;
    border-radius: var(--radius-lg);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .pick:hover {
    background: var(--fill-1);
  }

  .pick:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 2px var(--accent-line);
  }

  .voice-head {
    padding: var(--space-3) var(--space-3) 0;
  }

  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
  }

  .blurb {
    color: var(--text-2);
    font-size: var(--text-meta);
    line-height: 1.5;
  }

  /* What it costs, dimmer than what it is: nobody reads this one first. */
  .cost {
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: 1.5;
  }

  .reading {
    flex: none;
    color: var(--accent);
    font-size: var(--text-meta);
    font-weight: var(--weight-regular);
    white-space: nowrap;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-3);
  }

  .progress {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 0 var(--space-3) var(--space-3);
  }

  .bar {
    flex: 1;
    height: 4px;
    border-radius: var(--radius-1);
    background: var(--fill-2);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 120ms linear;
  }

  .stage {
    color: var(--text-3);
    font-size: var(--text-meta);
    white-space: nowrap;
  }

  .said {
    margin: 0;
    padding: var(--space-2) 0 0;
    color: var(--text-2);
    font-size: var(--text-meta);
  }
</style>
