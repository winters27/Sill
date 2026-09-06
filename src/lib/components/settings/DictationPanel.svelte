<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Segmented from "./Segmented.svelte";
  import Button from "./Button.svelte";
  import Select from "./Select.svelte";
  import ServerStatus from "./ServerStatus.svelte";
  import HistoryPanel from "./HistoryPanel.svelte";
  import DictationStats from "./DictationStats.svelte";
  import MicrophoneOrder from "./MicrophoneOrder.svelte";
  import Toggle from "../Toggle.svelte";
  import {
    formatBytes,
    getLocalDictationStatus,
    listAudioInputDevices,
    listWhisperModels,
    installLocalDictation,
    removeWhisperModel,
    stopWhisperServer,
    dictationHookState,
    resetDictationHook,
    LANGUAGES,
    type HookState,
    type AudioInputDevice,
    type LocalSetupStatus,
    type OutputMode,
    type SetupProgress,
    type WhisperModel,
  } from "$lib/dictation";
  import { acceleratorFrom, type Preferences } from "$lib/settings";

  interface Props {
    /** Not `$bindable`: nothing here reassigns it, only writes its fields,
     *  and those propagate through the same reactive object either way. */
    prefs: Preferences;
    commit: () => void;
  }

  let { prefs, commit }: Props = $props();

  /** Longer than the clipboard's, because this is a record of your own use
   *  rather than a pile of one-time codes, and a year of it is worth reading
   *  back. "Forever" is the default. */
  const RETENTION = [
    { value: "7", label: "1 week" },
    { value: "30", label: "1 month" },
    { value: "90", label: "3 months" },
    { value: "365", label: "1 year" },
    { value: "0", label: "Forever" },
  ];

  let devices = $state<AudioInputDevice[]>([]);
  let models = $state<WhisperModel[]>([]);
  let status = $state<LocalSetupStatus | null>(null);
  let recording = $state(false);
  let installing = $state(false);
  let hook = $state<HookState | null>(null);

  let stage = $state("");
  let progress = $state<number | null>(null);

  const isLocal = $derived(prefs.dictation.providerId === "local");

  /**
   * What the hook's counters mean, in the order they rule things out.
   *
   * Each step answers one question and only reaches the next if the answer
   * was yes: is it installed, does it see any keys, does it see this key,
   * does this key arrive with the right modifiers. Whichever step fails is
   * the fault, and each failure has a different cause and a different fix.
   */
  function diagnosis(h: HookState, stuck: boolean): string {
    if (stuck) {
      return "The hook thinks a dictation is running but none is. In that state the trigger key is swallowed and does nothing, so reset it.";
    }
    if (!h.armed) {
      return "The hook is not installed, so the trigger cannot fire. The log in Advanced says why.";
    }
    if (h.keysSeen === 0) {
      return "Installed, but it has been handed no keys at all. A keyboard hook does not see input while a window running as administrator has focus, so try typing somewhere ordinary like Notepad.";
    }
    if (h.chordKeySeen === 0) {
      return `Installed and seeing keys, ${h.keysSeen.toLocaleString()} so far, but never the trigger key itself. Something ahead of Sill is taking that combination before it arrives: another launcher, a keyboard utility, or the app you are pressing it in. Try a different key.`;
    }
    if (h.triggersSeen === 0) {
      const held = h.lastModifiers ?? "nothing";
      return `The trigger key arrives, but with ${held} held rather than the combination below. Rebind it to match, or press it again with exactly those modifiers.`;
    }
    return `Working. ${h.triggersSeen.toLocaleString()} ${h.triggersSeen === 1 ? "trigger" : "triggers"} out of ${h.keysSeen.toLocaleString()} keys seen.`;
  }

  /**
   * Resident working set per model, mirrored from `dictation/assets.rs`.
   *
   * Shown beside the download size because they are wildly different numbers
   * and the second one is the one you live with: a 465 MB model sits at
   * about 650 MB for as long as the server is up.
   */
  const MEMORY: Record<string, number> = {
    "tiny.en": 171_966_464,
    "base.en": 266_338_304,
    "small.en": 680_525_824,
    "medium.en": 1_875_902_464,
  };

  /**
   * Turns one setup event into the line and the bar the status card shows.
   *
   * The bar is only drawn while a fraction is genuinely known: an
   * indeterminate stage rendering as a full bar reads as finished.
   */
  function apply(event: SetupProgress) {
    if (event === "engine") {
      stage = "Fetching whisper.cpp";
      progress = null;
    } else if (event === "verifying") {
      stage = "Checking the download";
      progress = null;
    } else if (event === "starting") {
      stage = "Loading the model into memory";
      progress = null;
    } else if (event === "ready") {
      stage = "";
      progress = null;
    } else if ("engineDownload" in event) {
      const { bytesDownloaded, totalBytes } = event.engineDownload;
      stage = `Fetching whisper.cpp, ${formatBytes(bytesDownloaded)} of ${formatBytes(totalBytes)}`;
      progress = fraction(bytesDownloaded, totalBytes);
    } else if ("model" in event) {
      const { bytesDownloaded, totalBytes } = event.model;
      stage = `Downloading ${prefs.dictation.modelId}, ${formatBytes(bytesDownloaded)} of ${formatBytes(totalBytes)}`;
      progress = fraction(bytesDownloaded, totalBytes);
    } else {
      stage = `Setup failed: ${event.failed.error}`;
      progress = null;
    }
  }

  function fraction(done: number, total: number): number | null {
    return total > 0 ? Math.min(1, done / total) : null;
  }

  async function refresh() {
    try {
      hook = await dictationHookState();
    } catch {
      // The panel reads perfectly well without it.
    }

    try {
      [devices, models, status] = await Promise.all([
        listAudioInputDevices(),
        listWhisperModels(),
        getLocalDictationStatus(),
      ]);
    } catch (err) {
      stage = `Could not read the dictation setup: ${err}`;
    }
  }

  async function install() {
    installing = true;
    stage = "Starting";
    progress = null;
    try {
      await installLocalDictation(prefs.dictation.modelId);
      stage = "";
      await refresh();
    } catch (err) {
      stage = `Setup failed: ${err}`;
    } finally {
      installing = false;
      progress = null;
    }
  }

  async function remove(id: string) {
    try {
      await removeWhisperModel(id);
      await refresh();
    } catch (err) {
      stage = `Could not remove the model: ${err}`;
    }
  }

  async function stopServer() {
    await stopWhisperServer();
    await refresh();
  }

  /**
   * The trigger is recorded as a modifier plus a key, because a low-level
   * keyboard hook binds the two separately: it watches the modifier's real
   * down and up edges, which is what makes hold-to-talk possible at all.
   */
  function onRecord(event: KeyboardEvent) {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      recording = false;
      return;
    }

    const accelerator = acceleratorFrom(event);
    if (!accelerator) return;

    const parts = accelerator.split("+");
    const key = parts.pop() ?? "";
    if (parts.length === 0) return;

    prefs.dictation.shortcutModifier = parts.join("+");
    prefs.dictation.shortcutKey = key;
    recording = false;
    commit();
  }

  onMount(() => {
    let unlisten: UnlistenFn | undefined;

    // Polled rather than pushed: uptime, idle time and the working set all
    // move on their own, and a status card that claims to be live has to
    // actually change while you watch it. Two seconds is slow enough to be
    // free and fast enough that the numbers never look frozen.
    const timer = setInterval(() => {
      if (!installing) void refresh();
    }, 2000);

    (async () => {
      await refresh();
      unlisten = await listen<SetupProgress>("dictation:setup", ({ payload }) => apply(payload));
    })();

    return () => {
      clearInterval(timer);
      unlisten?.();
    };
  });
