<script lang="ts">
  /**
   * The bar along the bottom: the menu, what just happened, and the two keys.
   *
   * Presentation only. Every control here reports what was pressed and the
   * launcher decides what it means, because each of them is the same thing a
   * key already does: the pill's left half is Enter, its right half is the
   * action chord, and the menu's rows are commands the root list also holds.
   * A second opinion about any of those is the shape this codebase has been
   * bitten by repeatedly.
   */
  import LauncherMenu from "$lib/components/LauncherMenu.svelte";
  import WidgetChin from "$lib/widgets/Chin.svelte";
  import type { Mode } from "$lib/modes";
  import type { Preferences } from "$lib/settings";

  interface Props {
    mode: Mode;
    /** An extension's toast, which takes the line while it is up. */
    toast: { title: string; style: string } | null;
    /** What the launcher itself last had to say. */
    status: string;
    prefs: Preferences | null;
    /** The tag of a running command's view, so a form says Submit. */
    viewTag: string | undefined;
    /** Whether there is anything behind the action chord to offer. */
    hasActions: boolean;
    /** A builtin chosen from the launcher menu. */
    onbuiltin: (id: string) => void;
    /** The primary half of the pill, which is Enter. */
    onrun: () => void;
    /** The other half, which is the action chord. */
    onactions: () => void;
  }

  let {
    mode,
    toast,
    status,
    prefs,
    viewTag,
    hasActions,
    onbuiltin,
    onrun,
    onactions,
  }: Props = $props();
</script>

<!--
  No divider above the footer.

  The window already carries one under the search field, and the raised
  pill below is its own edge. A second full-width rule turned the quietest
  part of the window into a boxed-in strip.
-->
<footer>
  <LauncherMenu {onbuiltin} />
  {#if toast}
    <span class="toast" data-style={toast.style}>{toast.title}</span>
  {:else if status}
    <span class="toast">{status}</span>
  {/if}
  <span class="spacer"></span>

  <!-- Whatever is pinned, sitting in what was empty space between the
       status and the keys. -->
  <WidgetChin {prefs} />

  <!-- Escape sits outside the pill and stays plain, so the pill holds
       exactly the two things somebody reaches for. -->
  <span class="escape">
    {mode === "root" ? "Close" : "Back"}
    <span class="esc-key">Esc</span>
  </span>

  <!--
    The action pill.

    `tabindex="-1"` and a prevented mousedown on both segments, because the
    search field must keep document focus. A plain button would take it on
    click, and the arrow keys would stop moving the selection with no
    visible cause.
  -->
  <div class="pill">
    <button
      class="segment"
      tabindex="-1"
      onmousedown={(e) => e.preventDefault()}
      onclick={onrun}
    >
      {mode === "clipboard" ? "Paste" : mode === "root" ? "Open" : viewTag === "Form" ? "Submit" : "Run"}
      <span class="sill-key">↵</span>
    </button>
    {#if hasActions}
      <span class="split"></span>
      <button
        class="segment"
        tabindex="-1"
        onmousedown={(e) => e.preventDefault()}
        onclick={onactions}
      >
        Actions
        <span class="sill-key">Ctrl K</span>
      </button>
    {/if}
  </div>
</footer>

<style>
  /*
   * The chin: a plane the two controls sit on, back in flow.
   *
   * It briefly had no surface and let the list dissolve underneath it, which
   * was an attempt to get a blurred chin without an opaque window. That cannot
   * work; see the note on `--chin` in theme.css. A plain recessed wash is what
   * is left, and it is honest about being a bar.
   *
   * 8px of side padding puts the pill on the same right edge as the action
   * panel that rises out of it.
   */
  footer {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
    height: var(--chin-height);
    padding: 0 var(--space-2);
    background: var(--chin);
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  /* Outside the pill and quieter than it. Escape is the key nobody needs
     reminding of, so it does not get to sit in the affordance. */
  .escape {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--text-4);
  }

  .esc-key {
    font-weight: var(--weight-medium);
  }

  /*
   * The action pill.
   *
   * One raised cluster holding the primary action and the action menu, which
   * is the shape every launcher uses and the thing Sill's flat row of five
   * faint hints was standing in for. The bevel is the tile recipe: unlike the
   * window, this sits ON a surface, so an outer edge has something to fall on.
   */
  /* Lifted off the chin, which is a known background again. */
  .pill {
    display: flex;
    align-items: center;
    flex: none;
    height: var(--control-height);
    border-radius: var(--radius-lg);
    background: var(--fill-2);
    box-shadow: var(--bevel-tile);
    overflow: hidden;
  }

  .segment {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 100%;
    padding: 0 var(--space-2);
    border: 0;
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    white-space: nowrap;
    cursor: default;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .segment:hover {
    background-color: var(--fill-2);
    color: var(--text-1);
  }

  .split {
    width: 1px;
    height: 16px;
    flex: none;
    background: var(--hairline-strong);
  }

  .spacer {
    flex: 1;
  }

  .toast[data-style="success"] {
    color: var(--success);
  }
  .toast[data-style="failure"] {
    color: var(--danger);
  }
  .toast[data-style="animated"] {
    color: var(--info);
  }
</style>
