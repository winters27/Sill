<script lang="ts">
  import type { ElementNode, ViewTree } from "$lib/exthost/tree";
  import Instead from "./Instead.svelte";
  import { standing } from "$lib/instead";
  import { pickFiles } from "$lib/exthost/commands";
  import { hint } from "$lib/hint";

  interface Props {
    tree: ViewTree;
    node: ElementNode;
    version: number;
    /**
     * The command this form belongs to, which the file picker is opened
     * against. Empty in the preview harness, where there is no Rust behind the
     * button and pressing it is expected to do nothing.
     */
    session?: string;
    /** Called with the collected values, keyed by each field's id. */
    onsubmit: (values: Record<string, unknown>) => void;
  }

  let { tree, node, version, session = "", onsubmit }: Props = $props();

  /**
   * Field values, keyed by the `id` the extension gave each control.
   *
   * Raycast forms are uncontrolled from the extension's point of view: it
   * declares defaults and receives everything back on submit, so the values
   * live here rather than being pushed back on every keystroke.
   */
  let values = $state<Record<string, unknown>>({});

  const fields = $derived.by(() => {
    version;
    return tree.elementChildren(node);
  });

  const str = (n: ElementNode, key: string, fallback = ""): string => {
    const v = n.props[key];
    return typeof v === "string" ? v : fallback;
  };

  const id = (n: ElementNode): string => str(n, "id", `field-${n.id}`);

  /**
   * Seeds any field that has appeared since the last render.
   *
   * Only untouched fields are seeded, so a re-render caused by something else
   * on the form cannot wipe what the user has already typed.
   */
  $effect(() => {
    for (const field of fields) {
      const key = id(field);
      if (key in values) continue;

      if (field.tag === "Form.Checkbox") {
        values[key] = field.props.defaultValue === true;
      } else if (field.tag === "Form.DatePicker") {
        // Raycast hands the extension a Date; over the wire it is the ISO
        // string a `<input type=date>` also speaks, so nothing is converted.
        values[key] = str(field, "defaultValue");
      } else if (field.tag === "Form.TagPicker" || field.tag === "Form.FilePicker") {
        // Both hand the extension a list. A picker's is a list of paths, and
        // Raycast's own `defaultValue` for one is `string[]`, so an extension
        // reopening a form with what was chosen last time gets it back.
        values[key] = Array.isArray(field.props.defaultValue) ? field.props.defaultValue : [];
      } else if (field.tag === "Form.Dropdown") {
        const first = tree.elementChildren(field).find((c) => c.tag === "Form.Dropdown.Item");
        values[key] = field.props.defaultValue ?? (first ? str(first, "value") : "");
      } else if (
        field.tag === "Form.TextField" ||
        field.tag === "Form.TextArea" ||
        field.tag === "Form.PasswordField"
      ) {
        values[key] = str(field, "defaultValue");
      }
    }
  });

  export function submit() {
    onsubmit($state.snapshot(values));
  }

  /** What is chosen in a tag picker, which is always a list. */
  function picked(key: string): string[] {
    const held = values[key];
    return Array.isArray(held) ? (held as string[]) : [];
  }

  function toggleTag(key: string, value: string) {
    const held = picked(key);
    values[key] = held.includes(value) ? held.filter((v) => v !== value) : [...held, value];
  }

  function tagOptions(field: ElementNode) {
    return tree
      .elementChildren(field)
      .filter((child) => child.tag === "Form.TagPicker.Item")
      .map((child) => ({
        value: str(child, "value"),
        title: str(child, "title") || str(child, "value"),
      }));
  }

  /** What a picker has been given, which is always a list of paths. */
  function chosen(key: string): string[] {
    const held = values[key];
    return Array.isArray(held) ? (held as string[]) : [];
  }

  /**
   * The name to show for a chosen path, which is its last part.
   *
   * The whole path is a tooltip rather than the label. A form field is about
   * 380 pixels wide and a real path is longer than that, so drawing it in full
   * either wraps the row or is cut off in the middle, and the half that gets
   * cut is the half that says which file it is.
   */
  function leaf(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? path;
  }

  /**
   * Opens Windows' own dialog and keeps whatever came back.
   *
   * The dialog runs in Rust. The launcher window is not on
   * `capabilities/file-picker.json`, so the frontend cannot open one, and that
   * is deliberate rather than an oversight: this is the window an extension
   * draws into.
   *
   * Nothing is read here and nothing is read there. What arrives is the paths
   * somebody chose, and an extension wanting to open one of them still needs
   * `fileRead` at the module gate, exactly as it did before there was a picker.
   */
  async function choose(field: ElementNode, key: string) {
    if (!session) return;

    const picked = await pickFiles(
      session,
      field.props.canChooseDirectories === true && field.props.canChooseFiles !== true,
      field.props.allowMultipleSelection !== false,
    );

    // Nothing chosen leaves what was there. Somebody opening the dialog and
    // changing their mind has not asked for the field to be emptied, and
    // clearing it is what the button beside it is for.
    if (picked.length === 0) return;

    values[key] = field.props.allowMultipleSelection === false ? picked.slice(0, 1) : picked;
  }

  function dropdownOptions(field: ElementNode) {
    const out: { value: string; title: string }[] = [];

    const walk = (parent: ElementNode) => {
      for (const child of tree.elementChildren(parent)) {
        if (child.tag === "Form.Dropdown.Item") {
          out.push({ value: str(child, "value"), title: str(child, "title") });
        } else {
          // Sections group items without changing what a value means.
          walk(child);
        }
      }
    };

    walk(field);
    return out;
  }
