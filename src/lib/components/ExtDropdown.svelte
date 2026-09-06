<script lang="ts">
  /**
   * The picker an extension puts beside the search field.
   *
   * Raycast calls it `searchBarAccessory` and it is how a list says "these
   * rows are one of several sets". Hacker News offers fifteen feeds through
   * one of these and Kill Process offers two sort orders; neither reached the
   * screen, so both commands were stuck on whatever their extension happened
   * to default to with nothing on screen admitting it.
   *
   * ## Why a native select
   *
   * The launcher's search row already holds the focus and the arrow keys walk
   * the list underneath it. A popover of Sill's own would be a second thing
   * competing for both, and Escape would have three meanings on one screen.
   * The native control opens on its own layer, takes the keys only while it is
   * open, and gives them straight back. `FormView` reached the same conclusion
   * for the same reason and this wears the same clothes.
   *
   * ## Where the chosen value lives
   *
   * Here, seeded from the extension and re-seeded whenever the extension says
   * something different. A dropdown in Raycast is semi-controlled: the
   * extension may pass `value` and drive it, or pass `defaultValue` and let
   * the launcher hold it. Holding it here and yielding to `value` when it
   * arrives serves both without asking the extension which it is.
   *
   * ## Why it speaks before anybody touches it
   *
   * A picker the launcher holds knows something the extension does not: what
   * it opened on. Raycast tells the extension that, and extensions are
   * written expecting to hear it. Hacker News is the plain case, and it is
   * the shape half the store uses:
   *
   *     const [topic, setTopic] = useState(null);
   *     usePromise(getStories, [topic], { execute: !!topic });
   *     <List.Dropdown defaultValue={FrontPage} onChange={setTopic}>
   *
   * Nothing fetches until the dropdown reports a selection. Reporting only
   * what a person picked left that extension drawing its picker, its fifteen
   * feeds and an empty list forever, with no error anywhere: it had not
   * failed, it was still waiting to be told which feed it was on.
   */
  import type { Dropdown } from "$lib/exthost/present";
  import { hint } from "$lib/hint";

  interface Props {
    dropdown: Dropdown;
    /** Called with the chosen value, which the page relays to the extension. */
    onpick: (value: string) => void;
  }

  let { dropdown, onpick }: Props = $props();

  /** What is chosen, once anybody has chosen. */
  let picked = $state<string | undefined>(undefined);

  /**
   * Which picker `picked` belongs to.
   *
   * A command that swaps its dropdown, or a different command entirely, must
   * not inherit a value chosen in the last one. Without this the second list
   * would open showing a selection that means nothing in it.
   */
  let mine = $state(-1);

  $effect(() => {
    if (mine === dropdown.id) return;
    mine = dropdown.id;
    picked = undefined;
  });

  /**
   * The value the control shows.
   *
   * The extension's own `value` wins while it is passing one, because an
   * extension that drives its dropdown expects the screen to agree with it.
   * Otherwise whatever was chosen here, then the starting point the extension
   * suggested, and otherwise the first option, which is what a select with no
   * value would land on anyway.
   */
  const showing = $derived(
    dropdown.value ?? picked ?? dropdown.initial ?? dropdown.options[0]?.value ?? "",
  );

  /**
   * What the extension has already been told, so it is told once.
   *
   * A plain variable rather than state: nothing draws from it, and a reactive
   * one would re-run the effect that writes it. The value is kept beside the
   * id because that is what makes this safe against a picker being rebuilt: a
   * new node carrying a selection the extension has already been given is not
   * news, and announcing it again is how a re-render becomes a loop.
   */
  let announced: { id: number; value: string } | null = null;

  $effect(() => {
    const id = dropdown.id;
    const opening = showing;

    // Nothing to say: the extension is driving this, or there is nothing to
    // choose from yet, or it has already heard this.
    if (dropdown.value !== undefined) return;
    if (!opening || dropdown.options.length === 0) return;
    if (announced && announced.id === id && announced.value === opening) return;

    announced = { id, value: opening };
    onpick(opening);
  });

  /** The options, grouped the way the extension grouped them. */
  const groups = $derived.by(() => {
    const out: { section?: string; items: Dropdown["options"] }[] = [];
    for (const option of dropdown.options) {
      const last = out[out.length - 1];
      if (last && last.section === option.section) last.items.push(option);
      else out.push({ section: option.section, items: [option] });
    }
    return out;
  });
</script>

<select
  class="ext-dropdown"
  aria-label={dropdown.tooltip || "Filter results"}
  value={showing}
  use:hint={dropdown.tooltip || undefined}
  onchange={(event) => {
    const value = event.currentTarget.value;
    picked = value;
    // Written down here as well, or the effect above would see a value it has
    // not announced and say it a second time.
    announced = { id: dropdown.id, value };
    onpick(value);
  }}
>
  {#each groups as group, at (at)}
    {#if group.section}
      <optgroup label={group.section}>
        {#each group.items as option (option.value)}
          <option value={option.value}>{option.title}</option>
        {/each}
      </optgroup>
    {:else}
      {#each group.items as option (option.value)}
        <option value={option.value}>{option.title}</option>
      {/each}
    {/if}
  {/each}
</select>

<style>
  /*
   * A chip in the search row, sized to what it holds.
   *
   * The same shape the AI chip beside it takes, because they sit on one line
   * and two different pills in one row is two designs for one place. Bounded
   * width: a dropdown of feed names must not push the field somebody is
   * typing into off the edge of the window.
   */
  .ext-dropdown {
    flex: none;
    max-width: 12rem;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: 1px solid var(--hairline);
    border-radius: var(--radius-sm);
    background-color: var(--fill-1);
    background-image: var(--sheen);
    color: var(--text-2);
    font-family: inherit;
    font-size: var(--text-meta);
    outline: none;
  }

  .ext-dropdown:hover {
    color: var(--text-1);
  }

  /* The focus ring is one of the places the accent is allowed. */
  .ext-dropdown:focus-visible {
    border-color: var(--accent-line);
  }
</style>
