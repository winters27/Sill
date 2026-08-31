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

  /**
   * The programs field, as typed.
   *
   * Kept beside the list rather than derived from it, so a half-typed name is
   * not rewritten under the cursor: "co" would otherwise become "co" the
   * moment it parsed, and a trailing comma would vanish as it was typed.
   */
  let onlyInText = $state("");

  /** The formatted body, when there is one. */
  let richBody = $state<HTMLDivElement | null>(null);

  /**
   * Which snippet's markup has been put into the box.
   *
   * A contenteditable is not bound to anything: it holds what the browser
   * decides it holds, so the markup has to be written into it once when the
   * box appears. Once, and this is what says once. Writing it on every
   * keystroke would put the caret back at the start with each letter.
   *
   * Deliberately not `$state`. It is written inside the effect that reads it,
   * and a reactive one would wake that effect up on its own change.
   */
  let seeded: string | null = null;

  $effect(() => {
    const box = richBody;
    // Read so the effect wakes when the snippet changes, not only when the
    // box appears. The guard below is what stops it running per keystroke.
    const markup = editing?.html ?? "";
    const which = editing ? editing.id || "new" : null;

    if (!box || which === null || seeded === which) return;

    seeded = which;
    box.innerHTML = markup;
  });

  /** Whether the snippet being edited keeps its formatting. */
  const formatted = $derived(Boolean(editing?.html));

  /**
   * Every collection any snippet is in.
   *
   * There is no list of them: a collection exists because a snippet says it
   * does, so this is where the list comes from and it cannot go stale.
   */
  const collections = $derived(
    [...new Set(snippets.map((one) => one.collection.trim()).filter(Boolean))].sort(),
  );

  /**
   * The snippets, under the collection each is in.
   *
   * Ungrouped ones last rather than first: a heading over the ones that have
   * no heading would be inventing a collection called "everything else".
   */
  const grouped = $derived.by(() => {
    const groups = new Map<string, Snippet[]>();

    for (const snippet of snippets) {
      const name = snippet.collection.trim();
      const list = groups.get(name);
      if (list) list.push(snippet);
      else groups.set(name, [snippet]);
    }

    return [...groups.entries()].sort(([a], [b]) => {
      if (!a) return 1;
      if (!b) return -1;
      return a.localeCompare(b);
    });
  });

  const isNew = $derived(editing !== null && editing.id === "");

  async function refresh() {
    snippets = await listSnippets();
  }

  function edit(snippet: Snippet) {
    // A copy, so cancelling leaves the list untouched.
    editing = { ...snippet, onlyIn: [...snippet.onlyIn] };
    onlyInText = snippet.onlyIn.join(", ");
    // A different snippet, so the box has to be filled again.
    seeded = null;
    error = "";
  }

  /**
   * The programs typed into the field, as a list.
   *
   * Separated by commas or spaces, because both are what somebody types, and
   * emptied of blanks so a trailing comma does not become a program called
   * nothing that nothing ever matches.
   */
  function parsePrograms(text: string): string[] {
    return text
      .split(/[,\s]+/)
      .map((one) => one.trim())
      .filter(Boolean);
  }

  /**
   * Turns formatting on or off for the snippet being edited.
   *
   * Turning it on seeds the markup from the plain text, so nothing already
   * written is lost. Turning it off drops the markup and keeps the words,
   * which is the direction that can lose something, so it is the plain text
   * that survives rather than the other way round.
   */
  function setFormatted(on: boolean) {
    if (!editing) return;

    if (!on) {
      editing.html = "";
      return;
    }

    // Turning it on is a fresh box whatever was in one before.
    seeded = null;

    if (!editing.html) {
      editing.html = editing.content
        .split("\n")
        .map((line) => escapeMarkup(line))
        .join("<br>");
    }

    // The box itself is filled by the effect above, which runs once the block
    // has rendered. Doing it here would be writing to an element that does not
    // exist yet.
  }

  /** The five characters that mean something in markup. */
  function escapeMarkup(value: string): string {
    return value
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#39;");
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
        {#if formatted}
          <!--
            The browser does the formatting. In a contenteditable, Ctrl+B, I
            and U are handled by the engine itself, so there is no toolbar to
            build and no command layer to keep in step with what the keys do.

            Both versions are kept on every keystroke: the markup is what a
            formatted paste sends, and the plain text underneath it is what a
            plain field receives and what the launcher shows as a preview.
          -->
          <div
            class="rich"
            role="textbox"
            tabindex="0"
            aria-multiline="true"
            aria-label="Content, with formatting"
            contenteditable="true"
            bind:this={richBody}
            oninput={(event) => {
              if (!editing) return;
              const el = event.currentTarget;
              editing.html = el.innerHTML;
              editing.content = el.innerText;
            }}
          ></div>
        {:else}
          <textarea rows="7" bind:value={editing.content} spellcheck="false"></textarea>
        {/if}
      </label>

      <div class="pair">
        <label class="field">
          <span>Collection</span>
          <input
            bind:value={editing.collection}
            placeholder="Ungrouped"
            list="sill-collections"
            spellcheck="false"
            autocomplete="off"
          />
        </label>

        <label class="field">
          <span>Only in these programs</span>
          <input
            bind:value={onlyInText}
            oninput={() => {
              if (editing) editing.onlyIn = parsePrograms(onlyInText);
            }}
            placeholder="Any program"
            spellcheck="false"
            autocomplete="off"
          />
        </label>
      </div>

      <!--
        The collections that already exist, so a second snippet joins a group
        by picking it rather than by spelling it the same way. There is no list
        of collections anywhere else: they exist because snippets say they do.
      -->
      <datalist id="sill-collections">
        {#each collections as name (name)}
          <option value={name}></option>
        {/each}
      </datalist>

      <label class="check">
        <Toggle
          checked={formatted}
          onchange={(on) => setFormatted(on)}
          label="Keep formatting"
        />
        <span>
          Bold, italic and links, with <span class="sill-key">Ctrl</span>
          <span class="sill-key">B</span>, <span class="sill-key">Ctrl</span>
          <span class="sill-key">I</span> and <span class="sill-key">Ctrl</span>
          <span class="sill-key">U</span>. A formatted snippet is pasted rather
          than typed, so it borrows the clipboard for a moment and puts it back.
        </span>
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
    {#each grouped as [collection, members] (collection)}
      <!--
        Only when there is more than one group. A single heading over the
        whole list is a label rather than a grouping, and somebody who has
        never made a collection should not be shown the idea of one.
      -->
      {#if grouped.length > 1}
        <p class="collection">{collection || "Ungrouped"}</p>
      {/if}

      {#each members as snippet (snippet.id)}
        <Row
          title={snippet.name}
          description={snippet.content.split("\n")[0].slice(0, 90)}
        >
          {#snippet control()}
            <div class="row-actions">
              {#if snippet.onlyIn.length}
                <span class="limited" title={snippet.onlyIn.join(", ")}>
                  {snippet.onlyIn.length === 1
                    ? snippet.onlyIn[0]
                    : `${snippet.onlyIn.length} programs`}
                </span>
              {/if}
              {#if snippet.html}
                <span class="limited">formatted</span>
              {/if}
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

  /* The heading over a collection, in the settings list. */
  .collection {
    margin: var(--space-4) 0 var(--space-1);
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .collection:first-child {
    margin-top: 0;
  }

  /*
   * What is unusual about a snippet, said quietly beside it.
   *
   * The same weight as the keyword tag next to it rather than a colour of its
   * own: these are facts about the row, not warnings, and three coloured tags
   * on one line is a row that looks like it has gone wrong.
   */
  .limited {
    color: var(--text-2);
    font-size: var(--text-meta);
    white-space: nowrap;
  }

  /*
   * The body, when it keeps its formatting.
   *
   * Sized to match the plain field beside it so turning formatting on does not
   * resize the editor under the cursor.
   */
  .rich {
    min-height: 9.5rem;
    max-height: 20rem;
    overflow-y: auto;
    padding: var(--space-2);
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: inset 0 0 0 1px var(--hairline);
    color: var(--text-1);
    font-size: var(--text-body);
    line-height: 1.5;
  }

  .rich:focus {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--accent);
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
