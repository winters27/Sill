<script lang="ts">
  import type { RankedCommand } from "$lib/exthost/commands";
  import LaunchIcon from "./LaunchIcon.svelte";
  import SettingsIcon, { type IconName } from "./SettingsIcon.svelte";

  interface Props {
    commands: RankedCommand[];
    selected: number;
    onselect: (index: number) => void;
    onrun: (index: number) => void;
  }

  let { commands, selected, onselect, onrun }: Props = $props();

  /**
   * Row and header heights, measured rather than assumed.
   *
   * Hardcoding was wrong twice over: `box-sizing: border-box` is global, so a
   * row with `height: 38px` and a 1px border is 38px tall and not 40, and any
   * hardcoded value silently drifts the moment the CSS changes. A window
   * computed from the wrong height stops lining up with the scroll position
   * and renders blank space where rows should be.
   */
  const FALLBACK_ROW = 38;
  const FALLBACK_HEADER = 26;
  /** Lines drawn beyond the viewport, so scrolling never flashes empty. */
  const OVERSCAN = 8;

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let height = $state(600);
  let rowHeight = $state(FALLBACK_ROW);
  let headerHeight = $state(FALLBACK_HEADER);

  type Line =
    | { kind: "header"; label: string }
    | { kind: "row"; command: RankedCommand; index: number };

  /**
   * The list as drawn: group labels interleaved with their rows.
   *
   * Groups are ordered by their best-scoring member, not alphabetically, so
   * the ranker still decides what you see first. Grouping that fought the
   * ranking would put the answer below a heading nobody was looking at.
   */
  const lines = $derived.by((): Line[] => {
    const order: string[] = [];
    const groups = new Map<string, Line[]>();

    commands.forEach((command, index) => {
      const label = groupOf(command);
      let bucket = groups.get(label);
      if (!bucket) {
        bucket = [];
        groups.set(label, bucket);
        order.push(label);
      }
      bucket.push({ kind: "row", command, index });
    });

    // One group is not a grouping; a lone header over the whole list is
    // noise rather than structure.
    if (order.length < 2) {
      return commands.map((command, index) => ({ kind: "row", command, index }));
    }

    return order.flatMap((label) => [
      { kind: "header", label } as Line,
      ...(groups.get(label) ?? []),
    ]);
  });

  /** Where each line starts, and where the list ends. One extra entry. */
  const offsets = $derived.by(() => {
    const out = new Array<number>(lines.length + 1);
    let y = 0;
    for (let i = 0; i < lines.length; i++) {
      out[i] = y;
      y += lines[i].kind === "header" ? headerHeight : rowHeight;
    }
    out[lines.length] = y;
    return out;
  });

  const total = $derived(offsets[lines.length] ?? 0);

  /** Index of the last line starting at or before `y`. */
  function lineAt(y: number): number {
    let low = 0;
    let high = lines.length - 1;
    while (low < high) {
      const mid = (low + high + 1) >> 1;
      if (offsets[mid] <= y) low = mid;
      else high = mid - 1;
    }
    return low;
  }

  /*
   * Only the visible slice goes in the DOM.
   *
   * The index holds well over a thousand entries and rendering every row
   * makes each keystroke re-create the lot. Two spacers stand in for what is
   * above and below, so the scrollbar still describes the whole list.
   */
  const first = $derived(Math.max(0, lineAt(scrollTop) - OVERSCAN));
  const last = $derived(
    Math.min(lines.length, lineAt(scrollTop + height) + 1 + OVERSCAN),
  );
  const slice = $derived(lines.slice(first, last));

  /**
   * Keeps the selected row on screen.
   *
   * Sets scrollTop directly rather than calling `scrollIntoView`, because a
   * row outside the window is not in the DOM to scroll to.
   */
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

    if (top < viewport.scrollTop) {
      viewport.scrollTop = top;
    } else if (bottom > viewport.scrollTop + viewport.clientHeight) {
      viewport.scrollTop = bottom - viewport.clientHeight;
    }
  });

  function onScroll() {
    if (!viewport) return;
    scrollTop = viewport.scrollTop;
    height = viewport.clientHeight;
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

  function groupOf(command: RankedCommand): string {
    switch (command.mode) {
      case "answer":
        return "Answer";
      case "snippet":
        return "Snippets";
      case "sill-setting":
        return "Sill Settings";
      case "view":
      case "no-view":
        return "Commands";
      case "builtin":
        return "Sill";
      case "setting":
        return "Settings";
      case "file":
      case "file-setup":
        return "Files";
      case "window":
        return "Open Windows";
      case "emoji":
        return "Emoji";
      case "exe":
        return "Developer";
      default:
        return "Applications";
    }
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

<div
  class="sill-list"
  role="listbox"
  tabindex="-1"
  aria-label="Commands"
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
        <span class="title">
          {#each segments(command.title, command.matched) as part}
            {#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}
          {/each}
        </span>
        {#if command.alias}
          <!-- Beside the name rather than at the end of the row, because it
               is another name for the same thing and reads as one there. A
               name kept out of sight is a name nobody remembers setting. -->
          <span class="alias">{command.alias}</span>
        {/if}
        {#if sourceOf(command)}
          <span class="extension">{sourceOf(command)}</span>
        {/if}
        <span class="spacer"></span>
        <span class="kind">{kindOf(command)}</span>
      </div>
    {/if}
  {/each}

  <div style="height: {Math.max(0, total - (offsets[last] ?? 0))}px"></div>

  {#if commands.length === 0}
    <div class="sill-empty">Nothing found</div>
  {/if}
</div>

<style>
  .title {
    color: var(--core-foreground);
    font-size: var(--text-row);
    font-weight: var(--weight-row);
    white-space: nowrap;
    /* A long title yields before the labels after it do. */
    overflow: hidden;
    text-overflow: ellipsis;
    flex: none;
    max-width: 60%;
  }

  /* The one row whose title IS the payload rather than a label for it. */
  .answer .title {
    font-family: var(--font-display);
    font-size: 17px;
    font-weight: 500;
    letter-spacing: -0.01em;
    /* A long unit result must still yield to the question beside it. */
    max-width: 70%;
  }

  .title mark {
    background: none;
    color: var(--core-accent);
    font-weight: 600;
  }

  /* The user's own name for this. Quiet: it is a reminder, not a label
     competing with the title it sits beside. */
  /* Sized to fill the icon tile rather than sit in it, because here the
     character IS the icon. */
  .emoji {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    flex: none;
    font-size: 19px;
    line-height: 1;
  }

  .alias {
    flex: none;
    padding: 1px 5px;
    font-size: 10px;
    font-weight: 500;
    color: var(--accent-bright);
    background: rgba(var(--accent-rgb), 0.13);
    border-radius: 4px;
    white-space: nowrap;
  }

  .extension {
    color: var(--text-faint);
    font-size: var(--text-row);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    /* A full path is long; it must not push the category off the row, and
       `rtl` is what keeps the end of a path rather than its start. */
    max-width: 45%;
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
    width: 22px;
    height: 22px;
    flex: none;
    font-family: var(--font-display);
    font-size: 16px;
    color: var(--text-faint);
  }

  .spacer {
    flex: 1;
    min-width: 12px;
  }

  /*
   * The same size as the title, quieter rather than smaller.
   *
   * These used to be 11px uppercase and coloured per kind, which put three
   * type sizes and four colours on one line. The group heading above the row
   * already says what kind it is, so this only has to confirm it.
   */
  .kind {
    flex: none;
    color: var(--text-faint);
    font-size: var(--text-row);
    white-space: nowrap;
  }
</style>
