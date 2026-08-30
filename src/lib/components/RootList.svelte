<script lang="ts">
  import type { RankedCommand } from "$lib/exthost/commands";
  import { LISTBOX, optionId } from "$lib/results";
  import { groupOf, linesOf, offsetsOf, windowOf, type Line } from "$lib/list";
  import LaunchIcon from "./LaunchIcon.svelte";
  import SettingsIcon, { type IconName } from "./SettingsIcon.svelte";

  interface Props {
    commands: RankedCommand[];
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

  let { commands, selected, onselect, onrun, asking = "", numeric = false }: Props = $props();



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
   * still track `--row-height` and `.sill-group`: a wrong fallback paints
   * blank space on the very first frame, which is the frame somebody sees.
   */
  const FALLBACK_ROW = 40;
  const FALLBACK_HEADER = 30;
  /** Lines drawn beyond the viewport, so scrolling never flashes empty. */
  const OVERSCAN = 8;

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let height = $state(600);
  /**
   * How far the list can actually scroll.
   *
   * Read from the element rather than worked out from the rows, because the
   * rows are not all there is inside it: the container's own padding clears
   * the chin, and that padding is scrollable too.
   */
  let reach = $state(0);
  let rowHeight = $state(FALLBACK_ROW);
  let headerHeight = $state(FALLBACK_HEADER);

  /**
   * The list as drawn: group labels interleaved with their rows.
   *
   * Groups are ordered by their best-scoring member, not alphabetically, so
   * the ranker still decides what you see first. Grouping that fought the
   * ranking would put the answer below a heading nobody was looking at.
   */
  /**
   * The list as drawn: group labels interleaved with their rows.
   *
   * The arithmetic lives in `$lib/list` so it can be tested. A list that
   * renders a screen of nothing is arithmetic, not markup.
   */
  const lines = $derived(linesOf(commands));

  const offsets = $derived(offsetsOf(lines, rowHeight, headerHeight));
  const total = $derived(offsets[lines.length] ?? 0);

  /*
   * Only the visible slice goes in the DOM.
   *
   * The index holds well over a thousand entries and rendering every row makes
   * each keystroke re-create the lot. Two spacers stand in for what is above
   * and below, so the scrollbar still describes the whole list.
   */
  const shown = $derived(
    windowOf(offsets, lines.length, scrollTop, height, OVERSCAN, reach),
  );
  const first = $derived(shown.first);
  const last = $derived(shown.last);

  const slice = $derived(lines.slice(first, last));

  /**
   * Keeps the selected row on screen.
   *
   * Sets scrollTop directly rather than calling `scrollIntoView`, because a
   * row outside the window is not in the DOM to scroll to.
   */
  /** A new question is answered from the top of the list. */
  $effect(() => {
    // Read so the effect runs when it changes, and only then.
    asking;

    if (viewport) {
      viewport.scrollTop = 0;
      scrollTop = 0;
    }
  });

  $effect(() => {
    const target = selected;
    if (!viewport) return;

    const line = lines.findIndex((entry) => entry.kind === "row" && entry.index === target);
    if (line === -1) return;

    // The group label above the first row of a group scrolls in with it, or
    // the selection appears to sit under a heading that is cut off.
    const leadIn = lines[line - 1]?.kind === "header" ? headerHeight : 0;
    const top = offsets[line] - leadIn;
    const bottom = offsets[line + 1];

    // Both branches record where they moved to. Setting `scrollTop` on the
    // element fires a scroll event, but not until the browser gets round to
    // it, and a keystroke landing first would slice the list from a position
    // it is no longer at.
    /*
     * The chin is laid OVER the bottom of this list, not beside it.
     *
     * So `clientHeight` is not the usable height: the last stretch of it is
     * where rows dissolve into the fade and the controls sit. Scrolling a row
     * to `bottom - clientHeight` parks it exactly there, which reads as the
     * selection fading out every time you arrow past the last full row.
     *
     * The floor is the container's own bottom padding, read rather than
     * copied. That padding is already sized to clear the fade, so asking it is
     * both correct and impossible to get out of step with.
     */
    const floor = viewport.clientHeight - bottomInset();

    if (top < viewport.scrollTop) {
      viewport.scrollTop = top;
      scrollTop = top;
    } else if (bottom > viewport.scrollTop + floor) {
      viewport.scrollTop = bottom - floor;
      scrollTop = viewport.scrollTop;
    }
  });

  /**
   * The strip at the bottom a row must not be scrolled into.
   *
   * The list's own `padding-bottom`, which already clears both the fade and
   * the controls sitting over it. Measured rather than hardcoded, for the same
   * reason the row height is: a number copied out of the CSS drifts the first
   * time the CSS changes, and the failure here is a selection that scrolls out
   * of sight rather than anything that looks like a bug in this file.
   */
  function bottomInset(): number {
    if (!viewport) return 0;
    return Number.parseFloat(getComputedStyle(viewport).paddingBottom) || 0;
  }

  function onScroll() {
    if (!viewport) return;
    scrollTop = viewport.scrollTop;
    height = viewport.clientHeight;
    reach = viewport.scrollHeight - viewport.clientHeight;
  }

  /**
   * Keeps the row height and viewport size current.
   *
   * The viewport height was previously only read during a scroll, so before
   * the first scroll the window was sized from a guess. A resize observer
   * covers the window changing size too.
   */
  $effect(() => {
    if (!viewport) return;

    const sync = () => {
      if (!viewport) return;
      height = viewport.clientHeight;
      reach = viewport.scrollHeight - viewport.clientHeight;
      const row = viewport.querySelector<HTMLElement>(".sill-row");
      if (row && row.offsetHeight > 0) rowHeight = row.offsetHeight;
      const header = viewport.querySelector<HTMLElement>(".sill-group");
      if (header && header.offsetHeight > 0) headerHeight = header.offsetHeight;
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
    return (
      command.mode === "app" ||
      command.mode === "exe" ||
      command.mode === "setting" ||
      command.mode === "file"
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
      case "sill-setting":
        return "Setting";
      case "view":
      case "builtin":
        return "Command";
      case "no-view":
        return "Action";
      case "setting":
        return "Setting";
      case "file":
        return "File";
      case "exe":
        return "Executable";
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
   * An extension is worth naming. A settings page shows the section it lives
   * under, which is what tells "Proxy" under Network apart from anything else
   * with that word in it. Applications need neither, since the category
   * already shows on the right.
   */
  function sourceOf(command: RankedCommand): string {
    // The question, beside the answer.
    if (command.mode === "answer") return command.subtitle;
    // The keyword, which is how it is used when the launcher is closed.
    if (command.mode === "snippet") return command.subtitle;
    // Which panel it lives in, so a result says where it came from.
    if (command.mode === "sill-setting") return command.subtitle;
    if (command.mode === "view" || command.mode === "no-view") return command.extensionTitle;
    if (command.mode === "setting") return command.subtitle;
    // A file's path is what tells two files with the same name apart.
    if (command.mode === "file") return command.subtitle;
    if (command.mode === "builtin") return command.subtitle;
    return "";
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
  <div style="height: {offsets[first] ?? 0}px"></div>

  {#each slice as line (line.kind === "header" ? `h:${line.label}` : line.command.id)}
    {#if line.kind === "header"}
      <div class="sill-group" role="presentation">{line.label}</div>
    {:else}
      {@const command = line.command}
      {@const index = line.index}
      <div
        id={optionId(index)}
        class="sill-row"
        class:answer={command.mode === "answer"}
        class:selected={index === selected}
        role="option"
        aria-selected={index === selected}
        tabindex="-1"
        onmousemove={() => onselect(index)}
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
        <span class="kind">{kindOf(command)}</span>
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

  <div style="height: {Math.max(0, total - (offsets[last] ?? 0))}px"></div>

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
    width: 26px;
    height: 26px;
    flex: none;
    font-size: var(--glyph-md);
    line-height: 1;
  }

  /* The user's own name for this. Quiet: it is a reminder, not a label
     competing with the title it sits beside. */
  .alias {
    flex: none;
    padding: 1px var(--space-1);
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
    /* A full path is long, and `rtl` is what keeps the end of a path rather
       than its start. */
    direction: rtl;
    text-align: left;
  }

  /* A question is prose, not a path: it reads from the left like everything
     else, and truncating its tail is the right end to lose. */
  .answer .extension {
    direction: ltr;
  }

  .equals {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
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
