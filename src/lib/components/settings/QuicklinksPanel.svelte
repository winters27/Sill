<script lang="ts">
  import { onMount } from "svelte";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import Toggle from "../Toggle.svelte";
  import Instead from "../Instead.svelte";
  import { standing } from "$lib/instead";
  import { hint } from "$lib/hint";
  import {
    blankQuicklink,
    deleteQuicklink,
    exportQuicklinks,
    importQuicklinks,
    listQuicklinks,
    needsArgument,
    quicklinkSchemeToAllow,
    saveQuicklink,
    type Quicklink,
  } from "$lib/quicklinks";

  /**
   * Writes them out, and says where they went.
   *
   * Closing the dialog without choosing answers nothing, which is somebody
   * changing their mind rather than something failing.
   */
  async function sendOut() {
    transfer = "";

    try {
      const where = await exportQuicklinks();
      if (where) transfer = `Written to ${where}`;
    } catch (err) {
      transfer = `${err}`;
    }
  }

  /**
   * Reads them in, and says exactly what changed.
   *
   * Counted rather than summarised, because the two surprising outcomes both
   * need naming: links skipped for being here already, and keywords left off
   * because another link answers to them.
   */
  async function bringIn() {
    transfer = "";

    try {
      const done = await importQuicklinks();
      if (!done) return;

      const said: string[] = [];
      if (done.added) said.push(`${done.added} added`);
      if (done.updated) said.push(`${done.updated} updated`);
      if (done.skipped) said.push(`${done.skipped} already here`);
      if (done.keywordsTaken) {
        said.push(
          `${done.keywordsTaken} came without ${
            done.keywordsTaken === 1 ? "its keyword" : "their keywords"
          }, which ${done.keywordsTaken === 1 ? "was" : "were"} already in use`,
        );
      }

      transfer = said.length ? `${said.join(", ")}.` : "Nothing to bring in.";
      links = await listQuicklinks();
    } catch (err) {
      transfer = `${err}`;
    }
  }

  let links = $state<Quicklink[]>([]);

  /** What the last export or import did, said once and left on screen. */
  let transfer = $state("");
  let editing = $state<Quicklink | null>(null);
  let error = $state("");
  let confirmingDelete = $state("");

  const isNew = $derived(editing !== null && editing.id === "");

  /**
   * The placeholders a link can carry.
   *
   * The same grammar snippets use, because it is the same expander. Only the
   * query one is particular to quicklinks: a snippet expands where the caret
   * already is, so it has nowhere to ask.
   */
  const TOKENS = [
    { token: "{query}", means: "What you type after picking this link" },
    { token: "{clipboard}", means: "Whatever is on the clipboard" },
    { token: "{date}", means: "Today, as YYYY-MM-DD" },
    { token: "{time}", means: "Now, as HH:MM" },
    { token: "{uuid}", means: "A fresh identifier" },
  ];

  const EXAMPLE = "https://github.com/search?q={query}";

  async function refresh() {
    links = await listQuicklinks();
  }

  /**
   * The scheme the link being edited would need allowing for, or nothing.
   *
   * Asked of Rust when the link field is left rather than on every
   * keystroke: a half-typed address is not worth a round trip, and Rust is
   * the only side that knows which schemes it opens on its own.
   */
  let schemeToAllow = $state<string | null>(null);

  /** The tags as typed, comma or space separated, before they are a list. */
  let tagsText = $state("");

  function parseTags(text: string): string[] {
    return text
      .split(/[,\s]+/)
      .map((one) => one.trim())
      .filter(Boolean);
  }

  async function askScheme() {
    schemeToAllow = editing ? await quicklinkSchemeToAllow(editing.link) : null;
  }

  function edit(link: Quicklink) {
    // A copy, so cancelling leaves the list untouched.
    editing = { ...link, tags: [...(link.tags ?? [])] };
    tagsText = (link.tags ?? []).join(", ");
    error = "";
    void askScheme();
  }

  async function save() {
    if (!editing) return;

    const draft = {
      ...editing,
      name: editing.name.trim(),
      link: editing.link.trim(),
      keyword: editing.keyword.trim(),
      openWith: editing.openWith.trim(),
    };
    if (!draft.name) {
      error = "Give it a name so you can find it again.";
      return;
    }
    if (!draft.link) {
      error = "A quicklink with nowhere to go would open nothing.";
      return;
    }

    try {
      await saveQuicklink(draft);
      editing = null;
      error = "";
      await refresh();
    } catch (err) {
      error = String(err);
    }
  }

  async function remove(link: Quicklink) {
    if (confirmingDelete !== link.id) {
      confirmingDelete = link.id;
      // Reverts on its own, so an unconfirmed press does not leave the button
      // armed for whoever walks past next.
      setTimeout(() => {
        if (confirmingDelete === link.id) confirmingDelete = "";
      }, 4000);
      return;
    }
    confirmingDelete = "";
    await deleteQuicklink(link.id);
    if (editing?.id === link.id) editing = null;
    await refresh();
  }

  onMount(refresh);
</script>

<Section
  label={editing ? (isNew ? "New quicklink" : "Editing") : "Quicklinks"}
  description={editing
    ? undefined
    : "A saved address with a hole in it. Pick one from the launcher and whatever you type next goes in the hole, so one link covers every search of that site."}
  bare={editing !== null}
