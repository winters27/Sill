<script lang="ts">
  import type { RankedCommand } from "$lib/exthost/commands";
  import { LISTBOX, optionId } from "$lib/results";
  import { groupOf, linesOf, scrollFor, type Line } from "$lib/list";
  import LaunchIcon from "./LaunchIcon.svelte";
  import SettingsIcon, { type IconName } from "./SettingsIcon.svelte";

  interface Props {
    commands: RankedCommand[];
    /**
     * Subtitles that are a measurement rather than a description.
     *
     * By id, and absent for nearly every row. Kept out of `commands` itself
     * because that list is replaced on every search, and a subtitle patched
     * into it would be written over by the next keystroke.
     */
    live?: Record<string, string>;
    selected: number;
    onselect: (index: number) => void;
    onrun: (index: number) => void;
    /**
     * Changes when the list is answering a different question.
     *
     * The query, in practice. A new search starts at the top, and without
     * being told when that has happened the list stays wherever it was left,
     * which reads as results failing to appear until you scroll back up.
     *
     * Not the results themselves: those arrive in several parts as windows,
     * emoji and files come in, and jumping to the top for each would yank the
     * list out from under somebody who had started scrolling.
     */
    asking?: string;
    /**
     * Whether Ctrl and a digit jumps to a row, from `navigation.numeric`.
     *
     * Only used to decide whether to draw the hint. The binding itself has
     * always existed and is handled in `+page.svelte`; it was simply never
     * shown, so nobody could discover it.
     */
    numeric?: boolean;
  }

  let { commands, selected, onselect, onrun, live, asking = "", numeric = false }: Props =
    $props();



  /**
   * Row and header heights, measured rather than assumed.
   *
   * Hardcoding was wrong twice over: `box-sizing: border-box` is global, so a
   * row with `height: 38px` and a 1px border is 38px tall and not 40, and any
   * hardcoded value silently drifts the moment the CSS changes. A window
   * computed from the wrong height stops lining up with the scroll position
   * and renders blank space where rows should be.
   *
   * These are only what is used before the first measurement lands, but they
   * still track `--row-height` and `.sill-group`.
   *
   * They no longer decide which rows are drawn, because every row is. What
   * they still do is tell the fade how deep into the chin each row has sunk,
   * where being a pixel or two out means a slightly wrong blur rather than a
   * screen of nothing.
   */
  const FALLBACK_ROW = 40;
  const FALLBACK_HEADER = 30;

  let scrollTop = $state(0);
  let height = $state(600);
  let rowHeight = $state(FALLBACK_ROW);
  let headerHeight = $state(FALLBACK_HEADER);
  /** The list's own bottom padding, which the chin sits over. */
  let inset = $state(0);

  /**
   * Clearance between a row and the edge it is nearest.
   *
   * A row parked exactly against the edge reads as half cut off, and arrowing
   * into it feels like the list stopped rather than moved.
   */
  const EDGE_GAP = 8;

  let viewport = $state<HTMLDivElement | null>(null);

  /**
   * True while a selection change is the pointer's own doing.
   *
   * The keep-in-view effect below must not answer the mouse. A row sitting
   * half over the bottom edge is selected the moment the cursor touches it,
   * scrolling it fully on pulls the next row up under the cursor, that one is
   * selected in turn, and the list crawls downward on its own for as long as
   * the mouse stays near the edge. Chromium also replays a mousemove after a
   * wheel scroll to refresh what is hovered, so the list fought the wheel the
   * same way. The keys still scroll: they move the selection without moving
   * the pointer, so this stays down.
   *
   * Plain rather than `$state`, because the effect reads it without wanting
   * to run again when it changes.
   */
  let byPointer = false;

  /**
   * Every line is drawn.
   *
   * There used to be a window over them, with spacers standing in for what was
   * above and below, sized from an assumed row height. It was written when a
   * search could return two thousand rows; it returns at most a hundred and
   * twenty now, which a browser does not notice.
   *
   * It cost two screens of blank space, both times because the assumed heights
   * stopped matching what was actually laid out, and both times the difference
   * showed up as rows that existed but were not drawn. Nothing here measures
   * anything any more: the rows are simply all there.
   */
  const lines = $derived(linesOf(commands));

  /** A new question is answered from the top of the list. */
  $effect(() => {
    // Read so the effect runs when it changes, and only then.
    asking;

    if (viewport) viewport.scrollTop = 0;
  });

  /**
   * Keeps the selected row on screen.
   *
   * Every number is measured off the elements. The row is in the DOM whatever
   * the scroll position, because they all are, so its real rectangle can be
   * asked for rather than worked out.
   */
  $effect(() => {
    const target = selected;
    // Read so the row's position is recomputed when the list changes under it.
    lines.length;
    if (!viewport) return;

    // What the mouse selected is already under the mouse. "Keeping" it on
    // screen would move the list instead.
    if (byPointer) {
      byPointer = false;
      return;
    }

    const row = viewport.querySelector<HTMLElement>(`[data-row="${target}"]`);
    if (!row) return;

    const box = row.getBoundingClientRect();
    const around = viewport.getBoundingClientRect();

    // The group label above the first row of a group comes in with it, or the
    // selection appears to sit under a heading that is cut off.
    const heading = row.previousElementSibling;
    const lead =
      heading instanceof HTMLElement && heading.classList.contains("sill-group")
        ? heading.getBoundingClientRect().height
        : 0;

    /*
     * The chin is laid OVER the bottom of this list, not beside it, so the
     * visible height is not the whole of `clientHeight`: the last stretch is
     * where rows dissolve into the fade and the controls sit. The list's own
     * bottom padding is already sized to clear it, so it is read rather than
     * copied.
     */
    const usable = viewport.clientHeight - inset;

    viewport.scrollTop = scrollFor({
      scrollTop: viewport.scrollTop,
      viewport: usable,
      scrollHeight: viewport.scrollHeight,
      rowTop: box.top - around.top + viewport.scrollTop - lead,
      rowHeight: box.height + lead,
      gap: EDGE_GAP,
      first: target === 0,
      last: target === commands.length - 1,
    });
  });


  function onScroll() {
    if (!viewport) return;
    scrollTop = viewport.scrollTop;
    height = viewport.clientHeight;
  }

  /**
   * Keeps the measurements the chin needs current.
   *
   * Nothing about row positions is measured any more: every row is drawn, so
   * where each one sits is asked of the element at the moment it matters.
   * What is still read here is the padding and the fade, because those come
   * from the stylesheet rather than from layout.
   */
  $effect(() => {
    if (!viewport) return;

    const sync = () => {
      if (!viewport) return;

      height = viewport.clientHeight;
      scrollTop = viewport.scrollTop;

      const row = viewport.querySelector<HTMLElement>(".sill-row");
      if (row && row.offsetHeight > 0) rowHeight = row.offsetHeight;
      const header = viewport.querySelector<HTMLElement>(".sill-group");
      if (header && header.offsetHeight > 0) headerHeight = header.offsetHeight;

      // Read on layout rather than per row: `getComputedStyle` forces a style
      // recalculation, and this is consulted every time the selection moves.
      inset = Number.parseFloat(getComputedStyle(viewport).paddingBottom) || 0;
    };

    sync();

    const observer = new ResizeObserver(sync);
    observer.observe(viewport);
    return () => observer.disconnect();
  });

  /**
   * Splits a title into matched and unmatched runs.
   *
   * The ranker returns the indices it matched, so the highlight shows why a
   * result placed where it did rather than leaving the user guessing.
   */
  function segments(title: string, matched: number[]) {
    const hits = new Set(matched);
    const chars = [...title];
    const out: { text: string; hit: boolean }[] = [];

    for (let i = 0; i < chars.length; i++) {
      const hit = hits.has(i);
      const previous = out[out.length - 1];
      if (previous && previous.hit === hit) previous.text += chars[i];
      else out.push({ text: chars[i], hit });
    }

    return out;
  }

  /**
   * The label on the right of a row.
   *
   * Applications carry a category worked out from where they resolve, so that
   * is shown as-is. Extension commands have no such thing and are described by
   * their mode instead.
   */
  /**
   * Whether the shell can produce an icon for this row.
   *
   * Applications, PATH executables and settings all point at a real file. An
   * extension command points at its own bundled JavaScript, and the generic
   * icon Windows gives a `.js` file says nothing, so those keep the lettered
   * tile instead.
   */
  function hasIcon(command: RankedCommand): boolean {
    /*
     * An icon set on the row is a file somebody chose deliberately, so it is
     * always worth asking about: the program behind a page from a browser, the
     * browser a web search will open in, the program that owns a Windows
     * switch. Whatever put it there knew what the row was.
     *
     * This used to be a list of modes, and every new kind of row silently drew
     * a lettered tile until somebody remembered to add it here.
     */
    if (command.icon) return true;

    // Otherwise there is only the entrypoint, and it is worth asking the shell
    // about only when it is a real file.
    return (
      command.mode === "app" ||
      command.mode === "exe" ||
      command.mode === "setting" ||
      command.mode === "file" ||
      // A folder offered as somewhere to move something. Real on disk, and
      // the shell draws it as a folder.
      command.mode === "destination"
    );
  }

  /**
   * The label on the right of a row.
   *
   * Sentence case at the same size as everything else on the line, separated
   * by colour rather than by shrinking it and putting it in capitals. A row
   * carrying an 11px uppercase tag beside 13px body reads as two unrelated
   * things sharing a line.
   */
  function kindOf(command: RankedCommand): string {
    switch (command.mode) {
      case "answer":
        return "Copy";
      case "snippet":
        return "Snippet";
      // Whose setting it is. "Setting" beside a Windows one says nothing
      // about which program it belongs to.
      case "sill-setting":
        return "Sill Setting";
      case "view":
      case "builtin":
        return "Command";
      case "no-view":
        return "Action";
      // "Windows Settings", "Control Panel" or "Windows Tools", as Windows
      // itself files them. The catalog already knows which.
      case "setting":
        return command.extensionTitle;
      // File or Folder, which the search already worked out.
      case "file":
        return command.extensionTitle;
      case "exe":
        return command.extensionTitle;
      // Saved or visited, which is the useful half of what it is.
      case "url":
        return command.extensionTitle;
      case "websearch":
        return "Web Search";
      case "window":
        return "Open Window";
      case "audio-session":
        return "App Volume";
      case "destination":
        return "Folder";
      case "emoji":
        // The group, which is the only useful thing to say about one.
        return command.extensionTitle;
      default:
        // Applications carry a category worked out from where they resolve,
        // which says more than the word "Application" would.
        return command.extensionTitle;
    }
  }

  /**
   * Which group a row belongs under.
   *
   * Coarser than the row's own label on purpose: five or six headings is a
   * structure, and fifteen is a list of headings with a row under each.
   */
  /**
   * The first row of the next group after `from`.
   *
   * Here rather than in the launcher because the grouping is computed here and
   * nowhere else. Asking the page to work out where the headings are would
   * mean two implementations of the same grouping, drifting apart the first
   * time a kind is added.
   *
   * Returns `from` unchanged when there is nowhere to go, so holding the key
   * stops at the end rather than wrapping into the middle of the list.
   */
  export function nextSection(from: number): number {
    const heads = headings();
    return heads.find((at) => at > from) ?? (heads.length ? heads[heads.length - 1] : from);
  }

  export function previousSection(from: number): number {
    const heads = headings();
    const before = heads.filter((at) => at < from);
    return before.length ? before[before.length - 1] : (heads[0] ?? from);
  }

  /** The row index each group starts at, in order. */
  function headings(): number[] {
    const out: number[] = [];
    for (const line of lines) {
      if (line.kind === "header") continue;
      if (out.length === 0 || groupOf(commands[out[out.length - 1]]) !== groupOf(line.command)) {
        out.push(line.index);
      }
    }
    return out;
  }



  /**
   * The faint middle label.
   *
   * A row shows what it carries, and the few that do not are named here.
   *
   * **This was the other way round and it was wrong.** It listed the modes
   * that may show a subtitle, so every mode added afterwards silently lost
   * one: a Windows switch, an open window and a program's own volume all set
   * a subtitle and none of them drew it. The same shape as `hasIcon` before
   * it, which was a mode allowlist that threw away every icon outside four
   * modes and made them draw lettered tiles. A list of exceptions is honest
   * about what it is; a list of permissions pretends to be complete.
   */
  /**
   * Whether the subtitle is a path rather than prose.
   *
   * It decides which end of the subtitle is truncated, and which end is
   * truncated decides whether it is readable at all. A path wants its tail,
   * because the folder something is in is the useful half; a sentence wants
   * its head.
   *
   * A list of the ones that are paths rather than of the ones that are not.
   * The other way round is how the conversation rows arrived reading
   * "min ago and 1 reply 3", with a leading digit dragged to the far end by
   * the bidi algorithm: a mode added after the rule was written gets the
   * wrong default, and the wrong default here is silent.
   */
  function isPath(command: RankedCommand): boolean {
    return (
      command.mode === "file" ||
      command.mode === "file-setup" ||
      command.mode === "destination" ||
      command.mode === "exe"
    );
  }

  function sourceOf(command: RankedCommand): string {
    // A measurement wins over anything written down, and it is checked first
    // because the rules below are about descriptions. A row showing what the
    // machine is doing right now is not describing where that can be found.
    const measured = live?.[command.id];
    if (measured) return measured;

    // An extension is worth naming, and it is not in the subtitle.
    if (command.mode === "view" || command.mode === "no-view") {
      return command.extensionTitle;
    }

    // The subtitle is the character itself, drawn as the mark on the left.
    // Writing it here as well would put the emoji on the row twice.
    if (command.mode === "emoji") return "";

    // The category on the right already says "Applications", and the path
    // underneath every one of them is noise rather than information.
    if (command.mode === "app") return "";

    return command.subtitle;
  }
