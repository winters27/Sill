<script lang="ts">
  interface Props {
    terms: string[];
    onchange: (terms: string[]) => void;
  }

  let { terms = $bindable(), onchange }: Props = $props();

  let draft = $state("");

  function add() {
    const value = draft.trim();
    // A blank term matches every string, so it would empty the list the moment
    // someone opened the editor and did not type.
    if (!value || terms.includes(value)) {
      draft = "";
      return;
    }

    const next = [...terms, value];
    terms = next;
    draft = "";
    onchange(next);
  }

  function remove(term: string) {
    const next = terms.filter((t) => t !== term);
    terms = next;
    onchange(next);
  }
</script>

<div class="list">
  {#if terms.length}
    <div class="chips">
      {#each terms as term (term)}
        <span class="chip">
          {term}
          <button aria-label="Stop hiding {term}" onclick={() => remove(term)}>
            <svg width="10" height="10" viewBox="0 0 12 12" aria-hidden="true">
              <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            </svg>
          </button>
        </span>
      {/each}
    </div>
  {/if}

  <input
    bind:value={draft}
    placeholder="A name or folder to hide, then Enter"
    spellcheck="false"
    onkeydown={(e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        add();
      }
    }}
    onblur={add}
  />
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 5px 4px 10px;
    border-radius: 999px;
    background: rgba(var(--accent-rgb), 0.12);
    font-size: 12px;
    color: var(--core-foreground);
  }

  .chip button {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    transition: background-color 0.15s var(--ease), color 0.15s var(--ease);
  }

  .chip button:hover {
    background: rgba(var(--accent-red-rgb), 0.2);
    color: var(--accent-red);
  }

  input {
    width: 100%;
    max-width: 340px;
    padding: 7px 11px;
    border: 0;
    border-radius: var(--radius-sm);
    background: rgba(var(--accent-rgb), 0.05);
    box-shadow: inset 0 0 0 1px var(--hairline);
    color: var(--core-foreground);
    font: inherit;
    font-size: 12.5px;
    outline: none;
    user-select: text;
    transition: box-shadow 0.15s var(--ease);
  }

  input:focus {
    box-shadow: inset 0 0 0 1px var(--border-light);
  }

  input::placeholder {
    color: var(--text-faint);
  }
</style>
