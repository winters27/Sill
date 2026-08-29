<script lang="ts">
  import { onMount } from "svelte";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import {
    blankQuicklink,
    deleteQuicklink,
    listQuicklinks,
    needsArgument,
    saveQuicklink,
    type Quicklink,
  } from "$lib/quicklinks";

  let links = $state<Quicklink[]>([]);
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

  function edit(link: Quicklink) {
    // A copy, so cancelling leaves the list untouched.
    editing = { ...link };
    error = "";
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
        <span>Link</span>
        <input
          bind:value={editing.link}
          placeholder={EXAMPLE}
          spellcheck="false"
          autocomplete="off"
        />
      </label>

      <div class="placeholders">
        {#each TOKENS as placeholder (placeholder.token)}
          <button
            class="token"
            title={placeholder.means}
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

      <label class="field">
        <span>Open with</span>
        <input
          bind:value={editing.openWith}
          placeholder="Leave empty for whatever Windows uses"
          spellcheck="false"
          autocomplete="off"
        />
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

    {#if links.length === 0}
      <p class="empty">
        No quicklinks yet. A good first one is your search engine, with the query placeholder
        where the words go: <code>{EXAMPLE}</code>
      </p>
    {/if}
  {/if}
</Section>

{#if !editing}
  <Section label="Add" bare>
    <Button label="New quicklink" onclick={() => edit(blankQuicklink())} />
  </Section>
{/if}

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 18px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.02);
    box-shadow: var(--bevel-tile);
  }

  .pair {
    display: flex;
    gap: 12px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    flex: 1;
    min-width: 0;
  }

  .field span {
    font-size: var(--text-group);
    font-weight: 500;
    color: var(--text-faint);
  }

  .keyword {
    max-width: 180px;
  }

  input {
    padding: 7px 10px;
    border: 0;
    border-radius: var(--radius-sm);
    background: rgba(var(--accent-rgb), 0.05);
    box-shadow: inset 0 0 0 1px var(--hairline);
    color: var(--core-foreground);
    font: inherit;
    font-size: 13px;
    outline: none;
    user-select: text;
  }

  input:focus-visible {
    box-shadow: inset 0 0 0 1px rgba(var(--accent-rgb), 0.4);
  }

  .placeholders {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .token {
    padding: 3px 8px;
    border: 0;
    border-radius: 5px;
    background: rgba(var(--accent-rgb), 0.08);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 11.5px;
    cursor: pointer;
  }

  .token:hover {
    background: rgba(var(--accent-rgb), 0.16);
    color: var(--core-foreground);
  }

  .note,
  .empty {
    margin: 0;
    max-width: 62ch;
    font-size: var(--text-meta);
    line-height: 1.65;
    color: var(--text-muted);
  }

  .empty {
    padding: 18px 0;
  }

  .error {
    margin: 0;
    font-size: var(--text-meta);
    color: var(--accent-red);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .spacer {
    flex: 1;
  }

  .row-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .keyword-tag {
    padding: 2px 7px;
    border-radius: 4px;
    background: rgba(var(--accent-rgb), 0.1);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 11.5px;
  }

  .asks {
    font-size: 11.5px;
    color: var(--text-faint);
  }

  code {
    font-family: var(--font-mono);
    font-size: 0.94em;
    color: var(--text-muted);
  }
</style>
