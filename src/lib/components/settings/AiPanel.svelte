<script lang="ts">
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Toggle from "../Toggle.svelte";
  import Button from "./Button.svelte";
  import Select from "./Select.svelte";
  import TextField from "./TextField.svelte";
  import AiMark from "./AiMark.svelte";
  import Instead from "../Instead.svelte";
  import { drawer } from "$lib/motion";
  import {
    aiHello,
    aiKnown,
    aiModels,
    aiNamed,
    type AiHelloHere,
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

  /** The services Sill knows how to reach, for the Add list. */
  let known = $state<AiProvider[]>([]);

  /**
   * Whether this PC can actually run the Windows Hello gate.
   *
   * Asked because the switch below would otherwise claim a protection that is
   * not running. Most machines have nothing enrolled, and somebody reading a
   * switch that says on is entitled to believe it means something.
   */
  let hello = $state<AiHelloHere | null>(null);

  /**
   * What the Hello row says under its title.
   *
   * Three sentences and never a promise the machine cannot keep: what it does,
   * what happens when it cannot, and, when this is that machine, why.
   */
  const helloDescription = $derived(
    hello && !hello.ready
      ? `Not available here: ${hello.why ?? "Windows Hello is not set up on this PC"}. Sill still stops and asks before running a command or writing a file, and the card says a keypress is all it got.`
      : "A face, a fingerprint or a Hello PIN, rather than a keypress anything running as you could send. Machines without Windows Hello fall back to the same card as everything else.",
  );

  /** Which provider is open for setup, by id. Empty means none. */
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

  // Once, when the panel opens. Not a subscription: enrolling a fingerprint
  // means going to Windows Settings and coming back, which reopens this.
  $effect(() => {
    void aiHello().then((here) => (hello = here));
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
  /**
   * Which ask is the current one.
   *
   * Typing a key sends several, and they do not come back in order: a fast
   * refusal of half a key can land after the slow success of the whole one and
   * replace a good list with an error. Only the newest is allowed to write.
   */
  let asked = 0;

  async function loadModels(one: AiProvider) {
    const mine = ++asked;

    models = [];
    modelTrouble = "";
    loadingModels = true;

    try {
      const found = await aiModels(one);
      if (mine !== asked) return;

      models = found;

      if (found.length === 0) {
        modelTrouble = "That one did not say what it has, so type the name.";
      } else if (!one.model) {
        // Nothing shipped a name for this one, because what it offers differs
        // per machine and per account. Taking the first is the whole setup
        // finished rather than a picker left blank for somebody to notice.
        change(one.id, { model: found[0].id });
      }
    } catch (err) {
      if (mine !== asked) return;
      modelTrouble = `${err}`;
    } finally {
      if (mine === asked) loadingModels = false;
    }
  }

  /**
   * How long to let an address or a key settle before asking again.
   *
   * A pasted key arrives in one event and a typed one arrives in forty, and
   * asking per character is forty failed authentications against somebody's
   * account. This is long enough to cover typing and short enough that a paste
   * feels immediate.
   */
  const SETTLE = 600;

  /** The provider and credentials the last ask was made with. */
  let askedAbout = "";
  /** Which provider was open last, so opening a different one is not delayed. */
  let lastOpen = "";
  let settling: ReturnType<typeof setTimeout> | undefined;

  /*
   * Asks again whenever the answer could have changed.
   *
   * This is the difference between a picker and a text box. A service with a
   * key is added before the key exists, so the first ask is always refused;
   * without this, pasting the key changed nothing and the only way to a model
   * was to go and look up its id, which is exactly the work the picker exists
   * to remove.
   *
   * Only the address and the key are watched. The model is not: choosing one
   * writes the provider back, and re-asking on that would throw away the list
   * the choice was just made from.
   */
  $effect(() => {
    const one = open;

    if (!one) {
      askedAbout = "";
      lastOpen = "";
      return;
    }

    const signature = [one.id, one.wire, one.baseUrl, one.apiKey].join("\u0000");
    if (signature === askedAbout) return;
    askedAbout = signature;

    // Opening a different provider is not something to wait out. Only a change
    // to one already open is, because that is somebody still typing.
    const wait = one.id === lastOpen ? SETTLE : 0;
    lastOpen = one.id;

    const snapshot = { ...one };
    clearTimeout(settling);
    settling = setTimeout(() => void loadModels(snapshot), wait);

    return () => clearTimeout(settling);
  });

  function edit(one: AiProvider) {
    editing = editing === one.id ? "" : one.id;
  }

  /**
   * What a card says underneath its name.
   *
   * Either the model it will answer with or the one thing left to do. It used
   * to say "No model chosen yet", which is the empty field restated in more
   * words: a line of text that costs a row of height and tells somebody
   * something they can already see.
   *
   * The model as it is stored, not as the open editor happens to label it:
   * there is one list of models and it belongs to whichever provider is open,
   * so a card reading its label showed the wrong thing as soon as a second
   * provider was opened.
   */
  /**
   * What each card's model is called, keyed by provider id.
   *
   * Asked for rather than worked out here, so a card and the launcher's chip
   * call the same model the same thing. Until the answer arrives a card shows
   * the stored id, which is right, just longer.
   */
  let named = $state<Record<string, string>>({});

  $effect(() => {
    const asking = providers.map((one) => ({ ...one }));
    if (asking.length === 0) {
      named = {};
      return;
    }

    void aiNamed(asking).then((labels) => {
      named = Object.fromEntries(asking.map((one, at) => [one.id, labels[at] ?? one.model]));
    });
  });

  function status(one: AiProvider): string {
    const model = named[one.id] ?? one.model;

    if (one.wire === "claudeCode") {
      return model ? `Your subscription, ${model}` : "Your subscription";
    }

    return model || "Needs a model";
  }

  /**
   * What this service wants from you, for the top of its setup.
   *
   * Read from the known table rather than from the saved provider, because
   * `add` deliberately strips it: it is the settings window's prose about a
   * service, and it has no business in somebody's config file. That also means
   * it still shows for a provider added long ago.
   */
  function noteFor(id: string): string {
    return known.find((one) => one.id === id)?.note ?? "";
  }

  /** Whether that status is a job rather than a fact, so it can be marked. */
  const unfinished = (one: AiProvider) => one.wire !== "claudeCode" && !one.model;

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

<!--
  Cards, not settings rows, and `bare` so they are not cards inside a card.

  The setup form opens INSIDE the card it belongs to rather than as a second
  card below it. Nesting one filled, bevelled box in another is what made this
  read as a dialog stacked on a dialog when the only thing happening is that a
  row got taller.
-->
<Section
  bare
  label="Who answers"
  description="Press Tab in the launcher to ask whatever you have typed. One of these answers; the rest stay set up and unused."
>
  {#if providers.length === 0}
    <Instead
      tone="empty"
      inline
      headline="Nothing set up yet"
      hint="Claude Code uses the subscription you already have, and a model running on this machine costs nothing and sends nothing anywhere."
    />
  {:else}
    <div class="stack" role="radiogroup" aria-label="Who answers">
      {#each providers as one (one.id)}
        <div class="provider" class:on={answering === one.id} class:open={editing === one.id}>
          <div class="head">
            <!--
              The card is the choice. A row carrying a "Use this" button beside
              a "Set up" button is two identical grey rectangles per provider
              and fourteen down the panel, none of which says which one is
              picked without reading it.
            -->
            <button
              type="button"
              class="pick"
              role="radio"
              aria-checked={answering === one.id}
              onclick={() => choose(one.id)}
            >
              <AiMark name={one.id} />

              <span class="text">
                <span class="name">{one.name}</span>
                <span class="status" class:todo={unfinished(one)}>{status(one)}</span>
              </span>

              {#if answering === one.id}
                <span class="answering">Answering</span>
              {/if}
            </button>

            <button
              type="button"
              class="disclose"
              aria-expanded={editing === one.id}
              aria-label="Set up {one.name}"
              onclick={() => edit(one)}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                   stroke="currentColor" stroke-width="2"
                   stroke-linecap="round" stroke-linejoin="round">
                <path d="m6 9 6 6 6-6" />
              </svg>
            </button>
          </div>

          {#if editing === one.id && open}
            <div class="setup" in:drawer out:drawer={{ out: true }}>
              {#if noteFor(one.id)}
                <p class="intro">{noteFor(one.id)}</p>
              {/if}

              {#if open.wire !== "claudeCode"}
                <div class="field">
                  <span class="what">Address</span>
                  <div class="control">
                    <TextField
                      value={open.baseUrl}
                      oninput={(next) => change(one.id, { baseUrl: next })}
                      placeholder="https://api.example.com/v1"
                      ariaLabel="Address"
                      full
                      mono
                    />
                    <!-- The rule, said once, where somebody would otherwise
                         meet it as a refusal. -->
                    <span class="note">
                      Plain http only to this machine or this network. Anywhere
                      else needs https, or your key travels in the clear.
                    </span>
                  </div>
                </div>

                <div class="field">
                  <span class="what">Key</span>
                  <div class="control">
                    <TextField
                      value={open.apiKey}
                      oninput={(next) => change(one.id, { apiKey: next })}
                      placeholder="Paste it here"
                      ariaLabel="Key"
                      full
                      mono
                      secret
                    />
                    <span class="note">
                      Encrypted with your Windows account before it is written
                      to disk.
                    </span>
                  </div>
                </div>
              {/if}

              <div class="field">
                <span class="what">Model</span>
                <div class="control">
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

                  <!--
                    Only when there is something to say. "12 to choose from" is
                    a count of the list directly above it.
                  -->
                  {#if loadingModels}
                    <span class="note">Asking what it has…</span>
                  {:else if modelTrouble}
                    <!-- Whole sentences, because they are not all the same
                         kind of trouble. "Paste a key" is a thing to do next;
                         "type the name instead" is the way round a service
                         that will not list, and reading both at once told
                         somebody to type a model id and paste a key. -->
                    <span class="note warn">{modelTrouble}</span>
                  {:else if strayModel}
                    <span class="note warn">
                      {open.model} was not in the list it gave. Keep it if you
                      know it works, or pick one it named.
                    </span>
                  {/if}
                </div>
              </div>

              <div class="actions">
                <Button label="Refresh models" onclick={() => loadModels(open)} />
                <span class="spacer"></span>
                <Button label="Remove" tone="danger" onclick={() => remove(one.id)} />
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</Section>

<!--
  What the model has to get past before it changes anything.

  Its own section rather than a line under a provider, because it is about
  every provider and about MCP clients that have no card here at all.
-->
<Section
  label="Before it acts"
  description="Anything that changes something stops and asks you first. Running a command and writing a file can ask for more than a keypress."
>
  <Row
    title="Windows Hello to run a command or write a file"
    description={helloDescription}
  >
    {#snippet control()}
      <Toggle
        bind:checked={prefs.ai.helloForHeavyActions}
        onchange={save}
        label="Windows Hello to run a command or write a file"
      />
    {/snippet}
  </Row>
</Section>

{#if addable.length}
  <Section
    bare
    label="Add"
    description="Each of these is set up separately, and you can keep several."
  >
    <div class="stack">
      {#each addable as one (one.id)}
        <div class="provider addable">
          <div class="head">
            <div class="pick static">
              <AiMark name={one.id} />

              <!--
                The name and nothing else.

                What each service wants from you is worth saying, but not
                seven times at once in a list somebody is scanning for a name.
                It moves to the top of that provider's own setup, which is
                where it is actually needed and where there is room for it.
              -->
              <span class="text">
                <span class="name">{one.name}</span>
              </span>
            </div>

            <Button label="Add" onclick={() => add(one)} />
          </div>
        </div>
      {/each}
    </div>
  </Section>
{/if}

<style>
  /*
   * A grid, because a provider card is not a settings row.
   *
   * A mark, a name and a model id stretched across the full 872px content
   * column left most of every card empty, and seven of them read as seven
   * banners rather than as a set of things to choose between. Cards want to be
   * about as wide as they are informative.
   */
  .stack {
    /*
     * The label column, named once because two rules have to agree about it.
     * The field grid states the width and the action row underneath has to
     * clear the same width, and while both said 68px the second was a copy
     * waiting to be left behind by the first.
     */
    --label-column: 68px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(236px, 1fr));
    gap: var(--space-2);
  }

  .provider {
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    transition:
      background-color var(--motion-enter) ease,
      box-shadow var(--motion-enter) ease;
  }

  /*
   * The one that answers, washed rather than badged.
   *
   * This is the accent doing the job it is reserved for: saying which of
   * several things is selected. It is the same treatment the settings sidebar
   * gives the open panel, so the two read as the same kind of state.
   */
  .provider.on {
    background: var(--accent-fill);
  }

  /* Being set up needs the whole width, so an open card takes the whole row. */
  .provider.open {
    grid-column: 1 / -1;
    background: var(--fill-2);
  }

  .provider.on.open {
    background: var(--accent-fill);
    box-shadow: var(--ring-accent-faint);
  }

  /* Fills a card the grid has stretched, so the contents stay centred in it. */
  .head {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3) 0 0;
  }

  .pick {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    /* Two lines of text beside a 28px mark, with room to breathe. */
    min-height: 56px;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: var(--radius-lg);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  /* The Add list is not a choice, so its head must not look pressable. */
  .pick.static {
    cursor: default;
  }

  .pick:not(.static):hover {
    background: var(--fill-1);
  }

  .pick:focus-visible {
    outline: none;
    box-shadow: var(--ring-selected);
  }

  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-half);
    min-width: 0;
  }

  .name {
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
  }

  .status {
    color: var(--text-2);
    font-size: var(--text-meta);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  /* Something left to do, dimmed rather than alarmed: nothing is broken. */
  .status.todo {
    color: var(--text-3);
  }

  .answering {
    flex: none;
    color: var(--accent);
    font-size: var(--text-meta);
    white-space: nowrap;
  }

  .disclose {
    display: grid;
    place-items: center;
    flex: none;
    width: var(--icon-tile);
    height: var(--icon-tile);
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-3);
    cursor: pointer;
    transition:
      transform var(--motion-enter) ease,
      color var(--motion-enter) ease,
      background-color var(--motion-enter) ease;
  }

  .disclose:hover {
    background: var(--fill-2);
    color: var(--text-1);
  }

  .disclose:focus-visible {
    outline: none;
    box-shadow: var(--ring-selected);
  }

  .provider.open .disclose {
    transform: rotate(180deg);
    color: var(--text-1);
  }

  /*
   * The form, inside the card it configures.
   *
   * A hairline instead of a second background: the setup belongs to the card
   * above it, and giving it a fill of its own is what made a provider being
   * configured look like a window that had opened on top of the panel.
   */
  .setup {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: 0 var(--space-3);
    padding: var(--space-4) 0;
    border-top: 1px solid var(--hairline);
  }

  /*
   * A label column, so the fields line up.
   *
   * Stacked label-over-input-over-hint gave three left edges per field and
   * nine down the form, which is the shape of a page with no stylesheet.
   */
  .field {
    display: grid;
    grid-template-columns: var(--label-column) minmax(0, 1fr);
    align-items: baseline;
    gap: var(--space-3);
  }

  .what {
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .control {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  /* The one line of prose in the form, set apart from the fields below it. */
  .intro {
    margin: 0 0 var(--space-1);
    max-width: 68ch;
    color: var(--text-2);
    font-size: var(--text-meta);
    line-height: 1.55;
  }

  .note {
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: 1.5;
    max-width: 62ch;
  }

  .note.warn {
    color: var(--text-2);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    /* Clears the label column, so the buttons sit under the fields. */
    padding-left: calc(var(--label-column) + var(--space-3));
  }

  .spacer {
    flex: 1;
  }
</style>