</script>

<svelte:window onkeydown={onRecord} />


<Section
  label="Trigger"
  description="Press the combination to start listening, then the finish key to transcribe. The transcript arrives where the cursor already is."
>
  <Row
    title="Dictation"
    description="Installs a low-level keyboard hook, which is what gives the trigger clean press and release edges rather than the auto-repeat a registered hotkey delivers."
  >
    {#snippet control()}
      <Toggle bind:checked={prefs.dictation.enabled} onchange={commit} label="Dictation" />
    {/snippet}
  </Row>

  {#if prefs.dictation.enabled && hook}
    {@const h = hook}
    {@const stuck = h.listening && !h.recording}
    <!-- not a setting: a reading of whether the trigger is working, not a control -->
    <!-- not a setting: a reading of whether the trigger is working, not a control -->
    <Row
      title="Trigger status"
      description={diagnosis(h, stuck)}
    >
      {#snippet control()}
        <div class="hook-state">
          <span
            class="lamp"
            class:ok={h.armed && !stuck && h.triggersSeen > 0}
            class:warn={h.armed && !stuck && h.triggersSeen === 0}
            class:bad={!h.armed || stuck}
          ></span>
          <span class="hook-text">
            {!h.armed
              ? "Not installed"
              : stuck
                ? "Stuck"
                : h.recording
                  ? "Recording"
                  : h.keysSeen === 0
                    ? "No input"
                    : h.chordKeySeen === 0
                      ? "Key never arrives"
                      : h.triggersSeen === 0
                        ? "Modifiers differ"
                        : "Idle"}
          </span>
          {#if stuck}
            <Button
              label="Reset"
              onclick={async () => {
                await resetDictationHook();
                await refresh();
              }}
            />
          {/if}
        </div>
      {/snippet}
    </Row>
  {/if}

  <Row
    title="Start dictating"
    description="Press the combination you want, or Escape to keep the current one."
    disabled={!prefs.dictation.enabled}
  >
    {#snippet control()}
      <button class="recorder" class:recording onclick={() => (recording = !recording)}>
        {recording
          ? "Press a combination"
          : `${prefs.dictation.shortcutModifier.split("+").join(" ")} ${prefs.dictation.shortcutKey}`}
      </button>
    {/snippet}
  </Row>
</Section>

<Section label="Output">
  <Row
    title="What happens to the transcript"
    description="Pasting types it into whatever has focus. Clipboard only is the safe choice for password fields and terminals, where a stray paste is destructive. History only writes it down and nothing else."
  >
    {#snippet control()}
      <Segmented
        label="What happens to the transcript"
        value={prefs.dictation.outputMode}
        options={[
          { value: "paste", label: "Paste" },
          { value: "clipboard", label: "Clipboard" },
          { value: "none", label: "History only" },
        ]}
        onchange={(next) => {
          prefs.dictation.outputMode = next as OutputMode;
          commit();
        }}
      />
    {/snippet}
  </Row>
</Section>

<Section label="Microphone">
  <Row
    title="Use the system default"
    description="Follows whatever Windows Sound settings call the default input. Turn it off to rank your microphones instead, and Sill uses the first one connected."
  >
    {#snippet control()}
      <Toggle
        bind:checked={prefs.dictation.useSystemMicrophone}
        onchange={commit}
        label="Use the system default"
      />
    {/snippet}
  </Row>

  {#if prefs.dictation.useSystemMicrophone}
    <!-- not a setting: a reading of the microphone in use, not a control -->
    <Row
      title="Current input"
      description="A microphone blocked by Windows privacy settings returns silence rather than an error, so a flat waveform in the panel is the sign."
    >
      {#snippet control()}
        <span class="fact">
          {devices.find((device) => device.isDefault)?.name ?? "None found"}
        </span>
      {/snippet}
    </Row>
  {:else}
    <Row title="Priority order">
      {#snippet children()}
        <MicrophoneOrder
          {devices}
          priority={prefs.dictation.devicePriority}
          onchange={(next) => {
            prefs.dictation.devicePriority = next;
            commit();
          }}
        />
      {/snippet}
    </Row>
  {/if}

  <Row
    title="Mute everything else while recording"
    description="Audio playing through speakers is picked up by the microphone and transcribed as words, which is the most common way a dictation comes back with something nobody said."
  >
    {#snippet control()}
      <Toggle
        bind:checked={prefs.dictation.muteWhileRecording}
        onchange={commit}
        label="Mute while recording"
      />
    {/snippet}
  </Row>
</Section>

<Section label="Listening">
  <Row
    title="Language"
    description="Auto detects per dictation. Pinning one is faster and stops a strong accent being read as a neighbouring language."
  >
    {#snippet control()}
      <Select
        value={prefs.dictation.language ?? ""}
        options={LANGUAGES.map((option) => ({
          value: option.code ?? "",
          label: option.name,
        }))}
        onchange={(next) => {
          prefs.dictation.language = next === "" ? null : next;
          commit();
        }}
        ariaLabel="Language"
        steady
      />
    {/snippet}
  </Row>

  <Row
    title="Finish and cancel keys"
    description="Pressed while a dictation is running. Both are swallowed, so finishing never also submits the form behind it."
  >
    {#snippet control()}
      <Segmented
        label="Finish and cancel keys"
        value="{prefs.dictation.finishKey}/{prefs.dictation.cancelKey}"
        options={[
          { value: "Enter/Escape", label: "Enter / Esc" },
          { value: "Space/Escape", label: "Space / Esc" },
          { value: "Tab/Escape", label: "Tab / Esc" },
        ]}
        onchange={(next) => {
          const [finish, cancel] = next.split("/");
          prefs.dictation.finishKey = finish;
          prefs.dictation.cancelKey = cancel;
          commit();
        }}
      />
    {/snippet}
  </Row>

  <Row
    title="Ask before discarding"
    description="The cancel key has to be pressed twice. A dialog would need focus, and taking focus mid-dictation is what the panel exists to avoid."
  >
    {#snippet control()}
      <Toggle
        bind:checked={prefs.dictation.confirmCancel}
        onchange={commit}
        label="Ask before discarding"
      />
    {/snippet}
  </Row>

  <Row
    title="Start and stop cues"
    description="A short tone at each end, so you know it is listening without looking."
  >
    {#snippet control()}
      <Toggle bind:checked={prefs.dictation.soundEnabled} onchange={commit} label="Sound cues" />
    {/snippet}
  </Row>

</Section>

<Section
  label="History"
  description="Every transcript is kept on this machine so it can be found again. It is what the statistics below are counted from, and nothing is sent anywhere."
>
  <Row
    title="Keep a history"
    description="Every transcript is kept so it can be found again, and it is what the statistics below are counted from. Nothing is sent anywhere."
  >
    {#snippet control()}
      <Toggle bind:checked={prefs.dictation.keepHistory} onchange={commit} label="Keep a history" />
    {/snippet}
  </Row>

  {#if prefs.dictation.keepHistory}
    <Row
      title="Keep transcripts for"
      description="Anything older is dropped the next time you dictate. Kept forever unless you say otherwise, because nothing lands here that you did not speak into it, and the statistics below read less well the more of it is thrown away."
    >
      {#snippet control()}
        <Segmented
          label="Keep transcripts for"
          value={String(prefs.dictation.retainDays)}
          options={RETENTION}
          onchange={(next) => {
            prefs.dictation.retainDays = Number(next);
            commit();
          }}
        />
      {/snippet}
    </Row>
  {/if}
</Section>

<Section
  label="Engine"
  description="Local runs whisper.cpp on this machine and nothing leaves it. The others need an API key and are faster."
>
  <Row title="Backend">
    {#snippet control()}
      <Segmented
        label="Backend"
        value={prefs.dictation.providerId}
        options={[
          { value: "local", label: "Local" },
          { value: "openai", label: "OpenAI" },
          { value: "groq", label: "Groq" },
        ]}
        onchange={(next) => {
          prefs.dictation.providerId = next;
          commit();
        }}
      />
    {/snippet}
  </Row>

  {#if !isLocal}
    <Row title="API key" description="Stored with the rest of Sill's settings.">
      {#snippet control()}
        <input
          class="secret"
          type="password"
          spellcheck="false"
          autocomplete="off"
          placeholder="sk-…"
          value={prefs.dictation.provider.apiKey ?? ""}
          onchange={(e) => {
            const value = e.currentTarget.value.trim();
            prefs.dictation.provider.apiKey = value === "" ? null : value;
            commit();
          }}
        />
      {/snippet}
    </Row>
  {/if}

  <Row
    title="Custom endpoint"
    description={isLocal
      ? "Point at a whisper server on another machine. Leave it empty to run one here."
      : "Overrides the provider's own address, for a gateway or a self-hosted equivalent."}
  >
    {#snippet control()}
      <input
        class="endpoint"
        spellcheck="false"
        autocomplete="off"
        placeholder={isLocal ? "http://127.0.0.1:8791" : "https://…"}
        value={prefs.dictation.provider.baseUrl ?? ""}
        onchange={(e) => {
          const value = e.currentTarget.value.trim();
          prefs.dictation.provider.baseUrl = value === "" ? null : value;
          commit();
        }}
      />
    {/snippet}
  </Row>

  {#if isLocal && !prefs.dictation.provider.baseUrl}
    <!--
      The model rows sit with the backend that uses them rather than in a
      section of their own three sections down, so the reader chooses the
      engine and its size in one place.
    -->
    {#each models as model (model.id)}
      <Row
        title={model.label}
        description="{formatBytes(model.sizeBytes)} to download · about {formatBytes(
          MEMORY[model.id] ?? 0,
        )} in memory{model.installed ? ' · downloaded' : ''}. Bigger is more accurate and slower. Changing this restarts the server on the next dictation."
      >
        {#snippet control()}
          <div class="model-actions">
            {#if prefs.dictation.modelId === model.id}
              <span class="chosen">Selected</span>
            {:else}
              <Button
                label="Use"
                onclick={() => {
                  prefs.dictation.modelId = model.id;
                  commit();
                  void refresh();
                }}
              />
            {/if}
            {#if model.installed}
              <Button label="Remove" tone="danger" onclick={() => void remove(model.id)} />
            {/if}
          </div>
        {/snippet}
      </Row>
    {/each}
  {/if}
</Section>

<Section
  label="Personalization"
  description="All three are sent as the transcription prompt, which is what a Whisper-compatible model conditions its decoding on. They bias what it hears; they do not rewrite what it heard."
>
  <Row
    title="Custom instructions"
    description="Standing guidance that leads every prompt, such as a preferred spelling or a house style for numbers."
  >
    {#snippet children()}
      <textarea
        rows="3"
        spellcheck="false"
        placeholder="e.g. Prefer British spelling. Write numbers as digits."
        value={prefs.dictation.customInstructions}
        onchange={(e) => {
          prefs.dictation.customInstructions = e.currentTarget.value;
          commit();
        }}
      ></textarea>
    {/snippet}
  </Row>

  <Row
    title="Use the frontmost application"
    description="Naming the application in the prompt makes its jargon more likely: in an editor, const and struct beat constant and struck. The name is also filed against the transcript in the history, and nothing is sent anywhere it would not have gone."
  >
    {#snippet control()}
      <Toggle
        bind:checked={prefs.dictation.appContext}
        onchange={commit}
        label="Use the frontmost application"
      />
    {/snippet}
  </Row>

  <Row
    title="Vocabulary"
    description="Names and jargon the model reliably mangles. Placed last in the prompt, closest to the speech, which is where it biases most."
  >
    {#snippet children()}
      <textarea
        rows="3"
        spellcheck="false"
        placeholder="Proper nouns, product names, anything it gets wrong"
        value={prefs.dictation.vocabulary}
        onchange={(e) => {
          prefs.dictation.vocabulary = e.currentTarget.value;
          commit();
        }}
      ></textarea>
    {/snippet}
  </Row>
</Section>

{#if isLocal && !prefs.dictation.provider.baseUrl}
  <Section
    label="Local server"
    description="whisper.cpp runs as a resident server, so the model loads once instead of on every dictation. Inference cost is flat with clip length: a twenty second dictation costs the same as a two second one."
    bare
  >
    <ServerStatus
      {status}
      {installing}
      {stage}
      {progress}
      oninstall={install}
      onstop={() => void stopServer()}
    />
  </Section>
{/if}

{#if prefs.dictation.keepHistory}
  <!-- Last, not first: a first-time reader meets the switch that turns
       dictation on, not a board of words per minute. -->
  <Section label="Dictation statistics" description="Counted from the history." bare>
    <DictationStats />
  </Section>

  <HistoryPanel />
{/if}

<style>
  .recorder {
    min-width: 150px;
    padding: var(--space-1) var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    box-shadow: var(--bevel-tile);
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    letter-spacing: 0.04em;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .recorder:hover {
    background: var(--fill-3);
  }

  .recorder.recording {
    background: var(--hairline-strong);
    color: var(--accent-bright);
  }

  .hook-state {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  /* A lamp, because the useful reading is at a glance. Three states rather
     than two: red is the hook being absent or stuck, amber is it running but
     never having matched the trigger, green is it having matched. Amber
     matters because "installed" and "working" are not the same thing, and
     the gap between them is where every trigger fault lives. */
  .lamp {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
    background: var(--text-3);
  }

  .lamp.ok {
    background: var(--accent-bright);
  }

  .lamp.warn {
    background: var(--warning);
  }

  .lamp.bad {
    background: var(--danger);
  }

  .hook-text {
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  input,
  textarea {
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    outline: none;
    user-select: text;
    transition: box-shadow var(--motion-state) var(--ease);
  }

  input:focus,
  textarea:focus {
    box-shadow: var(--ring-strong);
  }

  input {
    width: 250px;
    font-family: var(--font-mono);
    font-size: var(--text-meta);
  }

  textarea {
    width: 100%;
    max-width: 520px;
    resize: vertical;
    line-height: 1.5;
  }

  textarea::placeholder,
  input::placeholder {
    color: var(--text-3);
  }

  .model-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .chosen {
    font-size: var(--text-meta);
    color: var(--accent);
  }
</style>