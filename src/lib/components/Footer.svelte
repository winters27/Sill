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
  import { shortcutKeys, type ActionEntry } from "$lib/exthost/actions";
  import type { Mode } from "$lib/modes";
  import type { Preferences } from "$lib/settings";

  interface Props {
    mode: Mode;
    /**
     * An extension's toast, which takes the line while it is up.
     *
     * `actions` are the buttons it put on itself. They are already
     * `ActionEntry` values because pressing one is the same thing as pressing
     * a row in the action panel, and this reports the press rather than
     * deciding what it means, exactly as every other control here does.
     */
    toast: { title: string; style: string; actions: ActionEntry[] } | null;
    /** What the launcher itself last had to say. */
    status: string;
    /**
     * A newer Sill, when there is one and there is something to press.
     *
     * Already reduced to words and a button label by `chinLine`, so this
     * component holds no opinion about which states deserve a line. Most do
     * not: being current is not news and a failed check belongs in settings,
     * and both arrive here as `null`.
     *
     * One half or the other is null too. A state with something to press says
     * it in the button and says it once.
     */
    update: { words: string | null; button: string | null } | null;
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
    /** A button on the toast was pressed. */
    ontoastaction: (action: ActionEntry) => void;
    /** The update button was pressed. */
    onupdate: () => void;
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
    ontoastaction,
    update,
    onupdate,
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
    <!--
      The buttons the extension put on its own message.

      Beside the words rather than under them, because the toast is one line in
      a chin that has no room for a second. `onmousedown` is prevented for the
      reason the pill's are: the search field must keep document focus, and a
      plain button takes it.

      Keyed by the handler id, which is the one thing about a toast button that
      is unique and does not change while it is on screen. Keying on the title
      would blank the row the moment an extension offered two buttons that said
      the same word.
    -->
    {#each toast.actions as action (action.handler)}
      <button
        type="button"
        class="toast-action"
        tabindex="-1"
        onmousedown={(e) => e.preventDefault()}
        onclick={() => ontoastaction(action)}
      >
        {action.title}
        {#if action.shortcut}
          {#each shortcutKeys(action.shortcut) as key (key)}
            <span class="sill-key">{key}</span>
          {/each}
        {/if}
      </button>
    {/each}
  {:else if status}
    <span class="toast">{status}</span>
  {:else if update}
    <!--
      Third in line, behind the toast and the status.

      Both of those are about what the person is doing this second. A newer
      Sill is about the application and can wait for the line to be free. It
      is held in Rust rather than here, so it returns when the line frees up
      rather than being lost.

      One half or the other, never both: an update that can be pressed says
      so in the button and nowhere else. The words and a button together are
      wider than what is left of this row once the readings and the keys have
      taken theirs, and prose is what gives way, so the sentence arrived
      clipped mid-word.
    -->
    {#if update.button}
      <button
        type="button"
        class="update"
        tabindex="-1"
        onmousedown={(e) => e.preventDefault()}
        onclick={onupdate}
      >
        {update.button}
      </button>
    {:else if update.words}
      <!--
        The same pill, not pressable.

        A download in flight has nothing to offer, because a second press
        would start a second download. It keeps the shape so the row does not
        change size under the cursor at the moment somebody presses it.
      -->
      <span class="update">{update.words}</span>
    {/if}
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

  /* The line an error lands on. One line, and it stays one line: a path or
     a stack frame here used to push the action pill off the window, and a
     wrapped one painted over the last row. The full text still reaches the
     log and the settings window's trouble list. */
  .toast {
    flex: 0 1 auto;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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

  /*
 * The status line, which is the only thing down here made of prose.
 *
 * It had no rule at all, so it was a flex item at its natural width with the
 * default `min-width: auto`, which means it could not be made narrower than
 * its own sentence. A long one ate the spacer, and then the only item in the
 * row that *could* give was the widget chin, which is set to hide what does
 * not fit: "Hacker News stopped: the worker exited" cut the clock and the
 * weather in half against the edge of the window.
 *
 * Prose is the right thing to shorten. It is the one item here that still
 * says something with its tail missing, and the ellipsis says a tail is
 * missing; half a temperature reading looks like a different temperature.
 */
  .toast {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /*
   * A newer Sill, which is the one thing down here that is neither a reply to
   * what was just pressed nor a key that is always on the row.
   *
   * The action pill's shape, in lit glass rather than in the row's own grey.
   * Everything else down here is either answering a press or is a key that is
   * always there, and both can afford to be found only when looked for. This
   * one has to be noticed by somebody who came to type something else, on the
   * quietest row in the window, so it is the one place `--info-lit` is used:
   * a tinted face, a catch along its top edge and a short bloom off it.
   */
  .update {
    display: flex;
    align-items: center;
    flex: none;
    height: var(--control-height);
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-lg);
    /* The face, and the light lying across it. `--sheen` is the gradient the
       window's own glass uses, so the pill is lit from the same direction as
       everything it sits on. */
    background-color: var(--info-fill);
    background-image: var(--sheen);
    box-shadow: var(--info-lit);
    color: var(--info);
    font: inherit;
    font-size: var(--text-meta);
    white-space: nowrap;
    cursor: default;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  /*
   * Only the pressable one answers the cursor, and it answers by lighting
   * further rather than by turning into something else.
   *
   * The one hand cursor in this row. Everything else down here is the label of
   * a key that does the same thing, and a hand over "Open ↵" would be offering
   * the mouse as the way to do what the key beside it just told you to press.
   * This has no key and no place in the tab order, so the pointer is the only
   * way in and the cursor should say so.
   */
  button.update {
    cursor: pointer;
  }

  button.update:hover {
    background-color: var(--info-fill-strong);
    color: var(--text-1);
  }

  .spacer {
    flex: 1;
  }

  /*
   * A toast's own button, which is the quietest control in the chin.
   *
   * No border. A bordered chip is the shape this project has refused before,
   * and this sits next to a coloured line of text where an outline would read
   * as a second message rather than as something to press. The fill arrives on
   * hover, which is the same thing the pill's segments do.
   */
  .toast-action {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex: none;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
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

  .toast-action:hover {
    background-color: var(--fill-2);
    color: var(--text-1);
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
  /* The same blue as an extension's own running message. A newer Sill is
     information: not a success, and not a failure. */
  .toast[data-style="info"] {
    color: var(--info);
  }
</style>
