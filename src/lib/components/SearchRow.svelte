<script lang="ts">
  /**
   * The query row: where you are, what you are typing, and who would answer.
   *
   * ## Why the whole row is one component
   *
   * The three things in it are one thing being read. The crumb says which of
   * the launcher's faces is on screen, the placeholder says what the field is
   * for on that face, and the chip says what Tab would do with it. Every one
   * of those is a sentence about the mode, and splitting them put three
   * hand-written mode lists in three places with nothing making them agree.
   *
   * Presentation only, apart from the field itself. The text and the element
   * are bound back out, because the launcher has to be able to put the caret
   * where a keystroke that arrived before focus did would have left it.
   */
  import AiMark from "$lib/components/settings/AiMark.svelte";
  import ExtDropdown from "$lib/components/ExtDropdown.svelte";
  import { LISTBOX, optionId } from "$lib/results";
  import { hint } from "$lib/hint";
  import { openSettings } from "$lib/settings";
  import type { Mode } from "$lib/modes";
  import type { AiReady } from "$lib/exthost/commands";
  import type { dropdownOf } from "$lib/exthost/present";

  interface Props {
    mode: Mode;
    /** What is typed, which the launcher owns and this collects. */
    query: string;
    /**
     * The field itself.
     *
     * Bound out because focus, the caret and the selection are all the
     * launcher's business: a character that arrives before the field has
     * focus is placed where typing a moment later would have put it.
     */
    field: HTMLInputElement | null;
    /** Which row is highlighted, for the combobox to point at. */
    selected: number;
    /** Whether what is on screen is a list this field is filtering. */
    browsing: boolean;
    /** What the field is being borrowed for, when it is. */
    awaitingTitle: string | undefined;
    /** The script whose output is on screen. */
    outputTitle: string | undefined;
    /** What is being moved, while somewhere is picked for it. */
    movingTitle: string | undefined;
    /** The result being given a name. */
    namingTitle: string | undefined;
    /** The extension whose command is running. */
    runningTitle: string | undefined;
    /** Who answers, for the crumb and the chip. */
    answersWith: AiReady | null;
    /** Whether a question is in flight. */
    asking: boolean;
    /** Whether the conversation has nothing in it yet. */
    conversationEmpty: boolean;
    /** The placeholder a running command asked for. */
    commandPlaceholder: string;
    /** The set of rows the command is showing, when it offers a choice. */
    dropdown: ReturnType<typeof dropdownOf> | undefined;
    onpick: (value: string) => void;
    /**
     * A person changed what is in the field.
     *
     * Not the same event as the query changing, which is why it is reported
     * separately. The launcher sets the query itself in a dozen places, an
     * extension can set it, and history walks it; none of those is somebody
     * waiting for an answer, and counting them as keystrokes would put a
     * programmatic clear into a measurement of how fast typing feels.
     */
    ontyped?: () => void;
  }

  let {
    mode,
    query = $bindable(),
    field = $bindable(),
    selected,
    browsing,
    awaitingTitle,
    outputTitle,
    movingTitle,
    namingTitle,
    runningTitle,
    answersWith,
    asking,
    conversationEmpty,
    commandPlaceholder,
    dropdown,
    onpick,
    ontyped,
  }: Props = $props();

  /**
   * What the chip says when it is hovered.
   *
   * The local case earns its own sentence. Whether a question costs money and
   * whether it leaves the machine is the one thing about a provider worth
   * knowing before pressing the key, and it is the only thing the mark itself
   * cannot say.
   */
  const askingIs = $derived.by(() => {
    if (!answersWith) return "";
    if (!answersWith.ready) return answersWith.whyNot;

    return answersWith.kind === "local"
      ? `${answersWith.name} answers, on this machine`
      : `${answersWith.name} answers when you press Tab`;
  });
</script>

