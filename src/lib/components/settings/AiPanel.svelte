<script lang="ts">
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import Select from "./Select.svelte";
  import TextField from "./TextField.svelte";
  import {
    aiKnown,
    aiModels,
    type AiModel,
    type AiProvider,
  } from "$lib/ai";
  import type { Preferences } from "$lib/settings";

  interface Props {
    /** Not `$bindable`: nothing here reassigns it, only writes its fields. */
    prefs: Preferences;
    commit: () => void;
  }

  let { prefs, commit }: Props = $props();

  /** The services Sill knows how to reach, for the Add row. */
  let known = $state<AiProvider[]>([]);

  /** Which provider is open in the editor, by id. Empty means none. */
  let editing = $state("");

  /** The models the open provider offers, once asked. */
  let models = $state<AiModel[]>([]);
  let loadingModels = $state(false);

  /** Why the models could not be listed, if they could not. */
  let modelTrouble = $state("");

  const providers = $derived(prefs.ai.providers);

  /**
   * Which one answers.
   *
   * Falls back to the only one, matching what Rust does when nothing is
   * chosen: somebody with exactly one set up means that one.
   */
  const answering = $derived(
    prefs.ai.provider || (providers.length === 1 ? providers[0].id : ""),
  );

  const open = $derived(providers.find((one) => one.id === editing) ?? null);

  $effect(() => {
    void aiKnown().then((list) => (known = list));
  });

  /** The ones not set up yet, so Add offers each service once. */
  const addable = $derived(
    known.filter((one) => !providers.some((have) => have.id === one.id)),
  );

  /**
   * Writes the fields of the settings object the page owns, then saves.
   *
   * Not a new object handed back: the page's `commit` takes no argument and
   * snapshots what it already holds, so anything returned instead of written
   * is silently dropped. That is a mistake the type system does not catch,
   * because a zero-argument function satisfies a one-argument type.
   */
  function save() {
    commit();
  }

  function choose(id: string) {
    prefs.ai.provider = id;
    save();
  }

  function add(one: AiProvider) {
    // The note is the settings window's own prose about the service. It
    // explains the row it is read from and has no business in the file.
    const { note: _note, ...provider } = one as AiProvider & { note?: string };

    // Read before the list is written to, because `providers` derives from it.
    const first = providers.length === 0;

    prefs.ai.providers = [...providers, { ...provider }];

    // Chosen straight away when it is the first, because somebody who adds
    // exactly one provider means that one and should not have to say so.
    if (first) prefs.ai.provider = one.id;
    save();

    editing = one.id;
    void loadModels({ ...provider });
  }

  function change(id: string, patch: Partial<AiProvider>) {
    prefs.ai.providers = providers.map((one) =>
      one.id === id ? { ...one, ...patch } : one,
    );
    save();
  }

  function remove(id: string) {
    prefs.ai.providers = providers.filter((one) => one.id !== id);
    // A choice pointing at something removed would answer with nothing.
    if (prefs.ai.provider === id) prefs.ai.provider = "";
    save();

    if (editing === id) editing = "";
  }

  /**
   * Asks the provider which models it has.
   *
   * A model id is a string, and one character wrong is a request that fails
   * with a message about a model nobody meant to ask for. When the list cannot
   * be had, the field below stays a text box, which still works.
   */
  async function loadModels(one: AiProvider) {
    models = [];
    modelTrouble = "";
    loadingModels = true;

    try {
      models = await aiModels(one);

      if (models.length === 0) {
        modelTrouble = "That one did not say what it has.";
      } else if (!one.model) {
        // Nothing shipped a name for this one, because what it offers differs
        // per machine and per account. Taking the first is the whole setup
        // finished rather than a picker left blank for somebody to notice.
        change(one.id, { model: models[0].id });
      }
    } catch (err) {
      modelTrouble = `${err}`;
    } finally {
      loadingModels = false;
    }
  }

  function edit(one: AiProvider) {
    editing = editing === one.id ? "" : one.id;
    if (editing) void loadModels(one);
  }

  /**
   * What a row says underneath its name.
   *
   * The model as it is stored, not as the open editor happens to label it:
   * there is one list of models and it belongs to whichever provider is open,
   * so a row reading its label showed the wrong thing as soon as a second
   * provider was opened.
   */
  function describe(one: AiProvider): string {
    if (one.wire === "claudeCode") {
      return one.model ? `Your subscription, ${one.model}` : "Your subscription";
    }

    return one.model || "No model chosen yet";
  }

  /**
   * What the picker offers.
   *
   * A model that is set but not on offer is still listed, at the top. A
   * picker showing nothing beside a stored value reads as the setting having
   * been lost, when what has actually happened is that the model was renamed
   * or uninstalled.
   */
  const choices = $derived.by(() => {
    const listed = models.map((model) => ({ value: model.id, label: model.label }));
    const chosen = open?.model ?? "";

    if (!chosen || listed.some((one) => one.value === chosen)) return listed;
    return [{ value: chosen, label: chosen }, ...listed];
  });

  /** Whether what is set is something the service did not list. */
  const strayModel = $derived(
    !!open?.model && models.length > 0 && !models.some((one) => one.id === open.model),
  );
