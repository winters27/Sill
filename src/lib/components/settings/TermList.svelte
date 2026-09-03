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
    aria-label="A name or folder to hide"
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
    gap: var(--space-2);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    font-size: var(--text-meta);
    color: var(--text-1);
  }

  .chip button {
    display: grid;
    place-items: center;
    width: var(--icon-tile-xs);
    height: var(--icon-tile-xs);
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .chip button:hover {
    background: var(--danger-fill);
    color: var(--danger);
  }

  input {
    width: 100%;
    max-width: 340px;
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
    transition: box-shadow var(--motion-state) var(--ease);
  }

  input:focus {
    box-shadow: var(--ring-strong);
  }

  input::placeholder {
    color: var(--text-3);
  }
</style>
