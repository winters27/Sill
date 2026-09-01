<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Segmented from "./Segmented.svelte";
  import Button from "./Button.svelte";
  import Select from "./Select.svelte";
  import TextField from "./TextField.svelte";
  import type { Preferences } from "$lib/settings";

  interface Props {
    prefs: Preferences;
    commit: () => void;
  }

  let { prefs, commit }: Props = $props();

  type VoiceStatus = {
    id: string;
    label: string;
    locale: string;
    installed: boolean;
  };

  let voices = $state<VoiceStatus[]>([]);
  let stage = $state("");
  let fraction = $state<number | null>(null);
  let busy = $state(false);
  let said = $state("");

  const chosen = $derived(
    voices.find((voice) => voice.id === prefs.speech.piperVoice),
  );

  /** What the sample button reads, kept short so a test costs a second. */
  const SAMPLE = "This is how Sill will read your text out loud.";

  async function refresh() {
    try {
      voices = await invoke<VoiceStatus[]>("piper_voices");
    } catch (err) {
      said = `Could not read the voice list: ${err}`;
    }
  }

  async function download() {
    busy = true;
    said = "";
    stage = "Starting";
    fraction = 0;
    try {
      await invoke("install_piper_voice", { voice: prefs.speech.piperVoice });
      await refresh();
      said = "Downloaded.";
    } catch (err) {
      said = `${err}`;
    } finally {
      busy = false;
      stage = "";
      fraction = null;
    }
  }

  async function remove() {
    try {
      await invoke("remove_piper_voice", { voice: prefs.speech.piperVoice });
      await refresh();
      said = "Removed.";
    } catch (err) {
      said = `${err}`;
    }
  }

  async function sample() {
    said = "";
    try {
      await invoke("speak_sample", { text: SAMPLE });
    } catch (err) {
      said = `${err}`;
    }
  }

  onMount(() => {
    void refresh();

    let stop: UnlistenFn | undefined;
    void listen<{ fraction: number; stage: string }>(
      "sill://speech-download",
      (event) => {
        fraction = event.payload.fraction;
        stage = event.payload.stage;
      },
    ).then((off) => (stop = off));

    return () => stop?.();
  });
</script>

<Section
  label="Speech"
  description="Reading text out loud, from the action panel or a key you bind to it."
>
  <Row
    title="Which voice reads"
    description="Windows is always available and sounds its age: the neural voices Windows 11 ships are reserved for Narrator. The other two are worth the setup."
  >
    {#snippet control()}
      <Segmented
        value={prefs.speech.engine}
        options={[
          { value: "system", label: "Windows" },
          { value: "http", label: "OpenAI-compatible" },
          { value: "piper", label: "Downloaded" },
        ]}
        onchange={(next) => {
          prefs.speech.engine = next as typeof prefs.speech.engine;
          commit();
        }}
      />
    {/snippet}
  </Row>

  {#if prefs.speech.engine === "http"}
    <Row
      title="Address"
      description="Anything speaking OpenAI's /v1/audio/speech. That is OpenAI itself, and local servers that copied it such as Kokoro-FastAPI or openedai-speech."
    >
      {#snippet control()}
        <TextField
          value={prefs.speech.provider.baseUrl ?? ""}
          oninput={(next) => {
            prefs.speech.provider.baseUrl = next.trim();
            commit();
          }}
          placeholder="https://api.openai.com/v1"
          ariaLabel="Address"
          mono
        />
      {/snippet}
    </Row>

    <Row
      title="API key"
      description="Left empty for a server on this machine, which usually wants none. Stored encrypted, never in the settings file as plain text."
    >
      {#snippet control()}
        <TextField
          value={prefs.speech.provider.apiKey ?? ""}
          oninput={(next) => {
            prefs.speech.provider.apiKey = next.trim();
            commit();
          }}
          placeholder="Paste it here"
          ariaLabel="API key"
          secret
          mono
        />
      {/snippet}
    </Row>

    <Row title="Model" description="Left empty uses tts-1.">
      {#snippet control()}
        <TextField
          value={prefs.speech.provider.lastModelId ?? ""}
          oninput={(next) => {
            prefs.speech.provider.lastModelId = next.trim();
            commit();
          }}
          placeholder="tts-1"
          ariaLabel="Model"
        />
      {/snippet}
    </Row>

    <Row title="Voice" description="Whatever that service calls its voices.">
      {#snippet control()}
        <TextField
          value={prefs.speech.voice}
          oninput={(next) => {
            prefs.speech.voice = next.trim();
            commit();
          }}
          placeholder="alloy"
          ariaLabel="Voice"
        />
      {/snippet}
    </Row>
  {/if}

  {#if prefs.speech.engine === "piper"}
    <Row
      title="Voice"
      description="A neural voice that runs on this machine. Nothing is downloaded until you ask for it, and nothing leaves the machine when it speaks."
    >
      {#snippet control()}
        <Select
          value={prefs.speech.piperVoice}
          options={voices.map((voice) => ({
            value: voice.id,
            label: `${voice.label}, ${voice.locale}${voice.installed ? "" : " (not downloaded)"}`,
          }))}
          onchange={(next) => {
            prefs.speech.piperVoice = next;
            commit();
          }}
        />
      {/snippet}
    </Row>

    <Row
      title={chosen?.installed ? "Downloaded" : "Download this voice"}
      description={chosen?.installed
        ? "Ready to use, and it works with no network."
        : "About 80 MB, plus the engine once. It is kept, so this is asked only the first time."}
    >
      {#snippet control()}
        {#if chosen?.installed}
          <Button label="Remove" tone="danger" onclick={remove} />
        {:else}
          <Button label={busy ? "Downloading" : "Download"} busy={busy} onclick={download} />
        {/if}
      {/snippet}
    </Row>

    {#if busy}
      <div class="progress">
        <div class="bar">
          <!-- Drawn only while a fraction is genuinely known: an
               indeterminate stage rendered as a full bar reads as finished. -->
          <div class="fill" style:width={`${Math.round((fraction ?? 0) * 100)}%`}></div>
        </div>
        <span class="stage">{stage}</span>
      </div>
    {/if}
  {/if}

  <Row
    title="Try it"
    description="Reads one sentence in whichever voice is set above."
  >
    {#snippet control()}
      <Button label="Read a sample" onclick={sample} />
    {/snippet}
  </Row>

  {#if said}
    <p class="said">{said}</p>
  {/if}
</Section>

<style>
  .progress {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
  }

  .bar {
    flex: 1;
    height: 4px;
    border-radius: var(--radius-1);
    background: var(--sunken);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: rgb(var(--accent-rgb));
    transition: width 120ms linear;
  }

  .stage {
    color: var(--ink-dim);
    font-size: var(--text-small);
  }

  .said {
    color: var(--ink-dim);
    font-size: var(--text-small);
    margin: 0;
    padding: var(--space-2) 0 0;
  }
</style>