</script>

<Section
  label="Who answers"
  description="Press Tab in the launcher to ask whatever you have typed. One of these answers; the rest stay set up and unused."
>
  {#each providers as one (one.id)}
    <Row title={one.name} description={describe(one)}>
      {#snippet control()}
        <div class="controls">
          {#if answering === one.id}
            <span class="answering">Answering</span>
          {:else}
            <Button label="Use this" onclick={() => choose(one.id)} />
          {/if}
          <Button
            label={editing === one.id ? "Done" : "Set up"}
            onclick={() => edit(one)}
          />
        </div>
      {/snippet}
    </Row>

    {#if editing === one.id && open}
      <div class="editor">
        {#if open.wire !== "claudeCode"}
          <div class="field">
            <span class="what">Address</span>
            <TextField
              value={open.baseUrl}
              oninput={(next) => change(one.id, { baseUrl: next })}
              placeholder="https://api.example.com/v1"
              ariaLabel="Address"
              full
              mono
            />
            <!-- The rule, said once, where somebody would otherwise meet it as
                 a refusal. -->
            <span class="hint">
              Plain http only to this machine or this network. Anywhere else
              needs https, or your key travels in the clear.
            </span>
          </div>
        {/if}

        {#if open.wire !== "claudeCode"}
          <div class="field">
            <span class="what">Key</span>
            <TextField
              value={open.apiKey}
              oninput={(next) => change(one.id, { apiKey: next })}
              placeholder="Paste it here"
              ariaLabel="Key"
              full
              mono
              secret
            />
            <span class="hint">
              Encrypted with your Windows account before it is written to disk.
            </span>
          </div>
        {/if}

        <div class="field">
          <span class="what">Model</span>
          {#if models.length}
            <Select
              value={open.model}
              options={choices}
              onchange={(next) => change(one.id, { model: next })}
              ariaLabel="Model"
              full
            />
          {:else}
            <TextField
              value={open.model}
              oninput={(next) => change(one.id, { model: next })}
              placeholder="Name the model"
              ariaLabel="Model"
              full
              mono
            />
          {/if}

          <span class="hint">
            {#if loadingModels}
              Asking what it has…
            {:else if modelTrouble}
              {modelTrouble} Type the name instead.
            {:else if strayModel}
              {open.model} was not in the list it gave. Keep it if you know it
              works, or pick one of the {models.length} it named.
            {:else if models.length}
              {models.length} to choose from.
            {/if}
          </span>
        </div>

        <div class="editor-actions">
          <Button label="Refresh models" onclick={() => loadModels(open)} />
          <span class="spacer"></span>
          <Button label="Remove" tone="danger" onclick={() => remove(one.id)} />
        </div>
      </div>
    {/if}
  {/each}

  {#if providers.length === 0}
    <p class="empty">
      Nothing set up yet. Add one below. Claude Code uses the subscription you
      already have; a model running on this machine costs nothing and sends
      nothing anywhere.
    </p>
  {/if}
</Section>

{#if addable.length}
  <Section label="Add" description="Each of these is set up separately, and you can keep several.">
    {#each addable as one (one.id)}
      <Row title={one.name} description={one.note}>
        {#snippet control()}
          <Button label="Add" onclick={() => add(one)} />
        {/snippet}
      </Row>
    {/each}
  </Section>
{/if}

<style>
  .controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  /*
   * Which one answers, said rather than drawn as a control.
   *
   * A selected radio beside an unselected button is two things that look
   * clickable and only one that is. A word is unambiguous.
   */
  .answering {
    color: var(--accent);
    font-size: var(--text-meta);
    white-space: nowrap;
  }

  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: var(--space-2) 0 var(--space-4);
    padding: var(--space-4);
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    box-shadow: var(--bevel-tile);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .what {
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .hint {
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: 1.5;
  }

  .editor-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .spacer {
    flex: 1;
  }

  .empty {
    margin: 0;
    max-width: 62ch;
    padding: var(--space-4) 0;
    color: var(--text-2);
    font-size: var(--text-meta);
    line-height: 1.65;
  }
</style>
