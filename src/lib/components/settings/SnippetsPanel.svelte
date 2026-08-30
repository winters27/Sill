<script lang="ts">
  import { onMount } from "svelte";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import Toggle from "../Toggle.svelte";
  import {
    deleteSnippet,
    emptySnippet,
    listSnippets,
    PLACEHOLDERS,
    saveSnippet,
    exportSnippets,
    importSnippets,
    type Snippet,
  } from "$lib/snippets";
  import type { Preferences } from "$lib/settings";

  /**
   * Writes them out, and says where they went.
   *
   * Closing the dialog without choosing answers nothing, which is somebody
   * changing their mind rather than something failing.
   */
  async function sendOut() {
    transfer = "";

    try {
      const where = await exportSnippets();
      if (where) transfer = `Written to ${where}`;
    } catch (err) {
      transfer = `${err}`;
    }
  }

  /**
   * Reads them in, and says exactly what changed.
   *
   * Counted rather than summarised, because the two surprising outcomes both
   * need naming: snippets skipped for being here already, and keywords left
   * off because another snippet answers to them.
   */
  async function bringIn() {
    transfer = "";

    try {
      const done = await importSnippets();
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
      snippets = await listSnippets();
    } catch (err) {
      transfer = `${err}`;
    }
  }

  interface Props {
    /** Not `$bindable`: nothing here reassigns it, only writes its fields. */
    prefs: Preferences;
    commit: () => void;
  }

  let { prefs, commit }: Props = $props();

  let snippets = $state<Snippet[]>([]);
  /** What the last export or import did, said in words rather than counted. */
  let transfer = $state("");
  let editing = $state<Snippet | null>(null);
  let error = $state("");
  let confirmingDelete = $state("");

  const isNew = $derived(editing !== null && editing.id === "");

  async function refresh() {
    snippets = await listSnippets();
  }

  function edit(snippet: Snippet) {
    // A copy, so cancelling leaves the list untouched.
    editing = { ...snippet };
    error = "";
  }

  async function save() {
    if (!editing) return;

    const draft = { ...editing, name: editing.name.trim(), keyword: editing.keyword.trim() };
    if (!draft.name) {
      error = "Give it a name so you can find it again.";
      return;
    }
    if (!draft.content) {
      error = "A snippet with nothing in it would paste nothing.";
      return;
    }

    try {
      await saveSnippet(draft);
      editing = null;
      error = "";
      await refresh();
    } catch (err) {
      // The keyword clash comes back from Rust, which is the only place that
      // can see every snippet at once.
      error = String(err);
    }
  }

  async function remove(snippet: Snippet) {
    if (confirmingDelete !== snippet.id) {
      confirmingDelete = snippet.id;
      setTimeout(() => {
        if (confirmingDelete === snippet.id) confirmingDelete = "";
      }, 4000);
      return;
    }
    confirmingDelete = "";
    await deleteSnippet(snippet.id);
    if (editing?.id === snippet.id) editing = null;
    await refresh();
  }

  onMount(refresh);
</script>

<Section
  label="Expansion"
  description="A keyword typed anywhere is replaced by its snippet. Snippets are always reachable from the launcher whether or not this is on."
