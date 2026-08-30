<script lang="ts">
  import type { ElementNode, ViewTree } from "$lib/exthost/tree";

  interface Props {
    tree: ViewTree;
    node: ElementNode;
    version: number;
    /** Called with the collected values, keyed by each field's id. */
    onsubmit: (values: Record<string, unknown>) => void;
  }

  let { tree, node, version, onsubmit }: Props = $props();

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
      } else if (field.tag === "Form.TagPicker") {
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
        <select bind:value={values[key] as string}>
          {#each dropdownOptions(field) as option (option.value)}
            <option value={option.value}>{option.title}</option>
          {/each}
        </select>
      </div>
    {:else if field.tag === "Form.TextArea"}
      <div class="row">
        <div class="label">{str(field, "title")}</div>
        <textarea
          rows="4"
          placeholder={str(field, "placeholder")}
          bind:value={values[key] as string}
        ></textarea>
      </div>
    {:else if field.tag === "Form.TextField" || field.tag === "Form.PasswordField"}
      <div class="row">
        <div class="label">{str(field, "title")}</div>
        <input
          type={field.tag === "Form.PasswordField" ? "password" : "text"}
          placeholder={str(field, "placeholder")}
          bind:value={values[key] as string}
        />
      </div>
    {/if}
  {/each}

  {#if fields.length === 0}
    <div class="sill-empty">
      <img src="/sill.png" alt="" width="32" height="32" draggable="false" />
      <span class="headline">This form has no fields</span>
      <span class="hint">The extension declared a form with nothing in it.</span>
    </div>
  {/if}
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