</script>

<!--
  The list is named by the field rather than by itself.

  A screen reader announces the field, and the field points here for what is
  currently highlighted. Two names would be read out twice.
-->
<div
  id={LISTBOX}
  class="sill-list"
  role="listbox"
  tabindex="-1"
  aria-label="Results"
  bind:this={viewport}
  onscroll={onScroll}
>
  {#each lines as line, at (line.kind === "header" ? `h:${line.label}` : line.command.id)}
    {#if line.kind === "header"}
      <div class="sill-group" role="presentation">{line.label}</div>
    {:else}
      {@const command = line.command}
      {@const index = line.index}
      <div
        id={optionId(index)}
        data-row={index}
        class="sill-row"
        class:answer={command.mode === "answer"}
        class:path={isPath(command)}
        class:selected={index === selected}
        role="option"
        aria-selected={index === selected}
        tabindex="-1"
        onmousemove={() => {
          // Only a change raises the flag: on the row already selected the
          // parent's state does not move, the effect never runs, and a raised
          // flag would be left waiting to swallow the next arrow key.
          if (index === selected) return;
          byPointer = true;
          onselect(index);
        }}
        onclick={() => onrun(index)}
        onkeydown={(e) => e.key === "Enter" && onrun(index)}
      >
        {#if command.mode === "emoji"}
          <!-- The emoji is its own icon. A lettered tile beside the character
               it stands for would be a worse drawing of the same thing. -->
          <span class="emoji" aria-hidden="true">{command.subtitle}</span>
        {:else if command.mode === "answer"}
          <!-- A lettered tile would show the first digit of the result,
               which means nothing. -->
          <span class="equals" aria-hidden="true">=</span>
        {:else if command.panel}
          <!-- Anything Sill owns wears its panel's mark, so a setting and the
               command that opens it are recognisably the same family. -->
          <SettingsIcon name={command.panel as IconName} size={26} />
        {:else}
          <LaunchIcon
            path={command.icon ?? command.entrypoint}
            label={command.title}
            resolvable={hasIcon(command)}
          />
        {/if}
        <!--
          Title over subtitle, in one column.

          A single flex column rather than two siblings on the row, so a row
          with no source centres its title instead of sitting high with a gap
          under it. Most rows are that case: `sourceOf` returns nothing for
          applications, PATH executables and emoji, which is nearly everything
          in the index.
        -->
        <span class="text">
          <span class="line">
            <span class="title">
              {#each segments(command.title, command.matched) as part}
                {#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}
              {/each}
            </span>
            {#if command.alias}
              <!-- Beside the name rather than at the end of the row, because
                   it is another name for the same thing and reads as one
                   there. A name kept out of sight is a name nobody remembers
                   setting. -->
              <span class="alias">{command.alias}</span>
            {/if}
          </span>
          {#if sourceOf(command)}
            <span class="extension">{sourceOf(command)}</span>
          {/if}
        </span>

        <span class="spacer"></span>
        {#if command.toggle !== undefined}
          <!--
            A switch draws as a switch, in place of the category. The category
            on one of these says "System" every time, which the heading above
            the group already says.
          -->
          <span
            class="switch"
            class:on={command.toggle}
            role="img"
            aria-label={command.toggle ? "On" : "Off"}
          ></span>
        {:else}
          <span class="kind">{kindOf(command)}</span>
        {/if}
        {#if numeric && index < 9}
          <!--
            Hidden from the reader on purpose. The combobox announces each
            option through `aria-activedescendant`, and appending "Ctrl 4" to
            every announcement is noise: it is a visual reminder of a binding,
            not part of what the row is. The category beside it stays spoken.
          -->
          <span class="sill-key jump" aria-hidden="true">Ctrl {index + 1}</span>
        {/if}
      </div>
    {/if}
  {/each}

  {#if commands.length === 0}
    <div class="sill-empty">
      <img src="/sill.png" alt="" width="32" height="32" draggable="false" />
      <span class="headline">Nothing found</span>
      <span class="hint">Try fewer letters, or a word from further along the name.</span>
    </div>
  {/if}
</div>

<style>
  /*
   * The text column: title over subtitle.
   *
   * `min-width: 0` is what lets the children ellipsis. Without it a flex item
   * refuses to shrink below its content and a long path pushes the category
   * clean off the row instead of truncating.
   */
  /*
   * The switch on a row that is one.
   *
   * Colour is the only thing that moves besides the knob. A row like this is
   * looked at for half a second, and anything more elaborate is motion for its
   * own sake.
   */
  .switch {
    flex: none;
    width: var(--icon-tile);
    height: 15px;
    position: relative;
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    box-shadow: var(--ring);
    transition: background-color var(--motion-state) var(--ease);
  }

  .switch::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 11px;
    height: 11px;
    border-radius: 50%;
    background: var(--text-2);
    transition:
      transform var(--motion-state) var(--ease),
      background-color var(--motion-state) var(--ease);
  }

  .switch.on {
    background: var(--accent);
    box-shadow: none;
  }

  .switch.on::after {
    transform: translateX(11px);
    background: var(--core-background);
  }

  .text {
    display: flex;
    flex-direction: column;
    justify-content: center;
    min-width: 0;
    flex: 1;
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .title {
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: var(--weight-body);
    /* Stated in px, not as a ratio, so the row's height does not move when
       the interface face changes. Satoshi, Inter and Segoe UI Variable have
       different default metrics and Rust cannot see which one is active. */
    line-height: var(--line-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* The one row whose title IS the payload rather than a label for it. */
  .answer .title {
    font-family: var(--font-display);
    font-size: var(--text-query);
    font-weight: var(--weight-medium);
    letter-spacing: var(--track-tight);
  }

  .title mark {
    background: none;
    color: var(--core-accent);
    font-weight: var(--weight-strong);
  }

  /* Sized to fill the icon tile rather than sit in it, because here the
     character IS the icon. */
  .emoji {
    display: grid;
    place-items: center;
    width: var(--icon-tile);
    height: var(--icon-tile);
    flex: none;
    font-size: var(--glyph-md);
    line-height: 1;
  }

  /* The user's own name for this. Quiet: it is a reminder, not a label
     competing with the title it sits beside. */
  .alias {
    flex: none;
    padding: var(--space-hair) var(--space-1);
    font-size: var(--text-micro);
    font-weight: var(--weight-medium);
    letter-spacing: var(--track-micro);
    color: var(--accent-bright);
    background: var(--fill-2);
    border-radius: var(--radius-sm);
    white-space: nowrap;
  }

  .extension {
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: var(--line-meta);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /*
   * A path keeps its tail; everything else keeps its head.
   *
   * `rtl` is what makes a truncated path show the folder it is in rather than
   * the drive it is on, and it is wrong for anything that is a sentence: the
   * bidi algorithm moves a leading digit to the far end, so "3 min ago" is
   * drawn as "min ago 3". This used to be the default with one exception, and
   * the next mode that had a sentence in its subtitle got it wrong.
   */
  .path .extension {
    direction: rtl;
    text-align: left;
  }

  .equals {
    display: grid;
    place-items: center;
    width: var(--icon-tile);
    height: var(--icon-tile);
    flex: none;
    font-family: var(--font-display);
    font-size: var(--glyph-sm);
    color: var(--text-3);
  }

  .spacer {
    flex: none;
    width: var(--space-3);
  }

  /*
   * The category, and it is not what the group heading says.
   *
   * The heading names the family (Applications, Files, Commands); this names
   * where the entry resolves (System, Store App, Documentation, Web Link,
   * Command Line), and all of those appear inside one heading. Quieter than
   * the title rather than smaller, because three type sizes on one line is
   * what made the old row read as three unrelated things.
   */
  .kind {
    flex: none;
    color: var(--text-3);
    font-size: var(--text-meta);
    white-space: nowrap;
  }

  /* Ctrl and a digit, on the first nine rows. A reminder that the binding
     exists, which is the whole reason it is drawn: it has always worked and
     nobody could discover it. */
  .jump {
    margin-left: var(--space-2);
  }
</style>
