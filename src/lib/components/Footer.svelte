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
  import { keysOf } from "$lib/keys";
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
    /**
     * What Enter does here, or `null` where it does nothing.
     *
     * Named by the page from the mode table rather than guessed here. The
     * guess said "Run" for quitting a program, switching to a window, and in
     * the views where Enter does nothing at all.
     */
    primary?: string | null;
    /**
     * The chord that opens the action panel, as the movement preset has it.
     *
     * Not a fixed Ctrl+K: under the vim preset Ctrl+K moves up, and the pill
     * named a key that did something else.
     */
    actionsChord?: string;
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
    primary = "Open",
    actionsChord = "Ctrl+K",
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
  <!--
    Polite and live on the part that stays, because the line inside it comes
    and goes: a live region that arrives with its text is often not read at
    all. What lands here is an outcome ("Moved report.pdf to the recycle bin",
    an update, a failure), never a keystroke, so it does not chatter.
  -->
  <div class="side" aria-live="polite" aria-relevant="additions text">
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
  </div>

  <!--
    Whatever is pinned, in the middle of the window.

    Always drawn, even with nothing pinned, because it is the middle column of
    the three and a column that comes and going would hand its track to the
    keys and put them where the readings belong.
  -->
  <div class="mid"><WidgetChin {prefs} /></div>

  <div class="keys">
  <!--
    The two things somebody reaches for, standing on the chin rather than
    gathered into a container of their own.

    `tabindex="-1"` and a prevented mousedown on both, because the search
    field must keep document focus. A plain button would take it on click,
    and the arrow keys would stop moving the selection with no visible
    cause.
  -->
  {#if primary}
    <button
      class="segment"
      tabindex="-1"
      onmousedown={(e) => e.preventDefault()}
      onclick={onrun}
    >
      {primary}
      <span class="sill-key">↵</span>
    </button>
  {/if}
  {#if hasActions}
    {#if primary}<span class="split"></span>{/if}
    <button
      class="segment"
      tabindex="-1"
      onmousedown={(e) => e.preventDefault()}
      onclick={onactions}
    >
      Actions
      <span class="sill-key">{keysOf(actionsChord).join(" ")}</span>
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
   *
   * ## Three columns, because the readings belong to the window
   *
   * A flex row with one spacer put the widget strip against the keys, so it
   * was positioned by how wide it happened to be: pinning a second reading
   * moved the first one left, and the strip sat wherever the arithmetic left
   * it rather than anywhere somebody chose.
   *
   * The two outer tracks are equal, so the middle one is centred on the window
   * whatever is in it, and one, two or three readings grow about the centre
   * instead of away from the right edge. `minmax(0, ...)` on the left track is
   * what lets a long status line shrink; without the 0 its minimum is its own
   * sentence and prose would push the centre off it.
   *
   * The right track floors at `max-content` instead, because the keys are the
   * one thing here that cannot give: a key with its tail cut off is not a
   * shorter key. While there is room the floor is slack and both tracks
   * resolve to the same `1fr`, so the centre is a real centre. When a wide
   * strip finally takes that room, the floor binds and the middle slides right
   * rather than the keys sliding out from under their own track and printing
   * over the readings.
   */
  footer {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(max-content, 1fr);
    align-items: center;
    gap: var(--space-2);
    flex: none;
    height: var(--chin-height);
    padding: 0 var(--space-2);
    background: var(--chin);
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  /* The two outer tracks. `min-width: 0` on both, so the shrinking the grid
     allows actually reaches the toast inside. */
  .side,
  .keys {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  /* Against the window's edge, which is where the action panel rises from. */
  .keys {
    justify-content: flex-end;
  }

  .mid {
    display: flex;
    justify-content: center;
    min-width: 0;
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
  /*
   * The two keys, each standing on the chin.
   *
   * No container around them. A raised cluster reads as one control with two
   * halves, which is wrong for two separate things: Enter runs what is
   * selected and Ctrl K opens a menu about it, and they are related the way
   * neighbours are rather than the way a switch's two positions are. The
   * separator stays, because that relation still wants saying.
   *
   * Each carries its own radius now. Inside the pill the hover fill was
   * clipped to the cluster by `overflow: hidden`, and without a shape of its
   * own a hovered segment would paint a bare rectangle on the chin.
   */
  .segment {
    display: flex;
    align-items: center;
    flex: none;
    gap: var(--space-2);
    height: var(--control-height);
    border-radius: var(--radius-md);
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
   * `min-width: 0` is what lets it give. Its track is `minmax(0, 1fr)` and can
   * shrink to nothing, but a flex item inside a track that can shrink still
   * refuses to be narrower than its own sentence, so without this the sentence
   * runs out of the track and over the readings in the middle of the chin.
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