</script>

<div class="form">
  {#each fields as field (field.id)}
    {@const key = id(field)}

    {#if field.tag === "Form.Separator"}
      <div class="separator"></div>
    {:else if field.tag === "Form.Description"}
      <div class="row">
        <div class="label">{str(field, "title")}</div>
        <div class="description">{str(field, "text")}</div>
      </div>
    {:else if field.tag === "Form.Checkbox"}
      <div class="row">
        <div class="label">{str(field, "title")}</div>
        <label class="checkbox">
          <input type="checkbox" bind:checked={values[key] as boolean} />
          <span>{str(field, "label")}</span>
        </label>
      </div>
    {:else if field.tag === "Form.Dropdown"}
      <div class="row">
        <div class="label">{str(field, "title")}</div>
        <!-- The label beside it is a `div`, so nothing links the two. An
             extension's field had no name at all until this. -->
        <select aria-label={str(field, "title")} bind:value={values[key] as string}>
          {#each dropdownOptions(field) as option (option.value)}
            <option value={option.value}>{option.title}</option>
          {/each}
        </select>
      </div>
    {:else if field.tag === "Form.TagPicker"}
      <div class="row">
        <div class="label">{str(field, "title")}</div>
        <!--
          A row of chips that toggle, rather than a multiple `<select>`.
          A native multi-select needs Ctrl held to pick a second thing, which
          is a rule nobody knows and one this window can simply not have.
        -->
        <div class="tags" role="group" aria-label={str(field, "title")}>
          {#each tagOptions(field) as option (option.value)}
            {@const chosen = picked(key).includes(option.value)}
            <button
              type="button"
              class="pick"
              class:chosen
              aria-pressed={chosen}
              onclick={() => toggleTag(key, option.value)}
            >
              {option.title}
            </button>
          {/each}
        </div>
      </div>
    {:else if field.tag === "Form.FilePicker"}
      <div class="row">
        <div class="label">{str(field, "title")}</div>
        <!--
          A button and what it chose, rather than a text field somebody could
          type a path into. A typed path is the extension being handed a place
          nobody looked at, and the point of a picker is that the dialog is
          Windows' own and the name in it is real.

          The whole path is a tooltip, through `hint`, because a native
          `title=` is the wrong font on the wrong surface a second late.
        -->
        <div class="picked">
          <button type="button" class="pick" onclick={() => void choose(field, key)}>
            {chosen(key).length ? "Change" : "Choose"}
          </button>
          {#each chosen(key) as path (path)}
            <span class="path" use:hint={path}>{leaf(path)}</span>
          {/each}
          {#if chosen(key).length}
            <button type="button" class="pick" onclick={() => (values[key] = [])}>Clear</button>
          {:else}
            <span class="description">Nothing chosen</span>
          {/if}
        </div>
      </div>
    {:else if field.tag === "Form.DatePicker"}
      <div class="row">
        <div class="label">{str(field, "title")}</div>
        <!--
          The type follows what the extension asked for. `Form.DatePicker.Type`
          is `date` or `date_time`, and giving somebody a time to fill in that
          the extension will not read is asking for something twice.
        -->
        <input
          type={field.props.type === "date" ? "date" : "datetime-local"}
          aria-label={str(field, "title")}
          bind:value={values[key] as string}
        />
      </div>
    {:else if field.tag === "Form.TextArea"}
      <div class="row">
        <div class="label">{str(field, "title")}</div>
        <textarea
          rows="4"
          aria-label={str(field, "title")}
          placeholder={str(field, "placeholder")}
          bind:value={values[key] as string}
        ></textarea>
      </div>
    {:else if field.tag === "Form.TextField" || field.tag === "Form.PasswordField"}
      <div class="row">
        <div class="label">{str(field, "title")}</div>
        <input
          type={field.tag === "Form.PasswordField" ? "password" : "text"}
          aria-label={str(field, "title")}
          placeholder={str(field, "placeholder")}
          bind:value={values[key] as string}
        />
      </div>
    {/if}
  {/each}

  <Instead
    tone={standing({ failed: false, loading: false, count: fields.length })}
    headline="This form has no fields"
    hint="The extension declared a form with nothing in it."
  />
</div>

<style>
  .form {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-4) var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  .row {
    display: grid;
    grid-template-columns: 160px 1fr;
    align-items: center;
    gap: var(--space-3);
  }

  .label {
    color: var(--text-2);
    font-size: var(--text-body);
    text-align: right;
  }

  .description {
    color: var(--text-3);
    font-size: var(--text-body);
    line-height: 1.5;
  }

  .separator {
    height: 1px;
    background: var(--hairline);
    margin: var(--space-1) 0;
  }

  input[type="text"],
  input[type="password"],
  textarea,
  select {
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--hairline);
    border-radius: var(--radius-sm);
    background-color: color-mix(in srgb, var(--core-secondary-background) 55%, transparent);
    background-image: var(--sheen);
    color: var(--text-1);
    font-family: inherit;
    font-size: var(--text-body);
    outline: none;
    user-select: text;
  }

  textarea {
    resize: vertical;
    line-height: 1.5;
  }

  /* The focus ring is one of the seven places the accent is allowed. */
  input:focus,
  textarea:focus,
  select:focus {
    border-color: var(--accent-line);
  }

  input::placeholder,
  textarea::placeholder {
    color: var(--text-4);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  /*
   * A chip that is either taken or not, and says which by being filled.
   *
   * Not a bordered chip button, which this project has refused before: the
   * unchosen state is the ordinary fill with no outline, and choosing one
   * fills it with the accent wash a selected row takes. One thing changes.
   */
  .pick {
    padding: var(--space-1) var(--space-2);
    border: none;
    border-radius: var(--radius-pill);
    background-color: var(--fill-2);
    color: var(--text-2);
    font-family: inherit;
    font-size: var(--text-meta);
    line-height: var(--line-meta);
    cursor: default;
    transition: background-color var(--motion-state) var(--ease);
  }

  .pick:hover {
    background-color: var(--fill-3);
  }

  .pick.chosen {
    background-color: var(--accent-fill);
    color: var(--text-1);
  }

  .pick:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  /*
   * The picker's row: the button, then the names, then the way back to none.
   *
   * Wrapping rather than scrolling, because several files chosen at once is
   * the case this exists for and a row that scrolls sideways hides the ones
   * somebody just added.
   */
  .picked {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .path {
    color: var(--text-1);
    font-size: var(--text-meta);
    line-height: var(--line-meta);
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  input[type="date"],
  input[type="datetime-local"] {
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--hairline);
    border-radius: var(--radius-sm);
    background-color: color-mix(in srgb, var(--core-secondary-background) 55%, transparent);
    background-image: var(--sheen);
    color: var(--text-1);
    font-family: inherit;
    font-size: var(--text-body);
    outline: none;
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-1);
    font-size: var(--text-body);
  }

  .checkbox input {
    width: 16px;
    height: 16px;
    accent-color: var(--core-accent);
  }
</style>