<div class="search">
  <img class="mark" src="/sill.png" alt="" width="26" height="26" draggable="false" />
  {#if mode === "argument" && awaitingTitle !== undefined}
    <span class="crumb">{awaitingTitle}</span>
  {:else if mode === "output" && outputTitle !== undefined}
    <span class="crumb">{outputTitle}</span>
  {:else if mode === "clipboard"}
    <span class="crumb">Clipboard History</span>
  {:else if mode === "switcher"}
    <span class="crumb">Open Windows</span>
  {:else if mode === "emoji"}
    <span class="crumb">Emoji</span>
  {:else if mode === "keys"}
    <span class="crumb">Keyboard</span>
  {:else if mode === "welcome"}
    <span class="crumb">Welcome</span>
  {:else if mode === "ai"}
    <!--
      Who is answering, in the place that says where you are.

      In every other mode the crumb names the surface, because the surface is
      the thing you are in. Here the thing you are in is a conversation with
      a particular model, and a bare name said that a launcher feature was open
      without saying the one fact that changes what comes back. The mark also
      does the work no label was doing: it is unmistakably a conversation
      with something rather than another list.

      Still a button, and the same button as the chip in the root list, so
      changing model is in one place whichever end you reach it from.
    -->
    {#if answersWith?.ready}
      <button
        class="crumb who-crumb"
        onclick={() => void openSettings("ai")}
        use:hint={askingIs}
      >
        <AiMark name={answersWith.id} size={13} />
        <span class="who">{answersWith.model || answersWith.name}</span>
      </button>
    {:else}
      <span class="crumb">AI Chat</span>
    {/if}
  {:else if mode === "conversations"}
    <span class="crumb">Conversations</span>
  {:else if mode === "appVolume"}
    <span class="crumb">App Volume</span>
  {:else if mode === "processes"}
    <span class="crumb">Processes</span>
  {:else if mode === "widgets"}
    <span class="crumb">Widgets</span>
  {:else if mode === "namingWorkspace"}
    <span class="crumb">Save workspace</span>
  {:else if mode === "store"}
    <span class="crumb">Extension Store</span>
  {:else if mode === "destination" && movingTitle !== undefined}
    <span class="crumb">Move {movingTitle}</span>
  {:else if mode === "collection"}
    <span class="crumb">Collection</span>
  {:else if mode === "alias" && namingTitle !== undefined}
    <span class="crumb">{namingTitle}</span>
  {:else if runningTitle !== undefined}
    <span class="crumb">{runningTitle}</span>
  {/if}
  <!--
    A combobox, which is what this is: a field whose typing filters a list
    below it, where the list is walked with the arrow keys while the field
    keeps focus.

    Without this a screen reader announces the field and then says nothing
    as somebody arrows through the results, because focus never moves and
    nothing tells it what is highlighted. The listbox half of the pattern
    was already there; this is the half that makes it audible.
  -->
  <input
    role={browsing ? "combobox" : undefined}
    aria-expanded={browsing ? true : undefined}
    aria-controls={browsing ? LISTBOX : undefined}
    aria-activedescendant={browsing ? optionId(selected) : undefined}
    aria-autocomplete={browsing ? "list" : undefined}
    aria-label="Search"
    bind:this={field}
    bind:value={query}
    oninput={ontyped}
    placeholder={mode === "argument"
      ? "Type what to search for, then Enter…"
      : mode === "emoji"
        ? "Search emoji by name…"
      : mode === "ai"
        ? asking
          ? "Waiting for the answer…"
          : conversationEmpty
            ? "Ask anything…"
            : "Ask a follow-up…"
      : mode === "conversations"
        ? "Filter what you have asked…"
      : mode === "appVolume"
        ? "Filter by program name…"
      : mode === "processes"
        ? "Filter what is running…"
      : mode === "widgets"
        ? "Esc to go back…"
      : mode === "welcome"
        ? "Esc to start searching…"
      : mode === "namingWorkspace"
        ? "Name this arrangement, then Enter…"
      : mode === "store"
        ? "Search the extension store…"
      : mode === "destination"
        ? "Search for a folder, then Enter…"
        : mode === "alias"
          ? "Type a short name, then Enter. Empty forgets it…"
        : mode === "collection"
          ? "Name the collection, then Enter…"
        : mode === "clipboard"
          ? "Filter what you have copied…"
        : mode === "switcher"
          ? "Switch to a window…"
          : mode === "root"
          ? "Search for apps and commands…"
          : commandPlaceholder}
    spellcheck="false"
    autocomplete="off"
  />

  <!--
    Who is about to answer, and the key that asks them.

    The only place anybody discovers that Tab does anything at all, which is
    why it is drawn even when nothing is set up: an invitation reads better
    than an empty corner. A button, so changing the model is two clicks from
    the thing you were about to ask rather than a trip through Settings.

    Only in the root list. In a conversation the crumb already says Ask, and
    in the clipboard or the switcher Tab is not free to ask anything.
  -->
  <!--
    The set of rows the command is showing, when it offers a choice of them.
    Beside the field, which is where Raycast puts it and where it belongs:
    it narrows the same list the field narrows.
  -->
  {#if dropdown}
    <ExtDropdown {dropdown} {onpick} />
  {/if}

  {#if mode === "root" && answersWith}
    <button
      class="asker"
      class:unset={!answersWith.ready}
      onclick={() => void openSettings("ai")}
      use:hint={askingIs}
    >
      {#if answersWith.ready}
        <!--
          The service as its own mark, and then only the model.

          Two names in a chip this size is the service said twice: the mark
          already carries it, and the model is the half that changes. The
          model is shortened in Rust, so this and the settings window agree
          about what it is called.
        -->
        <!-- The mark and the model are one thing being said, so they are
             grouped and sit closer to each other than to the key. -->
        <span class="whom">
          <AiMark name={answersWith.id} size={14} />
          <span class="who">{answersWith.model || answersWith.name}</span>
        </span>
        <!-- Revealed only once there is something to ask about, so an empty
             launcher is not carrying a key nobody can use yet. -->
        {#if query.trim()}
          <span class="sill-key">Tab</span>
        {/if}
      {:else}
        <span class="who">Set up AI Chat</span>
      {/if}
    </button>
  {/if}
</div>

<div class="divider"></div>

<style>
  /*
   * 60px, and stated rather than left to the input's line box.
   *
   * The window's corner radius is fixed at 8px by DWM, so the launcher cannot
   * be made to feel less boxy at the edges. It can be made to feel less
   * cramped inside, and the query row is where that reads first: this is the
   * one element somebody looks at before anything has been typed.
   */
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--search-height);
    /*
     * The same room at both ends.
     *
     * There was none on the right, because until the chip arrived nothing was
     * over there: the field simply ran to the edge, where a text caret needs
     * no margin. A pill does, and without this it sat against the glass while
     * the mark on the left had a comfortable inset, which reads as the row
     * being pushed sideways rather than as one element being tight.
     */
    padding-left: var(--space-4);
    padding-right: var(--space-4);
    flex: none;
  }

  /* The mark stands where a magnifier would, so the window is identifiable
     the moment it appears rather than only from its contents.

     The app icon itself, at the size it is drawn everywhere else. There is
     no separate in-app mark any more: the art lost its plaque, so the thing
     on the taskbar is already the right thing to put here. */
  .mark {
    flex: none;
    width: var(--icon-tile);
    height: var(--icon-tile);
    -webkit-user-drag: none;
  }

  /* A chip, not a tile. The sheen-and-bevel recipe belongs to something that
     reads as a raised object; this is a label saying where you are. */
  .crumb {
    flex: none;
    /* An extension's or a file's name, so it is bounded the way `.who`
       below is; otherwise a long one squeezes the field to nothing. */
    max-width: 22ch;
    overflow: hidden;
    text-overflow: ellipsis;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    color: var(--text-2);
    font-size: var(--text-meta);
    white-space: nowrap;
  }

  /* The one crumb that is pressable, so it says so on hover rather than only
     when the pointer is already on it. */
  .who-crumb {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    border: 0;
    padding-left: var(--space-1);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .who-crumb:hover {
    background: var(--hairline-strong);
    color: var(--text-1);
  }

  .who-crumb:focus-visible {
    outline: none;
    box-shadow: var(--ring-accent);
  }

  .search input {
    flex: 1;
    min-width: 0;
    padding: 0 var(--space-3) 0 0;
    border: 0;
    background: transparent;
    color: var(--text-1);
    /* Segoe has a separate cut for text this size; Inter resolves this back
       to itself. */
    font-family: var(--font-display);
    font-size: var(--text-query);
    font-weight: 400;
    /* Large text wants a touch of negative tracking; at 17px the default
       spacing reads loose next to a 13px list. */
    letter-spacing: var(--track-tight);
    outline: none;
    user-select: text;
  }

  .search input::placeholder {
    color: var(--text-4);
  }

  .divider {
    flex: none;
    height: 1px;
    background: var(--hairline);
  }

  /*
   * The chip at the end of the field.
   *
   * Quiet by default: it is a label that happens to be pressable, not a call
   * to action competing with what somebody is typing. It brightens on hover
   * and takes the accent only when there is nothing set up, which is the one
   * state that is asking to be pressed.
   */
  .asker {
    display: inline-flex;
    align-items: center;
    flex: none;
    gap: var(--space-2);
    padding: var(--space-snug) var(--space-2) var(--space-snug) var(--space-1);
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .asker:hover {
    background: var(--fill-2);
    color: var(--text-1);
  }

  .asker:focus-visible {
    outline: none;
    box-shadow: var(--ring-accent);
  }

  .asker.unset {
    background: var(--accent-fill);
    box-shadow: none;
    color: var(--accent);
  }

  /* Who is answering: the mark and the model, held together. */
  .whom {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }

  .who {
    max-width: 22ch;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