>
  {#if editing}
    <div class="editor">
      <div class="pair">
        <label class="field">
          <span>Name</span>
          <input bind:value={editing.name} placeholder="Search GitHub" spellcheck="false" />
        </label>

        <label class="field keyword">
          <span>Keyword</span>
          <input
            bind:value={editing.keyword}
            placeholder="gh"
            spellcheck="false"
            autocomplete="off"
          />
        </label>
      </div>

      <label class="field">
        <span>Tags</span>
        <input
          bind:value={tagsText}
          oninput={() => {
            if (editing) editing.tags = parseTags(tagsText);
          }}
          placeholder="work, docs"
          spellcheck="false"
          autocomplete="off"
        />
      </label>

      <label class="field">
        <span>Link</span>
        <input
          bind:value={editing.link}
          placeholder={EXAMPLE}
          spellcheck="false"
          autocomplete="off"
          onblur={askScheme}
        />
      </label>

      <div class="placeholders">
        {#each TOKENS as placeholder (placeholder.token)}
          <button
            class="token"
            use:hint={placeholder.means}
            onclick={() => {
              if (editing) editing.link += placeholder.token;
            }}
          >
            {placeholder.token}
          </button>
        {/each}
      </div>

      {#if editing.link}
        <p class="note">
          {#if needsArgument(editing.link)}
            Picking this in the launcher asks what to search for. What you type is escaped
            before it goes into the address, so spaces and punctuation survive the trip.
          {:else}
            This opens straight from the launcher. Add the query placeholder to make it ask
            for something first.
          {/if}
        </p>
      {/if}

      <!-- indexed as "Open with" -->
      <label class="field">
        <span>Open with</span>
        <input
          bind:value={editing.openWith}
          placeholder="Leave empty for whatever Windows uses"
          spellcheck="false"
          autocomplete="off"
        />
      </label>

      {#if schemeToAllow}
        <!-- Only for a link whose address Sill would otherwise refuse. Web,
             mail and settings addresses never show this, and the schemes
             that run code never can. -->
        <div class="allow">
          <Toggle
            checked={editing.allowedScheme === schemeToAllow}
            onchange={(on) => {
              if (editing) editing.allowedScheme = on ? (schemeToAllow ?? "") : "";
            }}
            label={`Open ${schemeToAllow}: addresses`}
          />
          <p class="note">
            Open <code>{schemeToAllow}:</code> addresses. Sill opens web, mail and settings
            addresses on its own; anything else is handed to whichever program owns that
            scheme, with the whole address as its argument, only once you allow it here.
            An imported link never carries this.
          </p>
        </div>
      {/if}

      {#if error}<p class="error">{error}</p>{/if}

      <div class="actions">
        <Button label="Save" onclick={save} />
        <Button label="Cancel" onclick={() => ((editing = null), (error = ""))} />
        <span class="spacer"></span>
        {#if !isNew}
          <Button
            label={confirmingDelete === editing.id ? "Delete it?" : "Delete"}
            tone="danger"
            onclick={() => editing && void remove(editing)}
          />
        {/if}
      </div>
    </div>
  {:else}
    {#each links as link (link.id)}
      <Row title={link.name} description={link.link}>
        {#snippet control()}
          <div class="row-actions">
            {#if link.keyword}
              <span class="keyword-tag">{link.keyword}</span>
            {/if}
            {#if needsArgument(link.link)}
              <span class="asks">asks</span>
            {/if}
            <Button label="Edit" onclick={() => edit(link)} />
          </div>
        {/snippet}
      </Row>
    {/each}

    <Instead
      tone={standing({ failed: false, loading: false, count: links.length })}
      inline
      headline="No quicklinks yet"
    >
      A good first one is your search engine, with the query placeholder where the words go:
      <code>{EXAMPLE}</code>
    </Instead>

    <!-- not a setting: the one thing this list can be told to do -->
    <Row
      title="Add a quicklink"
      description="A saved address that takes what you type and goes straight there."
    >
      {#snippet control()}
        <Button label="New" onclick={() => edit(blankQuicklink())} />
      {/snippet}
    </Row>
  {/if}
</Section>

{#if !editing}

  <Section
    label="Moving them around"
    description="A file of quicklinks, so they can be backed up, carried to another machine, or brought over from another tool. Importing only ever adds: nothing already here is removed."
  >
    <Row title="Quicklinks as a file">
      {#snippet control()}
        <div class="actions">
          <Button label="Export" onclick={sendOut} />
          <Button label="Import" onclick={bringIn} />
        </div>
      {/snippet}
    </Row>

    {#if transfer}
      <p class="note">{transfer}</p>
    {/if}
  </Section>
{/if}

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4);
    border-radius: var(--radius-lg);
    background: var(--fill-0);
    box-shadow: var(--bevel-tile);
  }

  .pair {
    display: flex;
    gap: var(--space-3);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    flex: 1;
    min-width: 0;
  }

  .field span {
    font-size: var(--text-group);
    font-weight: var(--weight-medium);
    color: var(--text-3);
  }

  .keyword {
    max-width: 180px;
  }

  /* The same field the Snippets editor draws, one panel up. The two were
     copied from each other and drifted: this one had wider text and
     tighter padding, so the two editors read as two designs. */
  input {
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    outline: none;
    user-select: text;
  }

  input:focus {
    box-shadow: var(--ring-strong);
  }

  .placeholders {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .token {
    padding: var(--space-half) var(--space-2);
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: var(--text-label);
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .token:hover {
    background: var(--hairline-strong);
    color: var(--text-1);
  }

  .note {
    margin: 0;
    max-width: 62ch;
    font-size: var(--text-meta);
    line-height: 1.65;
    color: var(--text-2);
  }

  .error {
    margin: 0;
    font-size: var(--text-meta);
    color: var(--danger);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .spacer {
    flex: 1;
  }

  .row-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .keyword-tag {
    padding: var(--space-half) var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: var(--text-label);
  }

  .asks {
    font-size: var(--text-group);
    color: var(--text-3);
  }

  code {
    font-family: var(--font-mono);
    font-size: var(--text-group);
    color: var(--text-1);
  }
</style>