>
  <Row
    title="Expand keywords as I type"
    description="Installs a keyboard hook that watches for your keywords. It never swallows a key, and what it remembers is a short rolling buffer held in memory and cleared by Enter, Tab, Escape or any arrow."
  >
    {#snippet control()}
      <Toggle
        bind:checked={prefs.snippets.expandKeywords}
        onchange={commit}
        label="Expand keywords as I type"
      />
    {/snippet}
  </Row>
</Section>

<Section
  label={editing ? (isNew ? "New snippet" : "Editing") : "Snippets"}
  description={editing
    ? undefined
    : "Pick one to edit it. Enter pastes a snippet from the launcher."}
  bare={editing !== null}
>
  {#if editing}
    <div class="editor">
      <div class="pair">
        <label class="field">
          <span>Name</span>
          <input bind:value={editing.name} placeholder="Email signature" spellcheck="false" />
        </label>

        <label class="field keyword">
          <span>Keyword</span>
          <input
            bind:value={editing.keyword}
            placeholder=";sig"
            spellcheck="false"
            autocomplete="off"
          />
        </label>
      </div>

      <label class="field">
        <span>Content</span>
        <textarea rows="7" bind:value={editing.content} spellcheck="false"></textarea>
      </label>

      <div class="placeholders">
        {#each PLACEHOLDERS as placeholder (placeholder.token)}
          <button
            class="token"
            title={placeholder.means}
            onclick={() => {
              if (editing) editing.content += placeholder.token;
            }}
          >
            {placeholder.token}
          </button>
        {/each}
      </div>

      <label class="check">
        <Toggle
          bind:checked={editing.wholeWord}
          onchange={() => {}}
          label="Only as a whole word"
        />
        <span>
          Only expand when the keyword stands alone. With this off, a keyword of
          <code>sig</code> also fires inside <code>resig</code>.
        </span>
      </label>

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
    {#each snippets as snippet (snippet.id)}
      <Row
        title={snippet.name}
        description={snippet.content.split("\n")[0].slice(0, 90)}
      >
        {#snippet control()}
          <div class="row-actions">
            {#if snippet.keyword}
              <span class="keyword-tag">{snippet.keyword}</span>
            {:else}
              <span class="no-keyword">launcher only</span>
            {/if}
            <Button label="Edit" onclick={() => edit(snippet)} />
          </div>
        {/snippet}
      </Row>
    {/each}

    {#if snippets.length === 0}
      <p class="empty">
        No snippets yet. One with a keyword expands wherever you type it; one without is still
        reachable from the launcher.
      </p>
    {/if}
  {/if}
</Section>

{#if !editing}
  <Section label="Add" bare>
    <Button label="New snippet" onclick={() => edit(emptySnippet())} />
  </Section>

  <Section
    label="Moving them around"
    description="A file of snippets, so they can be backed up, carried to another machine, or brought over from another tool. Importing only ever adds: nothing already here is removed."
  >
    <Row title="Snippets as a file">
      {#snippet control()}
        <div class="row-actions">
          <Button label="Export" onclick={sendOut} />
          <Button label="Import" onclick={bringIn} />
        </div>
      {/snippet}
    </Row>

    {#if transfer}
      <p class="transfer">{transfer}</p>
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
    background: rgba(255, 255, 255, 0.02);
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

  input,
  textarea {
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-1);
    box-shadow: inset 0 0 0 1px var(--hairline);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    outline: none;
    user-select: text;
    transition: box-shadow 0.15s var(--ease);
  }

  input:focus,
  textarea:focus {
    box-shadow: inset 0 0 0 1px var(--hairline-strong);
  }

  input::placeholder,
  textarea::placeholder {
    color: var(--text-3);
  }

  .keyword input,
  textarea {
    font-family: var(--font-mono);
    font-size: var(--text-meta);
  }

  textarea {
    resize: vertical;
    line-height: 1.6;
  }

  /* Listed rather than documented elsewhere: a feature nobody can discover
     is a feature nobody has. */
  .placeholders {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .token {
    padding: 2px var(--space-2);
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: var(--text-label);
    cursor: pointer;
    transition: background-color 0.15s var(--ease), color 0.15s var(--ease);
  }

  .token:hover {
    background: var(--hairline-strong);
    color: var(--text-1);
  }

  .check {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    cursor: pointer;
  }

  .check span {
    max-width: 58ch;
    font-size: var(--text-meta);
    line-height: 1.55;
    color: var(--text-2);
  }

  code {
    font-family: var(--font-mono);
    font-size: var(--text-group);
    color: var(--text-1);
  }

  .error {
    margin: 0;
    font-size: var(--text-meta);
    color: var(--accent-red);
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
    padding: 2px var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    font-family: var(--font-mono);
    font-size: var(--text-label);
    color: var(--text-1);
  }

  .no-keyword {
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  .transfer {
    margin: 0;
    padding: 2px 0 var(--space-1);
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  .empty {
    margin: 0;
    padding: var(--space-4) 0;
    max-width: 56ch;
    font-size: var(--text-body);
    line-height: 1.7;
    color: var(--text-2);
  }
</style>
