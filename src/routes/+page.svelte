<script lang="ts">
  // Aliased: this page has a `tick` of its own, which measures a running
  // command rather than waiting for the DOM.
  import { onMount, tick as rendered, untrack } from "svelte";
  import { beginCapture, captureScreen, lastImage, openMarkup } from "$lib/capture";
  import { openQuicklink } from "$lib/quicklinks";
  import { saveWorkspace } from "$lib/workspaces";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { whenVisible } from "$lib/visible";
  import { forgetUnreadable } from "$lib/status";
  import ListView from "$lib/components/ListView.svelte";
  import GridView from "$lib/components/GridView.svelte";
  import FormView from "$lib/components/FormView.svelte";
  import RootList from "$lib/components/RootList.svelte";
  import Instead from "$lib/components/Instead.svelte";
  import AiMark from "$lib/components/settings/AiMark.svelte";
  import Markdown from "$lib/components/Markdown.svelte";
  import Steps from "$lib/components/Steps.svelte";
  import { LISTBOX, isBrowsing, optionId, selectionAfter } from "$lib/results";
  import { deleteMeansTheRow, isTyping, typedInto } from "$lib/typing";
  import { asUrl, isPath, isUrl } from "$lib/typed";
  import {
    behaviourOf,
    drawsItsOwn,
    handlesItsOwnEscape,
    hasRowActions,
    searchesOnType,
  } from "$lib/modes";
  import ActionPanel from "$lib/components/ActionPanel.svelte";
  import LauncherMenu from "$lib/components/LauncherMenu.svelte";
  import ClipboardView from "$lib/components/ClipboardView.svelte";
  import StoreView from "$lib/components/StoreView.svelte";
  import { storeClose, type StoreRow } from "$lib/store";
  import WidgetBoard from "$lib/widgets/Board.svelte";
  import WidgetChin from "$lib/widgets/Chin.svelte";
  import { actionFor, collectActions, isRunnable } from "$lib/exthost/actions";
  import { clipboardEntry, clipboardMerge } from "$lib/clipboard";
  import {
    chordFrom,
    navigationChords,
    setAlias,
    setPreferences,
    type Move as MoveKey,
  } from "$lib/settings";
  import {
    activateHandler,
    dismiss,
    launchCommand,
    aiAsk,
    aiFollowUp,
    aiNew,
    aiResume,
    aiConversations,
    aiForget,
    aiDecide,
    aiRefusePending,
    aiClear,
    aiReady,
    completePath as finishPath,
    aiTranscript,
    forgetPreviews,
    searchAppVolume,
    searchProcesses,
    searchDestinations,
    summonPainted,
    windowPreview,
    systemStates,
    searchCommands,
    unloadExtension,
    scriptArguments,
    runScript,
    cancelScript,
    type Finished,
    snippetFields,
    pasteSnippetFilled,
    liveRows,
    type LiveRow,
    performBuiltin,
    searchElsewhere,
    searchWindows,
    searchEmoji,
    fileSearchMissing,
    startFileSearch,
    type FileSearchMissing,
    recordUse,
    queryHistory,
    openPath,
    browserAsCommand,
    defaultBrowser,
    extractTextFromLastImage,
    pathRow,
    urlRow,
    webSearchRow,
    fileAsCommand,
    actionsFor,
    // `runAction` here already means "run the panel entry at this index".
    runAction as runObjectAction,
    asTarget,
    type ActionTarget,
    undoAction,
    type ActionInfo,
    type AiConversation,
    type AiAsking,
    type AiStep,
    type AiReady,
    type AiTurn,
    type RankedCommand,
  } from "$lib/exthost/commands";
  import { ViewTree, isHandlerRef, type ElementNode, type Op } from "$lib/exthost/tree";
  import { SearchRelay, itemsOf, rowsOf, searchProps } from "$lib/exthost/search";
  import {
    applyAppearance,
    getPreferences,
    openAsk,
    openSettings,
    type Preferences,
  } from "$lib/settings";
  import "$lib/theme/theme.css";
  import { hint } from "$lib/hint";

  type UiEvent =
    | { kind: "render"; session: string; ops: Op[] }
    | { kind: "showToast"; session: string; id: string; title: string; message: string; style: string }
    | { kind: "updateToast"; session: string; id: string; title: string; message: string; style: string }
    | { kind: "hideToast"; session: string; id: string }
    | { kind: "showHud"; session: string; text: string }
    | { kind: "setSearchText"; session: string; text: string }
    | { kind: "popToRoot"; session: string }
    | { kind: "closeMainWindow"; session: string }
    | { kind: "crashed"; session: string; reason: string }
    | { kind: "closed"; session: string; reason: string };

  const tree = new ViewTree();

  /** Root browses installed commands; command shows one that is running. */
  let mode = $state<
    | "root"
    | "command"
    | "clipboard"
    | "argument"
    | "switcher"
    | "collection"
    | "alias"
    | "emoji"
    /**
     * Every program that is playing something, with its own volume.
     *
     * Its own mode rather than rows in the root list, and that is a
     * measurement: enumerating the audio sessions costs about three
     * milliseconds and the root list runs on every keystroke, whether or not
     * anything about sound was typed.
     */
    | "appVolume"
    /**
     * What is running, and what it costs.
     *
     * Its own view for the same reason the volume list has one: walking every
     * process on the machine is not something to do because somebody typed the
     * letter p.
     */
    | "processes"
    | "widgets"
    /**
     * Naming the arrangement of windows being saved.
     *
     * The launcher's own field rather than a dialog, the way an alias and a
     * collection are named: a window to dismiss for one short string is a
     * window nobody wanted.
     */
    | "namingWorkspace"
    /** A script's output, while it runs and once it has finished. */
    | "output"
    /**
     * Browsing extensions that are not installed yet.
     *
     * Its own mode rather than rows in the root list, and for the same reason
     * the process view has one: the catalogue is three thousand listings that
     * have to be fetched, and nothing about typing a letter should reach the
     * network. It is entered deliberately and left completely, which is what
     * lets the catalogue be dropped on the way out.
     */
    | "store"
    /**
     * A conversation with a model.
     *
     * Its own mode rather than a row in the list, because an answer is a
     * paragraph and a list is a list. Reached by Tab from the root, which is
     * where the question was already being typed.
     */
    | "ai"
    | "conversations"
    /**
     * Picking somewhere to move a file to.
     *
     * A list rather than a typed path, because a path typed into a launcher is
     * a path typed wrong: no completion, no telling whether it exists, and no
     * way to see that there are three folders called "src". The rows are
     * folders and Enter picks one.
     */
    | "destination"
  >("root");
  /**
   * The quicklink waiting for something to be typed into it.
   *
   * A quicklink with `{query}` in it is a search, and a search with no words
   * is not worth opening. So selecting one does not open it: it takes over
   * the field, and the next thing typed is what goes in the hole.
   */
  /**
   * What the field is being borrowed to collect, and what to do with it.
   *
   * A quicklink with a hole in it was the first thing to need this, and
   * renaming a file is the second: both are "take over the field, then act on
   * what was typed". `what` is the difference, so the two do not become two
   * modes that behave almost the same.
   */
  let awaiting = $state<{
    what: "quicklink" | "rename" | "snippet" | "script";
    id: string;
    title: string;
    link: string;
    /**
     * The holes still to be asked about, first one first.
     *
     * Only a snippet has these. One field at a time rather than a form,
     * because the launcher already knows how to borrow its own field for a
     * line of text, and a window that opens to collect three words is a window
     * to dismiss afterwards.
     */
    fields?: string[];
    /** What has been typed so far, by name. */
    filled?: Record<string, string>;
    /**
     * What has been typed so far, in order.
     *
     * A script takes its arguments by position, so what matters is the order
     * they were given in rather than what each one was called. A snippet is
     * the other way round: its holes are named and can appear in any order in
     * the text. Two shapes because they are two different things, not one
     * shape bent to cover both.
     */
    given?: string[];
    /**
     * The row itself, for the ones that end in a registry action.
     *
     * Only a rename has this. What was typed is the argument the action takes,
     * and an action needs the thing it is acting on as well as the answer:
     * renaming used to be a Tauri command of its own that took a bare path,
     * which is how it ended up being the one action nothing but this page
     * could run.
     */
    of?: ActionTarget;
  } | null>(null);

  /**
   * What is being moved, while somewhere is being picked for it.
   *
   * Not part of `awaiting`, which borrows the field to collect a line of text.
   * This borrows the whole list instead: the answer is one of the rows rather
   * than whatever was typed, and typing only narrows them.
   */
  let moving = $state<{ path: string; title: string; of: ActionTarget } | null>(null);
  let session = $state<string | null>(null);

  /**
   * Sessions that crashed, and why, keyed by session id.
   *
   * Not state, because nothing renders from it: it exists so a crash that
   * arrives before its launch has finished is still there when the launch
   * looks. An extension refused a module at `require` dies during module load,
   * which happens before the load's own caller has adopted the session, so the
   * two race and the launch used to win by clearing the status.
   *
   * Entries are removed as they are read, and the only writer is the crash
   * event, so this cannot grow without bound in a session that never crashes.
   */
  const died = new Map<string, string>();
  let running = $state<{ title: string; extensionTitle: string } | null>(null);

  let query = $state("");
  let selected = $state(0);
  let commands = $state<RankedCommand[]>([]);

  /**
   * The query the rows on screen belong to.
   *
   * Kept so a list arriving for the query somebody has already typed past can
   * be told from one arriving for the query they are looking at, which is what
   * decides whether the highlight starts again at the top.
   */
  let showing = $state("");

  /**
   * Puts a set of rows on screen and decides where the highlight goes.
   *
   * One place, because the selection was an index and every one of the eight
   * sites that replaced the list had its own `if (selected >= length)`, which
   * is a bounds check rather than a rule. A number means nothing once the list
   * it counted into has been replaced: typing another character kept row five
   * selected, and row five was now a different row.
   *
   * ## How many times this runs for one keystroke
   *
   * Twice, and the second time is optional. The audit counted up to five
   * visible steps; `P1-01` folded windows, emoji and the two slow sources into
   * one answer, and this is what is left.
   *
   * 1. The search comes back and the list is drawn. Anything appended in the
   *    same breath, such as the row offering to set file search up, happens
   *    with no `await` in between, so Svelte writes both in one update and it
   *    is one paint rather than two.
   * 2. A debounce later, files, browser pages, and the offers to open an
   *    address or look words up arrive together. Also one paint, and also with
   *    no `await` splitting it.
   *
   * That second step is deliberate and not worth removing: it is what stops a
   * slow query against somebody else's files delaying the results Sill already
   * has, and it only ever appends below what is on screen, so nothing a reader
   * is looking at moves. **A new source that awaits between two `show` calls
   * puts a step back**, which is the thing to watch for.
   */
  function show(rows: RankedCommand[], forQuery: string) {
    selected = selectionAfter(
      { id: commands[selected]?.id, index: selected },
      rows,
      forQuery === showing,
    );

    commands = rows;
    showing = forQuery;
  }

  /**
   * Subtitles that are a measurement, by row id.
   *
   * Separate from `commands` because that is replaced on every search and a
   * subtitle patched into it would be gone on the next keystroke.
   */
  let live = $state<Record<string, string>>({});

  /**
   * Which hole is being asked about, and what Enter will do with it.
   *
   * Counted out loud because a field that just asks again looks like the last
   * answer was rejected. Knowing there are two more is the difference between
   * filling in a form and wondering what went wrong.
   */
  const snippetAsking = $derived.by(() => {
    if (awaiting?.what !== "snippet") return "";

    const left = awaiting.fields?.length ?? 0;
    const done = Object.keys(awaiting.filled ?? {}).length;
    const total = done + left;
    const at = done + 1;

    return left > 1
      ? `Enter fills this one and asks for the next. ${at} of ${total}.`
      : `Enter fills this one and pastes the snippet. ${at} of ${total}.`;
  });

  /**
   * A script that is running, or the last one that ran.
   *
   * Its own surface rather than a line of status, because the whole point of
   * `fullOutput` is that the output is the answer: a script that prints twenty
   * lines has nowhere to put them in a subtitle, and a toast that vanishes is
   * the wrong place for something somebody ran deliberately to read.
   */
  let output = $state<{
    job: string;
    title: string;
    running: boolean;
    stdout: string;
    stderr: string;
    code: number | null;
    ended: "finished" | "timedOut" | "cancelled" | "started";
  } | null>(null);

  /**
   * Starts one and shows it, without waiting for it to finish.
   *
   * The window changes before there is anything to show, because a script that
   * takes four seconds with no acknowledgement reads as a launcher that
   * swallowed the keystroke.
   */
  async function startScript(path: string, title: string, args: string[]) {
    try {
      const job = await runScript(path, args);

      output = {
        job,
        title,
        running: true,
        stdout: "",
        stderr: "",
        code: null,
        ended: "finished",
      };
      mode = "output";
      query = "";
      selected = 0;
    } catch (err) {
      status = `${err}`;
    }
  }

  /** The ticker, while there is one. */
  let ticking: ReturnType<typeof setInterval> | undefined;

  let finishedScript: UnlistenFn | undefined;

  /**
   * Asks what the live rows say, and stops when the answer is nothing.
   *
   * Nothing means Rust has decided the launcher is not visible. That decision
   * is deliberately not made here: the window can go away by the hotkey, by a
   * click elsewhere, or because an action put it away, and a timer that had to
   * recognise all three would be right until the day somebody added a fourth.
   * Asking something that always knows, and stopping when it says to, is right
   * however it was dismissed.
   */
  async function tick() {
    let rows: LiveRow[] = [];

    try {
      rows = await liveRows();
    } catch {
      // Nothing worth saying on screen: a subtitle that keeps its last value
      // is better than an error where a measurement was.
      rows = [];
    }

    if (rows.length === 0) {
      stopTicking();
      return;
    }

    live = Object.fromEntries(rows.map((row) => [row.id, row.subtitle]));
  }

  function startTicking() {
    if (ticking) return;

    void tick();
    ticking = setInterval(() => void tick(), 1000);
  }

  function stopTicking() {
    if (!ticking) return;

    clearInterval(ticking);
    ticking = undefined;

    // Left as it was rather than blanked. The launcher is on its way out and
    // emptying the row first would be a flicker on the way.
  }
  let version = $state(0);
  let toast = $state<{ title: string; style: string } | null>(null);
  let status = $state("");

  /**
   * Why file search cannot answer, when it cannot.
   *
   * Held rather than asked for per search. It changes when a program starts or
   * stops, so it is re-read on summon, which is the only moment it can have
   * changed without the launcher hearing about it.
   */
  let fileSearchGap = $state<FileSearchMissing | null>(null);
  let searchInput = $state<HTMLInputElement | null>(null);
  let formView = $state<ReturnType<typeof FormView> | null>(null);
  let panelOpen = $state(false);

  /*
   * The field takes the keyboard back when the panel closes.
   *
   * The panel has a filter box of its own, so while it is open the field does
   * not have focus, and nothing was giving it back. Anything that runs an
   * action and then expects typing got nothing: picking "Move to Folder" left
   * the launcher showing a list of folders with the caret still in a box that
   * was no longer on screen, so typing a folder name did nothing at all and
   * Enter took whatever was first. Renaming had the same hole.
   *
   * An effect rather than a line at each of the eight places the panel closes,
   * because the ninth is the one that would be forgotten.
   */
  $effect(() => {
    if (!panelOpen) searchInput?.focus();
  });

  /**
   * What the action panel is filtered to.
   *
   * A file offers eleven things now, and a list of eleven is a list somebody
   * reads rather than one they use. Typing narrows it, the same way typing
   * narrows everything else here.
   */
  let panelFilter = $state("");
  let panelSelected = $state(0);
  let prefs = $state<Preferences | null>(null);

  /**
   * Pins or unpins a widget, and saves.
   *
   * Written here rather than in the board so the chin redraws from the same
   * `prefs` the board just changed: two copies of what is pinned would
   * disagree for exactly as long as it takes somebody to notice.
   */
  async function setPinned(id: string, pinned: boolean) {
    if (!prefs) return;

    const was = prefs.widgets.pinned.filter((one) => one !== id);
    // Appended, so the chin's order is the order things were pinned in.
    prefs.widgets.pinned = pinned ? [...was, id] : was;

    try {
      await setPreferences($state.snapshot(prefs));
    } catch (err) {
      status = `could not save that: ${err}`;
    }
  }
  /**
   * Whether to offer looking it up on the web.
   *
   * Read from the settings Sill already loads rather than asked for again, and
   * true until they arrive: the row costs nothing and appearing a moment late
   * on the first summon of a session would be more surprising than the
   * alternative.
   */
  const webSearchEnabled = $derived(prefs?.webSearch?.enabled ?? true);
  /**
   * The program a web search will open in, for the row's icon.
   *
   * Asked once on the way in. The default browser does not change while
   * somebody is typing, and a row that has to wait on a lookup before it can
   * be drawn would be the slowest row in the list.
   */
  let browser = $state<string | null>(null);
  let clipboardView = $state<ReturnType<typeof ClipboardView> | null>(null);
  let clipboardCount = $state(0);
  let storeView = $state<ReturnType<typeof StoreView> | null>(null);
  let storeCount = $state(0);
  /**
   * The store listing under the cursor, told to us by the view that holds it.
   *
   * The action panel reads a row, and the store's rows are catalogue entries
   * this page never sees otherwise. `storeRow` below is that listing in the
   * shape everything else on this page speaks, which is what lets one
   * `rowForActions` answer for every view rather than three.
   */
  let storeListing = $state<StoreRow | null>(null);
  /** Whether the store was opened on what has an update rather than on all. */
  let storeOnUpdates = $state(false);

  /**
   * A store listing as a row, so the panel can ask about it like any other.
   *
   * `store-listing` is a mode of its own in Rust, and deliberately not
   * `view` or `no-view`: those are extension *commands*, which are installed
   * and can be run. A listing is a row in somebody else's catalogue that may
   * have no files here at all, and offering to run one would be offering an
   * action that can only fail.
   *
   * The entrypoint is the extension's name because that is the string every
   * store operation already takes, and the one part of a listing that survives
   * the catalogue being fetched again: a revision changes, and the link built
   * from it changes with it.
   */
  const storeRow: RankedCommand | undefined = $derived.by(() => {
    const listing = storeListing;
    if (!listing) return undefined;

    return {
      id: `store:${listing.name}`,
      extension: "sill",
      extensionTitle: "Extension Store",
      title: listing.title,
      subtitle: listing.author,
      mode: "store-listing",
      // Not a switch, and the row shape wants to be told.
      toggle: undefined,
      entrypoint: listing.name,
      panel: "store",
      matched: [],
    };
  });

  const view = $derived.by(() => {
    version;
    return tree.top();
  });

  /**
   * What the extension said about the field above its list.
   *
   * `filtering`, `throttle`, `isLoading` and `onSearchTextChange`, all four of
   * which used to be read by nobody. The rules live in `$lib/exthost/search`
   * so they can be tested without a window.
   */
  const search = $derived.by(() => {
    version;
    return searchProps(tree.top());
  });

  /**
   * What the field narrows, which is nothing unless the extension asked.
   *
   * An extension doing its own searching gets every row it rendered drawn:
   * narrowing those as well would be Sill hiding results the extension went
   * and fetched because they do not happen to contain the letters typed.
   */
  const narrow = $derived(search.filtering ? query.trim() : "");

  /**
   * A running command's rows, flattened, narrowed and numbered once.
   *
   * The one sequence: `ListView` and `GridView` draw it and Enter runs out of
   * it, where each used to derive its own copy from the tree.
   */
  const rows = $derived.by(() => {
    version;
    const node = tree.top();
    if (!node || (node.tag !== "List" && node.tag !== "Grid")) return [];
    return rowsOf(tree, node, narrow);
  });

  /** Selectable items, which is what the arrow keys count and Enter runs. */
  const items = $derived(itemsOf(rows));

  /**
   * The word an empty command view is allowed to blame.
   *
   * Only when something actually consumed it: Sill narrowed on it, or the
   * extension was told about it. A `<List filtering={false}>` with no
   * `onSearchTextChange` ignores typing entirely, and "No results for foo"
   * there would blame a word that never reached anything.
   */
  const searchedFor = $derived(search.filtering || search.onChange ? query.trim() : "");

  /**
   * Carries typing to an extension that asked to hear it.
   *
   * One per window rather than a module-level timer, which rule 2 refuses.
   * Failures are reported only for the newest call; a slow refusal for text
   * somebody has already typed past is not news about anything on screen.
   */
  const relay = new SearchRelay({
    send: (text) => {
      const to = session;
      const handler = search.onChange;
      if (!to || !handler) return Promise.resolve(null);
      return activateHandler(to, handler, [text]);
    },
    failed: (err) => {
      status = `the command could not search: ${err}`;
    },
  });

  /**
   * The submit handler on a Form's action panel.
   *
   * A form is driven by its Action.SubmitForm rather than by a selected row,
   * so Enter has to find this instead of walking items.
   */
  const submitHandler = $derived.by((): string | undefined => {
    version;
    const node = tree.top();
    if (!node || node.tag !== "Form") return undefined;

    const panel = tree.slot(node, "actions");
    if (!panel) return undefined;

    const walk = (parent: ElementNode): ElementNode | undefined => {
      for (const child of tree.elementChildren(parent)) {
        if (child.tag === "Action.SubmitForm") return child;
        const found = walk(child);
        if (found) return found;
      }
      return undefined;
    };

    const action = walk(panel);
    const onSubmit = action?.props.onSubmit ?? action?.props.onAction;
    return isHandlerRef(onSubmit) ? onSubmit.$handler : undefined;
  });

  /**
   * How many rows the arrow keys walk.
   *
   * Where the number comes from is a property of the mode and is declared in
   * `$lib/modes`; only the "own" case still needs naming here, because each of
   * those keeps its count in a different variable. That was the last of the
   * four hand-written mode lists.
   */
  const count = $derived.by(() => {
    switch (behaviourOf(mode)?.rows) {
      case "commands":
        return commands.length;

      case "items":
        return items.length;

      // The field is a name, not a filter, so there is nothing to walk.
      case "none":
        return 0;

      case "own":
        if (mode === "clipboard") return clipboardCount;
        if (mode === "conversations") return conversationRows.length;
        if (mode === "store") return storeCount;
        return 0;

      default:
        return items.length;
    }
  });

  /**
   * Whether the field is filtering a list that is on screen and walkable.
   *
   * Only then is it a combobox. The same field is a plain one everywhere else:
   * where nothing on screen is a listbox there is nothing to point at, and
   * naming a row that is not rendered leaves a screen reader announcing
   * nothing at all. Naming a collection or an alias is typing a name rather
   * than filtering, so there is nothing to arrow through either.
   *
   * `count` rather than `commands.length`, which is the root list's number and
   * nobody else's. The clipboard, the store, the conversation list and an
   * extension's list each count their own rows, and every one of them was
   * silent because this asked the wrong question.
   *
   * The view's tag goes with it because the mode cannot answer for an
   * extension: `command` is a list, a grid, a form or a page of prose
   * depending on what was rendered.
   */
  const browsing = $derived(isBrowsing(mode, count, view?.tag));

  /**
   * The action set the panel shows.
   *
   * A list or grid takes them from the selected item; a form or detail has no
   * selected row, so its actions hang off the view itself.
   */
  const actions = $derived.by(() => {
    version;

    // At the root the actions belong to the launcher, not to an extension.
    // Raycast's Cmd+K works here too, and a menu that silently does nothing
    // in half the app is worse than no menu.
    if (mode === "clipboard") {
      // Merging is only offered once there is something to merge. An action
      // that is always listed and almost never applicable teaches people to
      // scroll past the whole panel.
      const merging =
        picked.length > 1
          ? [
              {
                id: -30,
                title: `Merge ${picked.length} Entries`,
                tag: "Sill.ClipboardMerge",
                props: {},
                shortcut: { modifiers: ["ctrl"], key: "m" },
              },
              {
                id: -31,
                title: `Merge ${picked.length} on One Line`,
                tag: "Sill.ClipboardMergeInline",
                props: {},
                shortcut: undefined,
              },
            ]
          : [];

      // Only for an entry that actually kept formatting. Offering it on a
      // line of terminal output would be offering to do nothing.
      const plain = richEntry
        ? [
            {
              id: -32,
              title: "Paste as Plain Text",
              tag: "Sill.ClipboardPastePlain",
              props: {},
              shortcut: { modifiers: ["ctrl", "shift"], key: "enter" },
            },
          ]
        : [];

      const collecting = [
        ...(picked.length
          ? [
              {
                id: -33,
                title: `Add ${picked.length} to a Collection`,
                tag: "Sill.ClipboardCollect",
                props: {},
                shortcut: undefined,
              },
            ]
          : []),
        ...(openCollection
          ? [
              {
                id: -34,
                title: `Remove from ${openCollection.name}`,
                tag: "Sill.ClipboardUncollect",
                props: {},
                shortcut: undefined,
              },
              {
                id: -35,
                title: `Delete the ${openCollection.name} Collection`,
                tag: "Sill.ClipboardForgetCollection",
                props: {},
                shortcut: undefined,
              },
            ]
          : []),
      ];

      return [
        ...merging,
        ...collecting,
        {
          id: -10,
          title: "Paste",
          tag: "Sill.ClipboardPaste",
          props: {},
          shortcut: { modifiers: [], key: "enter" },
        },
        ...plain,
        {
          id: -11,
          title: "Copy",
          tag: "Sill.ClipboardCopy",
          props: {},
          shortcut: { modifiers: ["ctrl"], key: "c" },
        },
        {
          id: -12,
          title: "Pin or Unpin",
          tag: "Sill.ClipboardPin",
          props: {},
          shortcut: { modifiers: ["ctrl"], key: "p" },
        },
        {
          id: -13,
          title: "Next Type",
          tag: "Sill.ClipboardFilter",
          props: {},
          shortcut: { modifiers: ["ctrl"], key: "t" },
        },
        {
          id: -14,
          title: "Delete",
          tag: "Sill.ClipboardDelete",
          props: {},
          // Ctrl, because the search field has focus and a bare Delete while
          // filtering used to destroy the row under the cursor instead of the
          // character being typed. With nothing typed the bare key still
          // works; the panel advertises the one that always does.
          shortcut: { modifiers: ["ctrl"], key: "delete" },
        },
        // What can be done to the text itself, from the same registry the
        // root list draws from. Paste, pin, filter and delete above act on
        // the list; these act on the content.
        //
        // The registry's primary for a clipboard row is a plain Copy, which
        // this view already offers above. Showing it twice under two
        // shortcuts is worse than either.
        ...clipboardActions
          .filter((action) => !action.primary)
          .map((action, index) => ({
          id: -20 - index,
          title: action.title,
          tag: `Sill.Action:${action.id}`,
          props: {},
          // The action's own, from Rust. These used to arrive with none at
          // all, so Read Aloud sat on the clipboard list advertising nothing
          // while the four rows above it advertised chords written by hand.
          shortcut: action.shortcut,
        })),
      ] as typeof extensionActions;
    }

    // Whatever Rust says can be done to the selected result. This used to be
    // two entries written here by hand, which meant the panel and the Enter
    // key were two separate opinions about what a result supports.
    if (hasRowActions(mode)) {
      const chosen = rowForActions;

      // Naming a result is offered on the result, not buried in settings.
      // An alias nobody can reach is one nobody sets, and the launcher is
      // where you are when you notice you want one.
      /*
       * Only where a name would still mean something tomorrow.
       *
       * An alias points at a command id, so it is only worth offering on a
       * row whose id survives a restart. A calculator answer exists for as
       * long as it is on screen, a window's id is a handle that stops being
       * valid when it closes, and a program's audio session carries the
       * process number in it, so naming one would be naming this morning's
       * copy of that program. A running process is that last one exactly: its
       * id **is** the process number.
       */
      const namable =
        chosen &&
        chosen.mode !== "answer" &&
        chosen.mode !== "window" &&
        chosen.mode !== "audio-session" &&
        chosen.mode !== "process" &&
        // An alias points at a command id and is matched against the index. A
        // conversation is not in the index, so a name given to one would find
        // nothing however carefully it was chosen.
        chosen.mode !== "conversation" &&
        chosen.mode !== "past-conversation" &&
        // Nor is an extension in the store, for a stronger version of the same
        // reason: it may not be installed at all, so there is nothing on this
        // machine for a name to point at. Once it is installed its commands
        // are in the index and each can be named there.
        chosen.mode !== "store-listing";

      const naming =
        namable
          ? [
              {
                id: -40,
                title: chosen.alias ? `Rename "${chosen.alias}"` : "Give It a Name",
                tag: "Sill.SetAlias",
                props: {},
                shortcut: undefined,
              },
              ...(chosen.alias
                ? [
                    {
                      id: -41,
                      title: `Forget the Name "${chosen.alias}"`,
                      tag: "Sill.ClearAlias",
                      props: {},
                      shortcut: undefined,
                    },
                  ]
                : []),
            ]
          : [];

      return [
        ...rootActions.map((action, index) => ({
        id: -1 - index,
        title: action.title,
        tag: `Sill.Action:${action.id}`,
        props: {},
        /*
         * Enter for the primary one, and otherwise whatever the action says.
         *
         * Enter stays written here rather than being declared in Rust,
         * because for the primary action it is not a shortcut: it is the
         * `open` movement, handled by the chord map with everything the
         * launcher does on the way out. Declaring it as a shortcut as well
         * would put two handlers on one key.
         */
        shortcut: action.primary
          ? { modifiers: [], key: "enter" }
          : action.shortcut,
        })),
        ...naming,
      ] as typeof extensionActions;
    }

    const node = tree.top();
    if (!node) return [];

    if (node.tag === "List" || node.tag === "Grid") {
      const item = items[selected];
      return item ? collectActions(tree, item) : [];
    }

    return collectActions(tree, node);
  });

  /** Only used to give the root list's synthetic actions the right type. */
  const extensionActions: ReturnType<typeof collectActions> = [];

  /**
   * What the action registry says can be done to the selected result.
   *
   * Fetched rather than derived because the answer lives in Rust, which is
   * where it has to live: the same list drives Enter, and later a global
   * shortcut and a workflow step. A second copy here would be a second
   * opinion about what a result supports.
   */
  /**
   * The actions on screen, which is what selection counts through.
   *
   * Matched on the title alone. An action id reads like `sill.file.verify` and
   * nobody types that; the title is the thing they just looked at.
   */
  const shownActions = $derived.by(() => {
    const needle = panelFilter.trim().toLowerCase();
    if (!needle) return actions;

    return actions.filter((action) => action.title.toLowerCase().includes(needle));
  });

  // A filter that narrows the list past the selection would leave it pointing
  // at nothing, and Enter would do nothing with no sign of why.
  $effect(() => {
    if (panelSelected >= shownActions.length) panelSelected = 0;
  });

  /*
   * The same, for a running command's rows.
   *
   * Two things shorten that list without the selection hearing about it: the
   * field narrowing it, and the extension itself re-rendering fewer rows after
   * an action removed one. Typing resets the highlight where it happens, so
   * this is the second case, and it is the one that leaves a launcher looking
   * broken rather than empty: the row under the cursor is gone, Enter finds
   * nothing at that index, and nothing on screen says why.
   */
  $effect(() => {
    if (mode === "command" && selected >= items.length) selected = 0;
  });

  let rootActions = $state<ActionInfo[]>([]);

  /**
   * What can be done to a clipboard row.
   *
   * Fetched once rather than per selection: every row in the history is the
   * same kind of thing, so the answer cannot differ between them.
   */
  let clipboardActions = $state<ActionInfo[]>([]);

/*
 * What each mode behaves like lives in `$lib/modes`.
 *
 * There were four of these lists across three files: what draws its own view,
 * what answers Escape, what the arrow keys walk, and what has an action panel.
 * Every one of them had silently done nothing for a mode somebody forgot to
 * add, and one still was: `output` was in neither the "draws its own" list nor
 * the list of ordinary lists, so the script output rendered with a stale
 * result list under it and dead arrow keys.
 *
 * One table, keyed by the mode union, so a mode with no entry does not
 * compile.
 */

  /**
   * The last action that can be taken back, named by its place in the log.
   *
   * One deep on purpose. A launcher is not a document editor, and an undo
   * stack that goes back through a morning of copies would mostly be a way to
   * put back something nobody wanted.
   *
   * An id rather than the reversal itself. Holding the reversal meant Ctrl+Z
   * never told the log anything, so the entry stayed undoable and Advanced
   * would happily do it again.
   */
  let lastUndo = $state<number | null>(null);

  /**
   * The kind the action list on screen belongs to.
   *
   * The comment below used to say the effect was "keyed on the kind, so
   * arrowing through a list of applications asks once rather than once per
   * row", and it was not: the key guarded only the *assignment*, so the call
   * still went out every time. Since the effect also reads `commands`, and a
   * single keystroke rebuilt that list up to six times as each source came
   * back, one keystroke asked what a row could do **six times over**, plus
   * once more per arrow key.
   *
   * Remembering it is the whole fix. Kept outside the effect deliberately: a
   * `$state` here would make the effect depend on what it writes.
   */
  /**
   * The row the action panel acts on.
   *
   * Not always `commands[selected]`. A view that draws its own list counts
   * through its own rows, and two do: asking `commands` there returned
   * whatever the last ordinary search had left in it, which is why Ctrl+K said
   * "no actions here" on a row that plainly has some.
   *
   * The conversation list holds its rows here and the store holds them inside
   * its own component, so one is read and the other is told. Both end up in
   * one place rather than three, which is what the panel was reading
   * `commands` in three places for.
   */
  const rowForActions = $derived.by(() => {
    if (!hasRowActions(mode)) return undefined;
    if (mode === "conversations") return conversationRows[selected];
    // The store keeps its rows inside its own view and tells us which one is
    // under the cursor; `commands` there holds whatever the last ordinary
    // search left in it.
    if (mode === "store") return storeRow;

    return commands[selected];
  });

  let askedFor: string | null = null;

  $effect(() => {
    const command = rowForActions;

    if (!command) {
      rootActions = [];
      askedFor = null;
      return;
    }

    // The answer depends only on the kind, and it has not changed.
    const wanted = command.mode;
    if (wanted === askedFor) return;

    askedFor = wanted;

    void actionsFor(wanted).then((list) => {
      // The selection moved to a different kind while this was in flight.
      if (rowForActions?.mode === wanted) rootActions = list;
    });
  });

  /**
   * Which query the results on screen belong to.
   *
   * Two searches can be in flight at once and they can resolve in either
   * order. Without this the older one can land last and put stale results
   * under a current query, which reads as the launcher ignoring what you
   * typed. Both halves share one counter: a newer keystroke invalidates the
   * command search and the file search alike.
   */
  let searchId = 0;

  /** The file query waiting to run, so a burst of typing runs one, not ten. */
  let fileTimer: ReturnType<typeof setTimeout> | undefined;

  /**
   * How long the file half waits before it runs.
   *
   * Only the file half waits. Everything lives behind a window message and a
   * blocking task, which is tens of milliseconds; the command search ranks the
   * whole index in Rust and is a fraction of one. Debouncing that too would
   * add lag to the fast path for no reason, which is the mistake every
   * launcher that feels sluggish has made.
   */
  const FILE_SEARCH_DEBOUNCE_MS = 120;

  /*
   * Re-reads the switches on screen after one of them has been pressed.
   *
   * Every one of them, not just the one pressed, because one switch can move
   * another: the audio outputs are a single choice spread over several rows,
   * so turning Speakers on turns the monitors off and nothing about the
   * Speakers row says so.
   *
   * A re-search would answer this too, and would also re-rank: the launch has
   * just been recorded, so the row would climb out from under the cursor of
   * somebody who wanted to press it twice. This asks about the rows already
   * drawn and leaves the order alone.
   */
  async function refreshSwitches() {
    const rows = commands.filter((row) => row.toggle !== undefined);
    if (rows.length === 0) return;

    try {
      const now = await systemStates(rows.map((row) => row.id));
      const by = new Map(rows.map((row, at) => [row.id, now[at]]));

      commands = commands.map((row) => {
        const state = by.get(row.id);
        return state === undefined || state === null ? row : { ...row, toggle: state };
      });
    } catch (err) {
      // The switch itself worked, and saying so would be the wrong story.
      status = `${status}, but the row may be out of date: ${err}`;
    }
  }

  /**
   * What a failed search says, and what a working one clears.
   *
   * Shared rather than written twice, because the clearing has to recognise
   * exactly what the failing wrote.
   */
  const TROUBLE = "search failed: ";

  async function refreshRoot() {
    const current = query;
    const id = ++searchId;

    // Typing leaves the history behind. Anything else would make the next Up
    // continue walking from a place the user has already edited away from.
    if (!current) walked = -1;

    clearTimeout(fileTimer);

    if (mode === "destination") {
      const source = moving;
      if (!source) return;

      try {
        const found = await searchDestinations(current, source.path);
        if (id !== searchId) return;

        show(found, current);
      } catch (err) {
        if (id === searchId) status = `could not look for folders: ${err}`;
      }
      return;
    }

    // Nothing to search. The readout is a picture of the machine rather than a
    // list of rows, and it keeps itself up to date.
    if (mode === "widgets" || mode === "namingWorkspace") return;

    // The store searches a catalogue rather than the index, and does it in
    // Rust. Running the index search here as well would put applications
    // underneath extensions in a view about extensions.
    if (mode === "store") return;

    if (mode === "appVolume") {
      try {
        const found = await searchAppVolume(current);
        if (id !== searchId) return;

        show(found, current);
      } catch (err) {
        if (id === searchId) status = `could not read the volumes: ${err}`;
      }
      return;
    }

    if (mode === "processes") {
      try {
        const found = await searchProcesses(current);
        if (id !== searchId) return;

        show(found, current);
      } catch (err) {
        if (id === searchId) status = `could not read what is running: ${err}`;
      }
      return;
    }

    if (mode === "emoji") {
      try {
        const found = await searchEmoji(current);
        if (id !== searchId) return;

        show(found, current);
      } catch (err) {
        if (id === searchId) status = `emoji search failed: ${err}`;
      }
      return;
    }

    // The switcher is windows and nothing else. Mixing applications in would
    // mean "Chrome" the program sitting beside "Chrome" the window you are
    // trying to get back to, and no way to tell which is which at a glance.
    if (mode === "switcher") {
      try {
        const open = await searchWindows(current);
        if (id !== searchId) return;

        show(open, current);
      } catch (err) {
        if (id === searchId) status = `window search failed: ${err}`;
      }
      return;
    }

    try {
      const ranked = await searchCommands(current);
      if (id !== searchId) return;

      show(ranked, current);

      /*
       * A search that worked clears what a search that failed said.
       *
       * The page mounts and searches before `setup` has finished managing what
       * the command needs, so the very first search of a run can fail. Nothing
       * cleared the line it wrote, and it sat under a list of perfectly good
       * results for the rest of the session, saying the search had failed.
       *
       * Only the search's own line: a message from an action that just ran is
       * the more recent thing and is what somebody is reading.
       */
      if (status.startsWith(TROUBLE)) status = "";
    } catch (err) {
      if (id === searchId) status = `${TROUBLE}${err}`;
      return;
    }

    if (!current.trim()) return;

    /*
     * Open windows are not asked for separately any more.
     *
     * They came back from their own command and were appended after the
     * command results had already been capped, so on a short query an exact
     * window title landed past the cap and was never seen. Rust ranks them in
     * the same pass now, which is both one fewer round trip per keystroke and
     * the only way a window can outrank a weak command match.
     */

    /*
     * Emoji are not asked for separately any more.
     *
     * They came back from their own command and were spliced into the results
     * here, which meant a second round trip per keystroke, the list rebuilt
     * twice for one keystroke, and the rule about where they go living in
     * TypeScript. Rust ranks them in the same pass now and places them itself.
     *
     * They are still a separate corpus: two thousand entries beside fifteen
     * hundred real ones would swamp the list, and only plainly named ones are
     * offered, because their names are ordinary words.
     */

    // Nothing will come back, and saying so beats an empty space where files
    // should be. One row, only once something has been typed, and only when
    // file search is switched on: somebody who turned it off does not need
    // telling that it is off.
    if (fileSearchGap) {
      show([...commands, fileSearchRow(fileSearchGap)], current);
    }

    // Files and browser pages are appended after the commands, so a slower
    // query against either can never reorder or delay what is already shown.
    //
    // One timer for both. They are the two sources that read somebody else's
    // files rather than Sill's index, they are the two that are worth waiting
    // a moment before asking, and giving them separate timers would only mean
    // two chances to fire on a query that has already been replaced.
    fileTimer = setTimeout(async () => {
      /*
       * Both slow sources in one call.
       *
       * Two commands before, awaited one after the other, so this cost two
       * round trips and the browser search did not start until the file
       * search had finished. Rust runs them at the same time and answers
       * once, which is also one list rebuild here instead of two.
       */
      try {
        const found = await searchElsewhere(current);
        if (id !== searchId) return;

        show(
          [
            ...commands,
            ...found.files.map(fileAsCommand),
            ...found.pages.map(browserAsCommand),
          ],
          current,
        );
      } catch (err) {
        if (id === searchId) status = `file search failed: ${err}`;
      }

      /*
       * Looking it up on the web is the last thing offered, always.
       *
       * Last because it answers anything. A row that matches every query would
       * displace a real result the moment it ranked above one, so it is not
       * ranked at all: it goes at the bottom, after everything that actually
       * matched, and is only reached by somebody who has looked past all of it.
       *
       * Here rather than earlier so it lands after the files and pages that
       * this same timer appends. It costs nothing to build and asks Rust for
       * nothing, so it is not what the wait is for.
       */
      if (id !== searchId) return;

      const typed = current.trim();

      /*
       * An address, and a path, before the offer to look words up.
       *
       * Both are destinations rather than questions. Typing an address and
       * pressing Enter used to search the web for the address itself, and
       * typing a path did nothing at all: a path is not in the index, is not
       * a command, and the file search matches names rather than whole paths.
       *
       * Here rather than earlier so they land after the real results, which
       * is where somebody who typed something the index knew about is
       * looking. Above the web search row, which answers anything and so
       * goes last.
       */
      const offers: RankedCommand[] = [];

      if (isUrl(typed)) offers.push(urlRow(asUrl(typed), browser ?? undefined));
      if (isPath(typed)) offers.push(pathRow(typed));
      if (webSearchEnabled) offers.push(webSearchRow(typed, browser ?? undefined));

      if (offers.length) show([...commands, ...offers], current);
    }, FILE_SEARCH_DEBOUNCE_MS);
  }

  /**
   * The commands this window opens itself, rather than launching.
   *
   * Each becomes a mode in the launcher or acts on something it already
   * holds, so there is nothing for Rust's action registry to run: `RunBuiltin`
   * has no arm for any of these eight and answers "unknown Sill command".
   * They worked only because the root list intercepted them before
   * `launchCommand` was ever reached.
   *
   * That interception living inside `openSelected` made it unreachable from
   * anywhere else, and the tray menu reaches for exactly this: its "Clipboard
   * History" row emits `sill://run`, which called `launchCommand` straight out
   * and got the error rather than the history. One list, both callers.
   *
   * Returns whether it handled the id, so a caller can fall through.
   */
  async function openHere(id: string, typed = ""): Promise<boolean> {
    // Its own view rather than a window: browsed the same way the root list
    // is, with the same field and the same keys.
    if (id === "sill:appVolume") {
      void recordUse(id, typed);
      mode = "appVolume";
      selected = 0;
      query = "";
      return true;
    }

    if (id === "sill:processes") {
      void recordUse(id, typed);
      mode = "processes";
      selected = 0;
      query = "";
      return true;
    }

    // Handled here rather than by the action, because running a builtin
    // dismisses the launcher: the mode was switching and the window was
    // hiding a moment later, so the name went nowhere. Naming happens in the
    // field, and the field has to still be on screen.
    if (id === "sill:save-workspace") {
      void recordUse(id, typed);
      mode = "namingWorkspace";
      selected = 0;
      query = "";
      return true;
    }

    if (id === "sill:widgets") {
      void recordUse(id, typed);
      mode = "widgets";
      selected = 0;
      query = "";
      return true;
    }

    if (id === "sill:emoji") {
      void recordUse(id, typed);
      mode = "emoji";
      selected = 0;
      query = "";
      return true;
    }

    // Picking an area puts an overlay over every screen, so the launcher has
    // nothing more to do than get out of the way, which Rust does.
    if (id === "sill:capture-area") {
      void recordUse(id, typed);
      try {
        await beginCapture();
      } catch (err) {
        status = `${err}`;
      }
      return true;
    }

    if (id === "sill:capture-screen") {
      void recordUse(id, typed);
      try {
        status = await captureScreen();
      } catch (err) {
        status = `${err}`;
      }
      return true;
    }

    // Marking up opens a window of its own on the last picture copied. It goes
    // through the action registry, so the row and the clipboard's own panel
    // entry are one implementation.
    if (id === "sill:mark-up") {
      void recordUse(id, typed);
      try {
        const image = await lastImage();
        if (image === null) {
          status = "nothing has been copied as a picture yet";
          return true;
        }
        await openMarkup(image);
      } catch (err) {
        status = `${err}`;
      }
      return true;
    }

    /*
     * Reads the last picture copied, without opening anything.
     *
     * A row rather than only an action buried in the clipboard's panel,
     * because a capability nobody can find is a capability nobody has. It is
     * reached by typing "ocr", "read text" or "screenshot".
     */
    if (id === "sill:extract-text") {
      void recordUse(id, typed);
      try {
        status = await extractTextFromLastImage();
      } catch (err) {
        status = `${err}`;
      }
      return true;
    }

    if (id === "sill:conversations") {
      void recordUse(id, typed);
      await openConversations();
      return true;
    }

    // Opened here rather than launched, but it is still a use, and ranking has
    // to see it or the history can never rise in the root list however often
    // it is reached for.
    if (id === "sill:clipboard") {
      void recordUse(id, typed);
      mode = "clipboard";
      selected = 0;
      query = "";
      return true;
    }

    /*
     * Two rows into one view.
     *
     * "Update Extensions" is the store opened on what has a newer version
     * published, not a second surface. Updating is installing at a newer
     * commit, including the screen that says what the new version reaches, so
     * a separate path would be the same code with a different name and one
     * fewer thing read.
     */
    if (id === "sill:store" || id === "sill:store-updates") {
      void recordUse(id, typed);
      storeOnUpdates = id === "sill:store-updates";
      mode = "store";
      selected = 0;
      query = "";
      return true;
    }

    return false;
  }

  /**
   * Leaves the store, and lets go of what it was holding.
   *
   * Every way out goes through here. The catalogue is two megabytes of
   * somebody else's product listings and there is no version of "at rest, do
   * almost nothing" where it outlives the view: a path out that only changed
   * the mode would leave it resident until the launcher was restarted.
   */
  async function leaveStore() {
    mode = "root";
    selected = 0;
    query = "";
    storeOnUpdates = false;
    await storeClose();
    await refreshRoot();
  }

  /**
   * Takes back the extension under the cursor in the store.
   *
   * One function, called by the chord and reached by the panel entry beside
   * it, and both are the same registry action. The alternative is two ways to
   * remove an extension with nothing making them agree, which is the shape
   * this codebase has been bitten by five times.
   */
  async function removeExtension() {
    const listing = storeRow;
    if (!listing) return;

    try {
      const outcome = await runObjectAction("sill.store.remove", asTarget(listing));
      status = outcome.message;
      await storeView?.reload();
    } catch (err) {
      status = `${err}`;
    }
  }

  async function openSelected() {
    if (mode === "conversations") {
      const row = conversationRows[selected];
      if (row) await resumeConversation(row.entrypoint, row.title);
      return;
    }

    if (mode === "clipboard") {
      await clipboardView?.paste(true);
      return;
    }

    if (mode === "store") {
      await storeView?.activate();
      return;
    }

    if (mode === "argument") {
      const asked = awaiting;
      if (!asked) return;

      if (asked.what === "script") {
        const [, ...rest] = asked.fields ?? [];
        const given = [...(asked.given ?? []), query];

        if (rest.length > 0) {
          awaiting = { ...asked, link: rest[0] ?? "", fields: rest, given };
          query = "";
          return;
        }

        awaiting = null;
        await startScript(asked.id, asked.title, given);
        return;
      }

      if (asked.what === "snippet") {
        const [current, ...rest] = asked.fields ?? [];
        if (!current) return;

        const filled = { ...(asked.filled ?? {}), [current]: query };

        // More to ask: the same field, a new question. Nothing is pasted
        // until every hole has an answer, so backing out half way leaves
        // whatever was being typed into untouched.
        if (rest.length > 0) {
          awaiting = { ...asked, link: rest[0] ?? "", fields: rest, filled };
          query = "";
          return;
        }

        try {
          await pasteSnippetFilled(asked.id, filled);
          await dismiss();
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      if (asked.what === "rename") {
        // The registry, with what was typed as the action's argument. It was a
        // command of its own that did the renaming itself, which made renaming
        // the one thing on this list no key could be bound to.
        if (!asked.of) return;

        try {
          status = (await runObjectAction("sill.file.rename", asked.of, query)).message;
          awaiting = null;
          mode = "root";
          query = "";
          selected = 0;
          await refreshRoot();
        } catch (err) {
          // Left in the field, because the name that failed is the one worth
          // editing rather than retyping.
          status = `${err}`;
        }
        return;
      }

      try {
        status = `opening ${asked.title}`;
        await openQuicklink(asked.id, query);
        await dismiss();
      } catch (err) {
        status = `${err}`;
      }
      return;
    }

    if (mode === "alias") {
      const target = naming;
      if (!target) return;

      try {
        prefs = await setAlias(target.id, query);
        status = query.trim()
          ? `${target.title} answers to "${query.trim().toLowerCase()}"`
          : `Forgot the name for ${target.title}`;
      } catch (err) {
        status = `${err}`;
      }

      naming = null;
      mode = "root";
      query = "";
      selected = 0;
      await refreshRoot();
      return;
    }

    if (mode === "namingWorkspace") {
      const name = query.trim();
      if (!name) return;

      try {
        const saved = await saveWorkspace(name);
        status = `Saved ${name}, ${saved} ${saved === 1 ? "window" : "windows"}`;
      } catch (err) {
        status = `${err}`;
      }

      mode = "root";
      query = "";
      selected = 0;
      await refreshRoot();
      return;
    }

    if (mode === "collection") {
      const name = query.trim();
      if (!name) return;

      try {
        const added = await clipboardView?.addPickedTo(name);
        status = added ? `Added ${added} to ${name}` : `Nothing added to ${name}`;
      } catch (err) {
        status = `${err}`;
      }

      mode = "clipboard";
      query = "";
      selected = 0;
      return;
    }

    if (mode === "emoji") {
      const emoji = commands[selected];
      if (!emoji) return;

      await useEmoji(emoji);
      return;
    }

    /*
     * In a conversation, Enter asks the next thing.
     *
     * The field is the composer, so a follow-up is typed where the question
     * was. Nothing is sent while an answer is still arriving: two questions
     * in flight would interleave their answers into one paragraph.
     */
    if (mode === "ai") {
      // A card is the only thing on screen worth answering, so Enter answers
      // it rather than sending a question into a turn that is not listening.
      if (asked) {
        decide(true);
        return;
      }

      const next = query.trim();
      if (!next || asking) return;

      await askAi(next);
      return;
    }

    if (mode === "destination") {
      const source = moving;
      const folder = commands[selected];
      if (!source || !folder) return;

      try {
        // The folder picked is the action's argument. Same shape as the
        // rename above, and the same reason: moving was a command of its own,
        // so it was reachable from this page and from nowhere else.
        const outcome = await runObjectAction(
          "sill.file.move",
          source.of,
          folder.entrypoint,
        );
        status = outcome.message;
        lastUndo = outcome.undoneBy ?? null;

        moving = null;
        mode = "root";
        selected = 0;
        query = "";
        await refreshRoot();
      } catch (err) {
        // Left where it is, because the folder that refused is the one worth
        // looking at rather than starting the search again.
        status = `${err}`;
      }
      return;
    }

    /*
     * Muting a program leaves the list where it is.
     *
     * The same reasoning as a Windows switch: the question is whether it went
     * quiet, and closing the window is not an answer to it. The row redraws,
     * and the rest redraw with it because turning one program down is often
     * the first of several.
     */
    if (mode === "appVolume") {
      const session = commands[selected];
      if (!session) return;

      try {
        const outcome = await runObjectAction("sill.audio.session.mute", asTarget(session));
        status = outcome.message;
        await refreshRoot();
      } catch (err) {
        status = `${err}`;
      }
      return;
    }

    /*
     * Enter asks a program to close, and never ends one.
     *
     * Named here rather than left to the primary lookup so that the one key
     * somebody presses without thinking is bound to the safe half of the pair
     * in the window as well as in the registry. Force Quit has no key at all:
     * it is reached through Ctrl+K, which is a deliberate act.
     *
     * The list stays where it is, like the volume list. A program asked to
     * close may put up "save changes?" and still be there, and closing the
     * launcher would take away the row that says whether it went.
     */
    if (mode === "processes") {
      const process = commands[selected];
      if (!process) return;

      try {
        const outcome = await runObjectAction("sill.process.quit", asTarget(process));
        status = outcome.message;
        await refreshRoot();
      } catch (err) {
        status = `${err}`;
      }
      return;
    }

    if (mode === "switcher") {
      const window = commands[selected];
      if (!window) return;

      try {
        await runObjectAction("sill.window.focus", asTarget(window));
        await dismiss();
      } catch (err) {
        status = `${err}`;
      }
      return;
    }

    if (mode === "root") {
      const command = commands[selected];
      if (!command) return;

      // Found in an ordinary search rather than through the picker, and it
      // behaves identically once it is on screen.
      if (command.mode === "emoji") {
        await useEmoji(command);
        return;
      }

      /*
       * The answer to a sum, which Enter copies.
       *
       * Intercepted here for the same reason a file or a window is: the row is
       * built by the search rather than found in the index, so `launch_command`
       * cannot look its id up and answered "no such command: sill:answer" for
       * every sum anybody ever pressed Enter on. The action dismisses the
       * launcher itself, which is why nothing follows this.
       */
      if (command.mode === "answer") {
        try {
          const outcome = await runObjectAction("sill.copyAnswer", asTarget(command));
          status = outcome.message;
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      // The row standing in for the files that could not be searched.
      if (command.mode === "file-setup") {
        try {
          status = await startFileSearch();
          // Asked again rather than assumed: starting a program is a request,
          // not a result, and the row should stay until it is actually gone.
          fileSearchGap = await fileSearchMissing();
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      // Anything this window opens itself, which is also what the tray menu
      // asks for. See `openHere`.
      if (await openHere(command.id, query)) return;

      /*
       * The conversation you left, reopened rather than launched.
       *
       * Intercepted here for the same reason the clipboard is: it opens a mode
       * in this window, and there is nothing for the action registry to run.
       */
      if (command.mode === "conversation") {
        void recordUse(command.id, query);
        void resumeConversation(command.entrypoint, command.title);
        return;
      }


      /*
       * A script, asked about and then watched.
       *
       * Kept here rather than launched through the action registry for the two
       * things the registry cannot do: ask for the arguments the script's own
       * header declares, and show the output while it is still running with a
       * way to stop it. The action stays for the model and for a script run
       * from anywhere else.
       */
      if (command.mode === "script" || command.mode === "script-arg") {
        void recordUse(command.id, query);

        /*
         * Silent, and the fallback is the safe half of the choice.
         *
         * An empty list means the script runs with nothing filled in, which is
         * what a script that declares no arguments does anyway, and the script
         * then says for itself what it was missing in the output panel that is
         * already open. The alternative reading, refusing to run, would turn a
         * failed read into a launcher that will not launch things.
         */
        const asks = await scriptArguments(command.entrypoint).catch(() => []);

        if (asks.length > 0) {
          awaiting = {
            what: "script",
            id: command.entrypoint,
            title: command.title,
            link: asks[0] ?? "",
            fields: asks,
            given: [],
          };
          mode = "argument";
          selected = 0;
          query = "";
          return;
        }

        await startScript(command.entrypoint, command.title, []);
        return;
      }

      /*
       * A snippet with named holes, asked about one at a time.
       *
       * Asked here rather than in the action, because an action takes an
       * object and has nowhere to stop and ask. The keyword expander takes
       * neither path: it fires while somebody is typing into another program,
       * where there is no field to borrow, so a snippet with holes expands
       * there with the holes still in it.
       *
       * Only when there is something to ask. A snippet without holes is
       * pasted by pressing Enter once, as it always was.
       */
      if (command.mode === "snippet") {
        // Silent. An empty list pastes the snippet with its holes still in
        // it, which is exactly what the keyword expander does with the same
        // snippet, and it is visible in the text the moment it lands.
        const holes = await snippetFields(command.entrypoint).catch(() => []);

        if (holes.length > 0) {
          void recordUse(command.id, query);
          awaiting = {
            what: "snippet",
            id: command.entrypoint,
            title: command.title,
            link: holes[0] ?? "",
            fields: holes,
            filled: {},
          };
          mode = "argument";
          selected = 0;
          query = "";
          return;
        }
      }

      // A quicklink with a hole in it. Kept here rather than launched, so the
      // field can be handed over to filling the hole.
      if (command.mode === "quicklink-arg") {
        void recordUse(command.id, query);
        awaiting = {
          what: "quicklink",
          id: command.entrypoint,
          title: command.title,
          link: command.subtitle,
        };
        mode = "argument";
        selected = 0;
        query = "";
        return;
      }

      /*
       * A file is not in the index, so there is nothing to launch by id.
       *
       * It used to fall through to `launchCommand`, which looks a record up by
       * id and fails: a file result's id is `file:` and its path, and the
       * index holds no such record. So **every file result failed to open**,
       * with "no such command" in the status line, and the branch below that
       * knew how to open one was never reached.
       *
       * The use is still recorded. Ranking has to see it, or a file reached
       * for daily never rises in the list.
       */
      if (command.mode === "file") {
        void recordUse(command.id, query);
        try {
          await openPath(command.entrypoint);
          await dismiss();
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      // A search is not in the index either, and there is not even an address
      // yet: the words are carried and Rust turns them into one, because which
      // engine to use is a setting and the escaping is the part that is easy to
      // get wrong.
      if (command.mode === "websearch") {
        try {
          await runObjectAction("sill.searchWeb", asTarget(command));
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      // Neither is a page a browser remembers, and for the same reason: it
      // was read out of somebody else's database when the query was typed and
      // it is gone again afterwards. The action registry knows how to open an
      // address, so that is what opens it.
      if (command.mode === "url") {
        try {
          await runObjectAction("sill.openUrl", asTarget(command));
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      // A window is not in the index, so there is nothing to launch. It is
      // switched to through the action registry, the same way the action
      // panel would do it, so there is one implementation of "switch to".
      if (command.mode === "window") {
        try {
          await runObjectAction("sill.window.focus", asTarget(command));
          await dismiss();
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      try {
        status = `opening ${command.title}`;
        // The query goes with it, so Sill learns the user's own shorthand
        // for this rather than only that they opened it.
        const launched = await launchCommand(command.id, query);

        /*
         * A switch flips under the cursor instead of the launcher closing.
         *
         * Turning Wi-Fi off and having the window vanish gives no answer to
         * the only question worth asking, which is whether it went off. The
         * row shows the new state, so the switch is watched happening and can
         * be pressed straight back.
         *
         * The state is written onto the row rather than searched for again:
         * one row changed, and re-ranking the whole index would also move it.
         * Whether to close is Rust's decision and it has already made it, so
         * there is nothing to do here for a one-shot like Lock Screen.
         */
        if (launched.mode === "system") {
          status = launched.message;

          // Absent means Rust is closing the window: a volume nudge has no
          // state for a row to show, so there is nothing left to draw and
          // drawing it would be work on a window nobody is looking at.
          if (launched.toggle === undefined) return;

          await refreshSwitches();
          return;
        }

        // A no-view command does its work and exits without rendering, so
        // switching to the command view would strand the UI on an empty
        // screen waiting for a tree that never arrives.
        if (launched.mode === "no-view") {
          status = `Ran ${launched.title}`;
          return;
        }

        /*
         * Nothing to render, so nothing to switch to.
         *
         * A command view draws what an extension renders, and Rust hands back
         * the session it renders into. An empty session means the work is
         * already finished: an application was launched, a link was opened, an
         * arrangement was restored, a setting was shown in a window of its own.
         *
         * This used to be a list of four modes checked *before* the two
         * branches above, and **the modes it left out are the bug**. Anything
         * unlisted fell through to the command view below and sat there with
         * an empty session, so the next summon came back to a blank screen
         * wearing the title of whatever was last opened, with Escape the only
         * way out. `sill-setting`, `quicklink`, `workspace` and
         * `audio-session` all reached it.
         *
         * Written as the rule rather than the cases, because the cases are
         * what went stale. The two modes that have no session and are not
         * finished either are answered above, deliberately, and that ordering
         * is why this is here rather than where the list was.
         */
        if (launched.session === "") {
          if (launched.message) status = launched.message;
          await dismiss();
          return;
        }

        /*
         * It may already be dead.
         *
         * A module refused at `require` throws while the extension is loading,
         * which is over before this line runs, so the crash can arrive between
         * the load returning a session and this adopting it. Entering the
         * command view anyway leaves the launcher waiting for a first render
         * from a worker that is gone, which is what "it does nothing" looks
         * like from the outside.
         */
        const already = died.get(launched.session);
        if (already !== undefined) {
          died.delete(launched.session);
          status = `${launched.title} stopped: ${already}`;
          return;
        }

        tree.reset();
        version++;
        // Whatever was typed at the last command is not typed at this one, and
        // a throttled call still waiting would arrive at a stranger.
        relay.cancel();
        session = launched.session;
        running = { title: launched.title, extensionTitle: launched.extensionTitle };
        mode = "command";
        selected = 0;
        query = "";
        status = "";
      } catch (err) {
        status = `could not open: ${err}`;
      }
      return;
    }

    // A form has no selected row: Enter means submit.
    if (view?.tag === "Form") {
      formView?.submit();
      return;
    }

    // Raycast treats the first declared action as the primary one, and it is
    // often a built-in like Action.CopyToClipboard rather than a callback.
    if (actions.length === 0) {
      status = "that item has no actions";
      return;
    }
    await runAction(0);
  }

  /**
   * Which entries are picked for merging.
   *
   * Mirrored up here because the action panel is drawn by this component and
   * has to know whether merging applies. The view owns the picking; this is a
   * copy kept in step, not a second opinion about it.
   */
  let picked = $state<number[]>([]);

  /**
   * Whether the highlighted history entry kept a formatted version.
   *
   * Mirrored up here for the same reason the picks are: the action panel is
   * drawn by this component, and an action that would do nothing should not
   * be listed.
   */
  let richEntry = $state(false);

  /**
   * The collection open in the history, when one is.
   *
   * Mirrored up here with the picks and the rich flag, for the same reason:
   * the action panel is drawn by this component and has to know which actions
   * apply. Removing something from a collection is only a thing while looking
   * at one.
   */
  let openCollection = $state<{ id: number; name: string } | null>(null);

  /**
   * Every chord that moves around, and what it means.
   *
   * Fetched rather than written here: the answer depends on a preset and on
   * whatever has been overridden, and Rust is where that is resolved.
   */
  let navKeys = $state<Record<string, MoveKey>>({});

  /**
   * How far a screenful moves.
   *
   * The rows the window actually shows, not a number that used to match them.
   * It was fixed at eight while the setting ranges from four to sixteen, so
   * Page Down moved two screens at one end and half of one at the other.
   */
  const page = $derived(prefs?.appearance.visibleRows ?? 8);

  /**
   * Past queries, and how far back through them the user has walked.
   *
   * Up recalls only from the top of an empty root list, which is the one
   * moment pressing Up means nothing else: there is no row above to move to
   * and no text to move through. Overloading it anywhere else would take the
   * arrow key away from navigating, which is the same mistake the navigation
   * presets are careful not to make.
   */
  let past = $state<string[]>([]);
  let walked = $state(-1);

  /**
   * Whether Up should reach for a past query rather than move the selection.
   *
   * Two conditions, and the second one is the part that took a real test to
   * find. "Nothing above to move to" is `selected === 0`. "Not in the middle
   * of editing" is **not** the same as an empty field: the launcher keeps the
   * last query across summons by default and selects it so typing replaces it,
   * so the field is almost never actually empty and a rule that required that
   * made this unreachable in ordinary use.
   *
   * Fully selected counts as empty, which is the same rule a shell follows and
   * the same thing `selectQueryOnSummon` already means: this text is about to
   * be replaced.
   */
  function recalling(): boolean {
    if (mode !== "root" || selected !== 0 || past.length === 0) return false;
    if (!query.trim()) return true;

    const field = searchInput;
    return (
      !!field &&
      field.selectionStart === 0 &&
      field.selectionEnd === query.length
    );
  }

  let rootList = $state<ReturnType<typeof RootList> | null>(null);

  /** The result being given a name, while the field holds the name. */
  let naming = $state<{ id: string; title: string } | null>(null);

  /** The separator a plain merge uses: one entry per line. */
  const NEWLINE = String.fromCharCode(10);

  /**
   * Joins the picked entries and puts the result on the clipboard.
   *
   * Copies rather than pastes. A merge is usually assembled and then used
   * somewhere deliberate, and pasting several entries into whatever happens to
   * be behind the launcher is not something to do without being asked.
   */
  async function mergePicked(separator: string) {
    if (picked.length < 2) return;

    try {
      const text = await clipboardMerge(picked, separator);
      const outcome = await runObjectAction("sill.clipboard.copy", {
        id: "merged",
        mode: "text",
        target: text,
        title: `${picked.length} entries`,
      });

      lastUndo = outcome.undoneBy ?? null;
      status = `Merged ${picked.length} entries`;
      clipboardView?.clearPicks();
      picked = [];
      panelOpen = false;
    } catch (err) {
      status = `${err}`;
    }
  }

  /**
   * The row that appears where files would have been.
   *
   * A row rather than a message under the field, because a message is
   * something to read and this is something to do. It sits with the files it
   * is standing in for, and Enter fixes the thing it names.
   */
  function fileSearchRow(why: FileSearchMissing): RankedCommand {
    const said = {
      indexing: {
        title: "Reading your files",
        subtitle: "Sill is going through your folders for the first time. This takes a moment and happens once.",
      },
      absent: {
        title: "Turn on file search",
        subtitle: "Sill is not indexing any folders. Choose this to start, and it will read the ones you work in.",
      },
      asleep: {
        title: "Start file search",
        subtitle: "Everything is installed but not running, so there is nothing to search. Choose this to start it.",
      },
    }[why];

    return {
      id: "sill:file-search",
      extension: "sill",
      extensionTitle: "Files",
      title: said.title,
      subtitle: said.subtitle,
      mode: "file-setup",
      entrypoint: "",
      matched: [],
    };
  }



  /**
   * Pastes or copies one emoji, and remembers what was typed to reach it.
   *
   * One implementation because there are two ways to reach one: the picker,
   * and finding it in an ordinary search. Two copies would drift the first
   * time the primary action or the learning changed.
   */
  async function useEmoji(emoji: RankedCommand) {
    // What was typed, remembered against the emoji itself. Search "party",
    // choose the popper, and next time "party" finds it first. Raycast does
    // this with a model; Sill already learns query to result for everything
    // else, so here it is the same mechanism.
    //
    // A query typed at the root DOES go into the history, unlike one typed in
    // the picker: Up recalling it will find the same emoji again.
    void recordUse(emoji.id, query, mode === "root");

    try {
      if ((prefs?.emoji.primary ?? "paste") === "paste") {
        // Dismissed first: pasting means putting it back where the user was
        // typing, and Sill has to stop being the foreground window for that
        // to mean anything.
        await dismiss();
        await runObjectAction("sill.emoji.paste", asTarget(emoji));
      } else {
        await runObjectAction("sill.clipboard.copy", asTarget(emoji));
        await dismiss();
      }
    } catch (err) {
      status = `${err}`;
    }

    mode = "root";
    selected = 0;
    query = "";
  }

  /** Runs an action chosen from the panel. -1 means dismiss without running. */
  async function runAction(index: number) {
    panelOpen = false;
    if (index < 0) return;

    // The filtered list, because that is the one on screen and the one the
    // index came from. Reading the unfiltered one here would run whichever
    // action happened to sit at that position before the filter narrowed it.
    const action = shownActions[index];
    if (!action) return;

    // The session guard belongs on the extension path below, not here: the
    // root list and the clipboard have actions of their own and no session,
    // so checking it up front made every one of them silently do nothing.
    if (mode === "clipboard") {
      if (action.tag.startsWith("Sill.Action:")) {
        const entry = clipboardView?.selection();
        if (!entry) return;

        /*
         * The whole entry, asked for by id.
         *
         * A listing carries a preview rather than the text, because four
         * hundred rows of it went to the window on every keystroke. So the
         * row in hand is not what an action should be given: transforming a
         * preview and copying the result back would quietly truncate what
         * somebody had saved.
         */
        const whole = await clipboardEntry(entry.id);
        const text = whole?.text ?? entry.text;

        try {
          const outcome = await runObjectAction(action.tag.slice("Sill.Action:".length), {
            id: String(entry.id),
            mode: "clipboard",
            target: text,
            // The row's own text, trimmed to something a status line can hold.
            title: text.slice(0, 40),
          });
          status = outcome.message;
          lastUndo = outcome.undoneBy ?? null;
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      switch (action.tag) {
        case "Sill.ClipboardPaste":
          await clipboardView?.paste(true);
          break;
        case "Sill.ClipboardCopy":
          await clipboardView?.paste(false);
          break;
        case "Sill.ClipboardPastePlain":
          await clipboardView?.paste(true, true);
          break;
        case "Sill.ClipboardCollect":
          // The field becomes the name, the way it does for a quicklink that
          // needs a query. One way of asking for a word, not two.
          mode = "collection";
          query = "";
          panelOpen = false;
          return;
        case "Sill.ClipboardUncollect":
          await clipboardView?.removeFromCollection();
          break;
        case "Sill.ClipboardForgetCollection":
          await clipboardView?.forgetCollection();
          break;
        case "Sill.SetAlias": {
          const chosen = commands[selected];
          if (!chosen) return;
          naming = { id: chosen.id, title: chosen.title };
          mode = "alias";
          query = chosen.alias ?? "";
          panelOpen = false;
          return;
        }
        case "Sill.ClearAlias": {
          const chosen = commands[selected];
          if (!chosen) return;
          try {
            prefs = await setAlias(chosen.id, "");
            status = `Forgot the name for ${chosen.title}`;
            await refreshRoot();
          } catch (err) {
            status = `${err}`;
          }
          break;
        }
        case "Sill.ClipboardPin":
          await clipboardView?.togglePin();
          break;
        case "Sill.ClipboardMerge":
          await mergePicked(NEWLINE);
          break;
        case "Sill.ClipboardMergeInline":
          await mergePicked(" ");
          break;
        case "Sill.ClipboardFilter":
          clipboardView?.cycleFilter(1);
          break;
        case "Sill.ClipboardDelete":
          await clipboardView?.remove();
          break;
      }
      return;
    }

    // The root list's actions come from Rust's registry, so running one is a
    // matter of naming it. The window does not decide what any of them mean.
    //
    // The volume list is here too rather than in a branch of its own: its rows
    // are ordinary results carrying an ordinary kind, and the registry already
    // knows what can be done to one. A second copy of this would be a second
    // opinion about what a row supports.
    if (hasRowActions(mode)) {
      const chosen = action.tag.startsWith("Sill.Action:")
        ? action.tag.slice("Sill.Action:".length)
        : "";
      // The row the panel is showing actions for, which is not always the
      // ranked results: the conversation list and the store both count
      // through their own.
      const command = rowForActions;
      if (!chosen || !command) return;

      // The primary action goes through openSelected, which knows the two
      // things the registry does not: a quicklink with a hole in it takes
      // over the field, and an extension command switches the whole view.
      if (rootActions.find((a) => a.id === chosen)?.primary) {
        await openSelected();
        return;
      }

      /*
       * Two actions need something asked first, and the asking is the window's
       * job rather than the action's.
       *
       * Renaming borrows the field, exactly as a quicklink with a hole in it
       * does. Moving borrows the whole list instead, because the answer is a
       * folder and typing only narrows which one.
       *
       * **Only the asking is here.** Both used to do their work in a Tauri
       * command of their own, which made them the two actions nothing but this
       * page could run. What is collected goes to the registry as the action's
       * argument, and everything past that point is the same code a bound key
       * and the model reach.
       */
      if (chosen === "sill.file.move") {
        moving = {
          path: command.entrypoint,
          title: command.title,
          of: asTarget(command),
        };
        mode = "destination";
        selected = 0;
        query = "";
        await refreshRoot();
        return;
      }

      if (chosen === "sill.file.rename") {
        awaiting = {
          what: "rename",
          id: command.entrypoint,
          title: command.title,
          link: command.subtitle,
          of: asTarget(command),
        };
        mode = "argument";
        selected = 0;
        // The name it already has, so a small change is a small edit rather
        // than typing the whole thing again.
        query = command.title;
        return;
      }

      try {
        const outcome = await runObjectAction(chosen, asTarget(command));
        status = outcome.message;
        lastUndo = outcome.undoneBy ?? null;

        /*
         * The panel reaches the same things Enter does, so pressing one here
         * has to leave the rows saying the same thing.
         *
         * A whole re-read for the volume list, because these rows carry a
         * percentage as well as a switch and `refreshSwitches` only puts the
         * switch back: turning something down would have flipped nothing and
         * left "100%" underneath. The process list wants the same for a
         * different reason: the row acted on is the one that has just stopped
         * existing, and a list still offering to quit it is offering to quit
         * whatever inherits its number. Elsewhere the switch is the whole
         * state, and only when the row acted on was one, because copying a
         * path moves nothing and a one-shot is closing the window anyway.
         */
        if (mode === "appVolume" || mode === "processes") {
          await refreshRoot();
        } else if (mode === "conversations") {
          // Forgetting one removes the row that was acted on, so the list has
          // to be read again or the panel keeps offering actions on something
          // that is gone.
          pastConversations = await aiConversations();
          selected = Math.min(selected, Math.max(0, conversationRows.length - 1));
        } else if (mode === "store") {
          // Removing one is the row that was acted on losing its "Installed"
          // badge, and the root list losing its commands. From disk, not the
          // network: nothing about the catalogue changed.
          await storeView?.reload();
        } else if (command.toggle !== undefined) {
          await refreshSwitches();
        }
      } catch (err) {
        status = `${err}`;
      }
      return;
    }

    if (!session) return;

    try {
      // A form's submit action still expects its collected values.
      if (action.tag === "Action.SubmitForm" && view?.tag === "Form") {
        formView?.submit();
        return;
      }

      if (action.handler) {
        await activateHandler(session, action.handler);
        return;
      }

      // No callback means Raycast performs it, so Sill does.
      if (isRunnable(action)) {
        status = await performBuiltin(session, action.tag, action.props);
        return;
      }

      status = `"${action.title}" has no action attached`;
    } catch (err) {
      status = `action failed: ${err}`;
    }
  }

  /** Hands the form's collected values to the extension's onSubmit. */
  async function submitForm(values: Record<string, unknown>) {
    if (!session || !submitHandler) {
      status = "this form has no submit action";
      return;
    }

    try {
      await activateHandler(session, submitHandler, [values]);
    } catch (err) {
      status = `submit failed: ${err}`;
    }
  }

  /**
   * Opens the window switcher.
   *
   * Whatever the launcher was showing is dropped, including a running
   * extension: the key was pressed to get to another window, and arriving in
   * a half-finished command instead is not that.
   */
  /**
   * A picture of the window under the cursor, in the switcher.
   *
   * Fetched for the selected row only, never for the list: opening the
   * switcher on twenty windows must not photograph twenty windows.
   *
   * Debounced, because holding Down walks the list faster than a window can
   * be photographed, and every one passed through on the way would be a
   * capture nobody looked at.
   */
  let preview = $state<string | null>(null);
  let previewTimer: ReturnType<typeof setTimeout> | undefined;

  /** Which row the picture on screen belongs to, so a stale one is dropped. */
  let previewOf = "";

  const PREVIEW_SETTLE_MS = 90;

  /**
   * Drops the pictures when the switcher is left.
   *
   * A preview is a picture of a moment, and keeping them would mean showing a
   * window as it was the last time somebody looked rather than as it is. A
   * plain variable rather than state, so this only acts on the change.
   */
  let wasSwitching = false;

  $effect(() => {
    const switching = mode === "switcher";

    if (wasSwitching && !switching) {
      preview = null;
      previewOf = "";
      void forgetPreviews();
    }

    wasSwitching = switching;
  });

  $effect(() => {
    // Read so this runs again when either changes.
    const wanted = mode === "switcher" ? commands[selected]?.entrypoint : undefined;

    clearTimeout(previewTimer);

    if (!wanted) {
      preview = null;
      previewOf = "";
      return;
    }

    if (wanted === previewOf) return;

    previewTimer = setTimeout(() => {
      previewOf = wanted;

      void windowPreview(wanted)
        .then((picture) => {
          // The selection moved on while this was being taken.
          if (previewOf === wanted) preview = picture;
        })
        // A window that closed or refuses to be photographed is not an error
        // worth a message, on the surface or anywhere else. The strip is
        // simply empty, and `windowPreview` answers `null` for the same
        // reasons without failing at all.
        .catch(() => {
          if (previewOf === wanted) preview = null;
        });
    }, PREVIEW_SETTLE_MS);
  });

  /**
   * The conversation on screen.
   *
   * Held in Rust, not here: this window is closed most of the time and
   * reloaded whenever the page does, and a conversation that lived here would
   * be lost every time somebody pressed Escape. This is a copy for drawing.
   */
  let conversation = $state<Shown[]>([]);

  /** The answer being written right now, before it becomes a turn. */
  let answering = $state("");

  /** The scrolling column, so it can be kept at the bottom as text arrives. */
  let chatScroll = $state<HTMLDivElement | null>(null);

  /*
   * Kept at the bottom while an answer is being written.
   *
   * Only while it is being written, and only if the reader is already at the
   * bottom: yanking somebody back down while they are reading what was said
   * earlier is worse than letting the new text arrive out of sight.
   */
  $effect(() => {
    answering;
    conversation;

    const box = chatScroll;
    if (!box) return;

    const nearBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 80;
    if (nearBottom) box.scrollTop = box.scrollHeight;
  });

  /** Whether a question is in flight, so the composer can say so. */
  let asking = $state(false);

  /** Who answers, and why not when nothing does. */
  let aiWhoNot = $state("");

  /**
   * Asks, and switches to the conversation to watch it arrive.
   *
   * Checked before switching rather than after: a launcher that leaves the
   * results, shows an empty panel and then says "no provider" has thrown away
   * the search for nothing.
   */
  /**
   * What was in the field when the conversation started.
   *
   * Put back on the way out. Tab is autocomplete nearly everywhere else, so
   * it gets pressed by reflex, and a search that vanished because of that is
   * the one thing that would stop somebody pressing it on purpose.
   */
  let cameFrom = $state("");

  /**
   * Who answers, for the chip at the end of the field.
   *
   * Asked once when the launcher opens and again whenever the settings change,
   * never per keystroke. It is one row of managed state and a preferences
   * read; doing it on every character typed would be a lock taken fifteen
   * times a second for an answer that changes about once a month.
   */
  let answersWith = $state<AiReady | null>(null);

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

  /**
   * One turn as the window draws it: what was said, and what was looked at to
   * say it.
   *
   * The steps belong to the turn rather than to the conversation, because
   * that is what they are about. Held here only: they are provenance for the
   * moment, and a conversation reopened tomorrow is the answer rather than
   * the working.
   */
  interface Shown {
    role: string;
    text: string;
    steps: AiStep[];
  }

  /** What the model has looked at during the turn in flight. */
  let steps = $state<AiStep[]>([]);

  /**
   * What it wants to do, while nobody has said yes or no.
   *
   * At most one at a time, because the turn that raised it is paused: nothing
   * else can be asked until this is answered.
   */
  let asked = $state<AiAsking | null>(null);

  function decide(allowed: boolean) {
    if (!asked) return;
    void aiDecide(asked.id, allowed);
    asked = null;
  }

  /**
   * What to ask, offered to an empty conversation.
   *
   * Not decoration. Nothing anywhere else says the model can read this
   * machine, so somebody who does not already know asks it the questions they
   * would ask any chat window and never finds out. Each of these needs a tool
   * to answer, and each names a different one.
   */
  const OPENERS = [
    "What windows do I have open?",
    "What did I copy earlier?",
    "Find the largest files in my Downloads folder",
    "What is my volume set to?",
  ];

  /**
   * Fills the field rather than sending it.
   *
   * One more keystroke, and the keystroke is the point: an example that sends
   * itself spends money on a question somebody was reading rather than
   * asking.
   */
  function offer(question: string) {
    query = question;
    searchInput?.focus();
  }

  /** Every conversation, while the list of them is open. */
  let pastConversations = $state<AiConversation[]>([]);

  /**
   * The list, as rows.
   *
   * Built here rather than in Rust because these never go through search:
   * the list is short, it is already ordered by when each was last spoken to,
   * and filtering it is a substring test on the question.
   */
  const conversationRows: RankedCommand[] = $derived.by(() => {
    const wanted = query.trim().toLowerCase();

    return pastConversations
      .filter((one) => !wanted || one.title.toLowerCase().includes(wanted))
      .map((one) => ({
        id: `chat-row:${one.id}`,
        extension: "sill",
        extensionTitle: "Conversations",
        command: "conversation",
        title: one.title,
        subtitle: saidAbout(one),
        mode: "past-conversation" as const,
        // Not a switch, and the row shape wants to be told.
        toggle: undefined,
        entrypoint: one.id,
        panel: "ai",
        score: 0,
        matched: [],
      }));
  });

  /** What a conversation row says underneath the question. */
  function saidAbout(one: AiConversation): string {
    const when =
      one.age < 60
        ? "Just now"
        : one.age < 3600
          ? `${Math.floor(one.age / 60)} min ago`
          : one.age < 86_400
            ? `${Math.floor(one.age / 3600)} hr ago`
            : `${Math.floor(one.age / 86_400)} d ago`;

    const replies = `${one.replies} ${one.replies === 1 ? "reply" : "replies"}`;

    // Saying which one is open stops the row offering to reopen something
    // that is already open.
    return one.open ? `${when} · ${replies} · open` : `${when} · ${replies}`;
  }

  async function openConversations() {
    panelOpen = false;

    try {
      pastConversations = await aiConversations();
    } catch (err) {
      status = `${err}`;
      return;
    }

    if (pastConversations.length === 0) {
      status = "Nothing has been asked yet.";
      return;
    }

    mode = "conversations";
    selected = 0;
    query = "";
  }

  /**
   * Forgets the conversation under the cursor.
   *
   * The list comes back from Rust rather than being edited here, so what is
   * drawn is what is held rather than what this thinks removing one did.
   */
  async function forgetConversation(id: string) {
    try {
      pastConversations = await aiForget(id);
    } catch (err) {
      status = `${err}`;
      return;
    }

    // The row under the cursor is gone, so the cursor lands on what took its
    // place rather than past the end of a shorter list.
    selected = Math.min(selected, Math.max(0, conversationRows.length - 1));

    if (pastConversations.length === 0) {
      mode = "root";
      status = "Nothing left to go back to.";
      await refreshRoot();
    }
  }

  async function refreshWhoAnswers() {
    try {
      answersWith = await aiReady();
    } catch {
      // A chip that cannot say who answers says nothing, which is the same
      // launcher anybody had before this existed.
      answersWith = null;
    }
  }

  /**
   * Finishes the path in the field, if the folder has anything to add.
   *
   * Silent when there is nothing to add, which is the ordinary case while
   * somebody is still typing a folder that does not exist yet. A message
   * for that would fire on most key presses.
   */
  async function completePath() {
    const typed = query.trim();
    const done = await finishPath(typed);

    // Compared against what is in the field now, not against what was sent.
    // Reading a folder takes a moment, and replacing the field with the
    // answer to an older question would undo whatever was typed meanwhile.
    if (!done || query.trim() !== typed) return;

    query = done;
  }

  async function askAi(question: string) {
    let ready;
    try {
      ready = await aiReady();
    } catch (err) {
      status = `${err}`;
      return;
    }

    if (!ready.ready) {
      // Said where the search still is, so Escape is not needed to get back
      // to what was typed.
      status = `${ready.whyNot} Set one up in Settings.`;
      return;
    }

    /*
     * A question from the root list begins its own conversation.
     *
     * The launcher used to hold exactly one, for the life of the process, and
     * every press of Tab joined it. Nothing on screen said so, which is what
     * made it wrong: a launcher that carries hidden state between summons
     * surprises you every time. What you left is offered back as a row that
     * expires, so returning is something you choose.
     */
    const starting = mode !== "ai";

    if (starting) {
      cameFrom = question;
      conversation = [];
    }

    mode = "ai";
    selected = 0;
    aiWhoNot = "";
    answering = "";
    steps = [];
    asked = null;
    asking = true;

    // Shown immediately, so the question is on screen before the answer
    // starts rather than after.
    conversation = [...conversation, { role: "user", text: question, steps: [] }];
    query = "";

    try {
      await (starting ? aiAsk(question) : aiFollowUp(question));
    } catch (err) {
      status = `${err}`;
    }
  }

  /**
   * Reopens the conversation the root list offered back.
   *
   * The transcript comes from Rust rather than from anything the window kept:
   * the page reloads on every rebuild and the window is closed most of the
   * time, so what it holds is never the record.
   */
  async function resumeConversation(id: string, title: string) {
    try {
      // A conversation reopened is the answers, not the working: the steps
      // were provenance for the moment they happened in.
      conversation = (await aiResume(id)).map((turn) => ({ ...turn, steps: [] }));
    } catch (err) {
      status = `${err}`;
      return;
    }

    cameFrom = title;
    mode = "ai";
    selected = 0;
    aiWhoNot = "";
    answering = "";
    asking = false;
    query = "";
  }

  /**
   * Starts a fresh conversation without leaving the one open.
   *
   * The one set aside is still offered back from the root list, so this is not
   * a delete. Nothing is asked yet: the field is simply empty and waiting.
   */
  async function freshConversation() {
    await aiNew();
    conversation = [];
    answering = "";
    asking = false;
    aiWhoNot = "";
    cameFrom = "";
    query = "";
  }

  /** Starts again, keeping the mode. */
  async function newConversation() {
    await aiClear();
    conversation = [];
    answering = "";
    status = "";
  }

  async function openSwitcher() {
    panelOpen = false;
    // Whatever was on screen was a picture of a moment that has passed.
    preview = null;
    previewOf = "";
    mode = "switcher";
    selected = 0;
    query = "";
    commands = [];
    await refreshRoot();
  }

  /** Escape steps back to the root list, and only then out of the launcher. */
  async function goBack() {
    panelOpen = false;

    if (mode === "argument") {
      awaiting = null;
      mode = "root";
      selected = 0;
      query = "";
      await refreshRoot();
      return;
    }

    /*
     * Escape stops a running script before it leaves.
     *
     * Two jobs on one key, and the order is the important half: leaving first
     * and stopping never would abandon a script that is still running with no
     * way back to it, since the surface that could stop it is the one being
     * left. Pressing it again, once it has stopped, goes back as usual.
     */
    if (mode === "output") {
      if (output?.running) {
        void cancelScript(output.job);
        return;
      }

      output = null;
      mode = "root";
      selected = 0;
      query = "";
      await refreshRoot();
      return;
    }

    /*
     * Escape goes back to the results, and the conversation is kept.
     *
     * Leaving is not finishing: somebody who looks something up mid-answer and
     * comes back should find it where it was. Rust holds it, so this only
     * stops drawing it.
     */
    if (mode === "ai") {
      // Escape answers the card before it leaves. A question nobody replied to
      // holds its turn open for a minute and a half, and the action would land
      // long after somebody moved on.
      if (asked) {
        decide(false);
        return;
      }

      void aiRefusePending();

      mode = "root";
      selected = 0;
      // The search this was opened from, so leaving lands where it started
      // rather than on an empty list.
      query = cameFrom;
      await refreshRoot();
      return;
    }

    /*
     * Anything browsed goes back to the root.
     *
     * Written as "not the root and not a mode that handles Escape itself",
     * rather than as a list of the views that can be backed out of. The list
     * version stranded the widget board: it opened, and Escape did nothing,
     * because nobody had added it to a list that had no reason to be a list.
     * A view you can get into is one you can get out of, and that should be
     * true by default rather than by being remembered.
     */
    if (mode !== "root" && !handlesItsOwnEscape(mode)) {
      // Whatever was being moved is no longer being moved.
      moving = null;
      mode = "root";
      selected = 0;
      query = "";
      await refreshRoot();
      return;
    }

    if (mode === "conversations") {
      mode = "root";
      selected = 0;
      query = "";
      await refreshRoot();
      return;
    }

    // A step back rather than a way out, when there is something to step back
    // from: Escape on the screen asking whether to install returns to the
    // list, and throws away what was fetched.
    if (mode === "store") {
      if (storeView?.back()) return;
      await leaveStore();
      return;
    }

    if (mode === "clipboard") {
      // A step back rather than a way out. Escape with entries picked means
      // "not those", and closing the whole view would throw away the search
      // that found them as well.
      if (clipboardView?.clearPicks()) {
        picked = [];
        return;
      }

      mode = "root";
      selected = 0;
      query = "";
      await refreshRoot();
      return;
    }

    // Changing your mind about a name leaves the result where it was, rather
    // than dropping out of the launcher entirely.
    if (mode === "alias") {
      naming = null;
      mode = "root";
      query = "";
      selected = 0;
      await refreshRoot();
      return;
    }

    // Naming a collection steps back to the history with the picks intact,
    // so changing your mind about the name does not undo the picking.
    if (mode === "collection") {
      mode = "clipboard";
      query = "";
      selected = 0;
      return;
    }

    // Escape leaves the switcher rather than stepping back to the root list.
    // It was opened by its own key to do one thing, and dropping into a
    // general search on the way out is not what "never mind" means.
    if (mode === "switcher") {
      // The mode does not change on the way out, so the pictures are dropped
      // here rather than by the effect that watches for it.
      preview = null;
      previewOf = "";
      void forgetPreviews();

      await dismiss();
      return;
    }

    if (mode === "command") {
      const previous = session;
      session = null;
      running = null;
      mode = "root";
      selected = 0;
      query = "";
      toast = null;
      tree.reset();
      version++;
      // A throttled call still waiting would fire at a session that is on its
      // way to being unloaded, and answer with an error about a command
      // nobody is looking at any more.
      relay.cancel();
      await refreshRoot();

      // Unloaded after the UI has moved on, so tearing down a worker never
      // delays the frame the user is waiting for.
      if (previous) void unloadExtension(previous);
      return;
    }

    void dismiss();
  }

  function onKeydown(event: KeyboardEvent) {
    /*
     * A character typed while the field does not have focus still lands in it.
     *
     * The launcher is summoned by a key and typed into at once, so there is
     * always a moment where the window is up and the field is not focused yet.
     * Focus is taken as early as it can be now, but "as early as it can be" is
     * not "before the first keystroke": the summon key is released and the
     * next character is on its way. Anything landing on the document was
     * discarded, which gives back a wrong query rather than a slow one.
     *
     * Handled here rather than only at the moment of summon, because the same
     * gap exists every time focus is briefly elsewhere: after a picture is
     * dismissed, after a row is clicked, after a view of its own goes away.
     */
    if (isTyping(event) && !panelOpen && searchInput && document.activeElement !== searchInput) {
      const busy = document.activeElement;
      // An extension's own form field, or the settings search: those have
      // focus because somebody put it there, and stealing their keystrokes
      // would be a far worse bug than the one this fixes.
      const elsewhere =
        busy instanceof HTMLElement &&
        (busy.isContentEditable ||
          busy instanceof HTMLInputElement ||
          busy instanceof HTMLTextAreaElement);

      if (!elsewhere) {
        event.preventDefault();

        const typed = typedInto(
          {
            value: query,
            // The summon selects the old query so the next character replaces
            // it, and that selection is on the field whether or not it has
            // focus. Reading it here is what makes this agree with what
            // typing a moment later would have done.
            start: searchInput.selectionStart ?? query.length,
            end: searchInput.selectionEnd ?? query.length,
          },
          event.key,
        );

        query = typed.value;
        searchInput.focus();

        // After Svelte has written the value, or the caret would be placed in
        // the text that was there before this character.
        const caret = typed.caret;
        void rendered().then(() => searchInput?.setSelectionRange(caret, caret));
        return;
      }
    }

    // Ctrl+, opens settings, which is the convention in essentially every app.
    if (event.key === "," && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void openSettings();
      return;
    }

    // Ctrl+Z takes back the last action that offered it. Most do not, and
    // the key does nothing rather than claiming to have undone something.
    if (event.key.toLowerCase() === "z" && (event.ctrlKey || event.metaKey) && lastUndo) {
      event.preventDefault();
      const token = lastUndo;
      lastUndo = null;
      void undoAction(token)
        .then((message) => (status = message))
        .catch((err) => (status = `${err}`));
      return;
    }

    /*
     * Tab asks whatever is in the field.
     *
     * The gesture every launcher with an AI in it has settled on, and it is
     * the right one: the question is already typed, because searching for
     * something and asking about it start the same way. Nothing is lost if
     * the search was what you meant, because Escape comes straight back to it
     * with the words still there.
     *
     * Only from the root list, and only with something typed. In the switcher
     * or the clipboard, Tab is not free and a question about nothing is not a
     * question.
     */
    if (event.key === "Tab" && !event.ctrlKey && !event.altKey) {
      // A running command owns Tab: a form moves between its fields with it,
      // and taking that away would leave the second field unreachable.
      if (mode !== "command") {
        // Swallowed whether or not it asks anything. Left alone it is the
        // browser's own focus key, and the only other focusable thing is
        // outside this window: the field lost focus, the launcher dismissed
        // itself on blur, and a key that did nothing looked like a key that
        // closed the launcher.
        event.preventDefault();

        /*
         * A path finishes itself; anything else is a question for the model.
         *
         * Tab on `C:\Users\Bra` means the same thing here as it does in
         * every shell and every address bar, and somebody typing a path is
         * not asking anything. Asked first for that reason: the completion
         * refuses everything that is not a path, so the model still gets
         * every query that was one.
         */
        if (mode === "root" && query.trim()) {
          if (isPath(query.trim())) void completePath();
          else void askAi(query.trim());
        }
      }

      return;
    }

    /*
     * Delete forgets the conversation under the cursor.
     *
     * The same key that removes a row from the clipboard, and the same reason
     * it is not on the action panel alone: a list somebody opened to tidy up
     * wants tidying with one key rather than two.
     */
    if (mode === "conversations" && deleteMeansTheRow(event, query)) {
      event.preventDefault();
      const row = conversationRows[selected];
      if (row) void forgetConversation(row.entrypoint);
      return;
    }

    /*
     * Ctrl O moves the conversation into the window with room.
     *
     * The gesture for "this needs more space than a launcher has". Nothing is
     * carried across because nothing has to be: Rust holds the one open
     * conversation and the window reads the same one, so it opens showing
     * exactly what was on screen a moment ago.
     */
    if (
      mode === "ai" &&
      event.key.toLowerCase() === "o" &&
      (event.ctrlKey || event.metaKey)
    ) {
      event.preventDefault();
      // Opened before the launcher steps aside, and awaited. Dismissing hands
      // the screen back to whatever was in front, so doing it first, or at the
      // same time, is what put the new window behind that.
      void openAsk().then(() => dismiss());
      return;
    }

    /*
     * Ctrl N starts a fresh conversation from inside one.
     *
     * The same key that means a new document nearly everywhere else. Only in a
     * conversation, because in the root list there is nothing to be new
     * relative to.
     */
    if (
      mode === "ai" &&
      event.key.toLowerCase() === "n" &&
      (event.ctrlKey || event.metaKey)
    ) {
      event.preventDefault();
      void freshConversation();
      return;
    }

    /*
     * While the panel is open it owns the keyboard.
     *
     * Except for the chord that opened it, which closes it again. That used to
     * be a hardcoded Ctrl+K ahead of everything, which is what made the vim
     * preset a lie: Rust binds Ctrl+K to Previous under vim and says so in a
     * comment, and the window took the key anyway. The chord is asked for
     * here, so whatever the preset says opens the panel also closes it, and
     * Alt+Enter works for the first time.
     */
    if (panelOpen) {
      if (chordFrom(event) && navKeys[chordFrom(event)!] === "actions") {
        event.preventDefault();
        panelOpen = false;
        return;
      }

      // Nothing to move through, so the arrows would divide by zero.
      const count = Math.max(1, shownActions.length);

      if (event.key === "ArrowDown") {
        event.preventDefault();
        panelSelected = (panelSelected + 1) % count;
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        panelSelected = (panelSelected - 1 + count) % count;
      } else if (event.key === "Enter") {
        event.preventDefault();
        void runAction(panelSelected);
      } else if (event.key === "Escape") {
        event.preventDefault();
        // The filter goes first. Escape with something typed means "show me
        // all of them again", and closing instead loses the panel as well as
        // the filter for one keystroke.
        if (panelFilter) {
          panelFilter = "";
          panelSelected = 0;
        } else {
          // Closes the panel only; the launcher stays put.
          panelOpen = false;
        }
      }
      // Everything else falls through to the panel's own field, which has
      // focus while it is open.
      return;
    }

    /*
     * The store's own keys, which only exist while it is open.
     *
     * Ctrl rather than bare letters throughout, because the launcher's search
     * field has focus and a bare letter is somebody searching the catalogue.
     * The same reason the clipboard's keys below are all chords.
     */
    if (mode === "store") {
      const ctrl = event.ctrlKey || event.metaKey;

      if (ctrl && event.key.toLowerCase() === "r") {
        event.preventDefault();
        void storeView?.refresh();
        return;
      }
      if (ctrl && event.key.toLowerCase() === "t") {
        event.preventDefault();
        storeView?.cycleScope(event.shiftKey ? -1 : 1);
        return;
      }
      // Removing is the one destructive thing in here, so it is the awkward
      // chord rather than a single key next to the arrows.
      //
      // Through the registry, like the same entry in the panel above it. It
      // used to call Rust from inside the store view, which is how removing an
      // extension ended up being something only this page could do and
      // something the activity log never heard about.
      if (ctrl && event.shiftKey && event.key.toLowerCase() === "x") {
        event.preventDefault();
        void removeExtension();
        return;
      }
    }

    // The clipboard's own keys, which only exist while it is open. Not while
    // naming a collection: Ctrl and M there is somebody typing a name.
    if (mode === "clipboard") {
      const ctrl = event.ctrlKey || event.metaKey;
      if (ctrl && event.key.toLowerCase() === "c") {
        event.preventDefault();
        void clipboardView?.paste(false);
        return;
      }
      if (ctrl && event.key.toLowerCase() === "p") {
        event.preventDefault();
        void clipboardView?.togglePin();
        return;
      }
      if (ctrl && event.key.toLowerCase() === "t") {
        event.preventDefault();
        clipboardView?.cycleFilter(event.shiftKey ? -1 : 1);
        return;
      }
      // Ctrl and space rather than space alone: the search field has focus,
      // so a bare space is a space.
      if (ctrl && event.key === " ") {
        event.preventDefault();
        clipboardView?.togglePick();
        picked = clipboardView?.picks() ?? [];
        return;
      }
      if (ctrl && event.key.toLowerCase() === "m") {
        event.preventDefault();
        void mergePicked(NEWLINE);
        return;
      }
      if (deleteMeansTheRow(event, query)) {
        event.preventDefault();
        void clipboardView?.remove();
        return;
      }
    }

    // Ctrl and a digit jumps straight to a row, when that is switched on.
    // Checked before the map so a digit is never a chord anybody has to bind.
    if (
      prefs?.navigation.numeric &&
      (event.ctrlKey || event.metaKey) &&
      /^[1-9]$/.test(event.key)
    ) {
      event.preventDefault();
      const at = Number(event.key) - 1;
      if (at < count) {
        selected = at;
        void openSelected();
      }
      return;
    }

    /*
     * A shortcut an action advertises actually runs it.
     *
     * The panel draws these down its right hand side, so "Paste as Plain Text
     * Ctrl Shift Enter" has been on screen as a promise nothing kept. The same
     * was true of every shortcut an extension declares: rendered, never read.
     *
     * Asked before the chord map and after the mode's own keys, so navigation
     * still owns the arrows and Escape, and asked only for a keystroke that is
     * not text, so a bare letter an action happened to claim can never swallow
     * typing. `shownActions` rather than `actions`, because that is the list
     * `runAction` counts through.
     */
    const selecting =
      !!searchInput && searchInput.selectionStart !== searchInput.selectionEnd;

    // With text selected in the field, the field owns the keyboard. Ctrl+C is
    // the case that matters: the clipboard's Copy action claims it, and
    // somebody who has just selected part of what they typed means the
    // selection rather than the row.
    if (!isTyping(event) && !selecting) {
      const at = actionFor(event, shownActions);
      if (at >= 0) {
        event.preventDefault();
        void runAction(at);
        return;
      }
    }

    // One lookup rather than a chain of comparisons. Which chord means what is
    // decided in Rust, so this and the settings screen cannot disagree.
    const chord = chordFrom(event);
    const movement = chord ? navKeys[chord] : undefined;
    if (!movement) return;

    event.preventDefault();

    switch (movement) {
      case "next":
        // Walking forward out of the history, before it is navigation again.
        if (walked >= 0) {
          walked -= 1;
          query = walked >= 0 ? past[walked] : "";
          return;
        }
        if (count) selected = (selected + 1) % count;
        break;
      case "previous":
        if (recalling()) {
          walked = Math.min(walked + 1, past.length - 1);
          query = past[walked];
          // The recalled text arrives selected, so a second Up keeps
          // recalling and typing still replaces it.
          requestAnimationFrame(() => searchInput?.select());
          return;
        }
        if (count) selected = (selected - 1 + count) % count;
        break;
      case "pageDown":
        if (count) selected = Math.min(count - 1, selected + page);
        break;
      case "pageUp":
        if (count) selected = Math.max(0, selected - page);
        break;
      case "first":
        selected = 0;
        break;
      case "last":
        if (count) selected = count - 1;
        break;
      case "sectionNext":
        selected = rootList?.nextSection(selected) ?? selected;
        break;
      case "sectionPrevious":
        selected = rootList?.previousSection(selected) ?? selected;
        break;
      case "open":
        void openSelected();
        break;
      case "actions":
        if (actions.length) {
          panelOpen = true;
          // A fresh panel is an unfiltered one.
          panelFilter = "";
          panelSelected = 0;
        } else {
          // Never silent: with nothing to show, say so, or a working key
          // press is indistinguishable from a dead one.
          status = "no actions here";
        }
        break;
      case "back":
        void goBack();
        break;
    }
  }

  // Typing at the root re-ranks; inside a command the query is the extension's.
  $effect(() => {
    query;
    /*
     * Not while the field holds a name rather than a query, and not for a view
     * that filters rows it already has.
     *
     * `searchesOnType` rather than `isListMode`, which was deciding both this
     * and what the arrow keys walk. They are not the same question: the
     * clipboard and the conversation list are walkable and narrow what they
     * already hold, and re-running the index search for them answers a
     * question nobody asked.
     */
    if (searchesOnType(mode)) {
      void refreshRoot();
      return;
    }

    /*
     * Inside a command the field belongs to the extension.
     *
     * Two things can happen and which one is the extension's choice, declared
     * in `filtering`. Sill narrowing what was rendered is `rows` above and
     * needs nothing here; the extension being told is this. Both can be true,
     * because Raycast calls `onSearchTextChange` whenever it is registered and
     * an extension is allowed to fetch on typing and have Sill narrow what
     * comes back.
     *
     * The selection goes back to the top either way. It is an index into a
     * list that has just changed underneath it, and leaving it where it was
     * points the highlight at a row that has nothing to do with what was
     * typed.
     */
    if (mode === "command") {
      selected = 0;

      /*
       * The typing is what this reacts to, never the render that answers it.
       *
       * `search` reads the op-stream version, so tracking it here would make
       * every re-render an extension makes look like a keystroke: the
       * selection would jump back to the top each time a toast appeared or
       * `isLoading` went false, and Sill would offer the extension text it
       * already has once per render for as long as it kept rendering.
       */
      untrack(() => relay.offer(query, search.throttle));
    }
  });

  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    let shown: UnlistenFn | undefined;
    let switcher: UnlistenFn | undefined;
    let indexed: UnlistenFn | undefined;
    let changed: UnlistenFn | undefined;
    let ran: UnlistenFn | undefined;
    let said: UnlistenFn | undefined;
    let using: UnlistenFn | undefined;
    let wants: UnlistenFn | undefined;
    let finished: UnlistenFn | undefined;
    let wentWrong: UnlistenFn | undefined;
    let disposed = false;

    /*
     * Before anything is awaited.
     *
     * Everything below this line waits on Rust: the listeners, the
     * preferences, the default browser, the first root list. The field was
     * focused after all of it, so a summon in the first moments of the
     * application had nowhere to type. It costs nothing to do it first.
     */
    searchInput?.focus();

    (async () => {
      unlisten = await listen<UiEvent>("sill://ui", ({ payload }) => {
        // A late message from a command the user already left would otherwise
        // redraw a view that is no longer on screen.
        if ("session" in payload && session && payload.session !== session) return;

        switch (payload.kind) {
          case "render":
            tree.apply(payload.ops);
            version++;
            status = "";
            break;
          case "showToast":
          case "updateToast":
            toast = { title: payload.title, style: payload.style };
            break;
          case "hideToast":
            toast = null;
            break;
          case "showHud":
            toast = { title: payload.text, style: "success" };
            break;
          case "setSearchText":
            query = payload.text;
            // Adopted rather than relayed. The extension wrote this, and
            // handing it straight back through `onSearchTextChange` would be
            // Sill reporting the extension's own words to it as news, which
            // for an extension that sets the text from that handler is a loop.
            relay.adopt(payload.text);
            break;
          case "popToRoot":
          case "closeMainWindow":
            void goBack();
            break;
          case "crashed": {
            /*
             * Remembered as well as shown, because it can arrive before the
             * launch that caused it has finished.
             *
             * An extension refused a module at `require` dies during module
             * load, which is the first thing it does. The load itself has
             * already succeeded and handed back a session id, so the crash and
             * the launch's own continuation race, and the continuation ends
             * with `status = ""` and `mode = "command"`. When the crash won
             * that race its message was wiped and the launcher sat on an empty
             * command view for a first render that was never coming.
             *
             * That is the silent hang: the command appeared to do nothing at
             * all, and the reason had been on screen for a few milliseconds.
             */
            died.set(payload.session, payload.reason);

            // Only the session actually on screen is backed out of here. One
            // that has not been adopted yet is handled by the launch, which is
            // still running and about to look this up.
            if (session !== payload.session) break;

            // Read before goBack, which clears `running` synchronously on its
            // way to the root list, so asking afterwards gets null.
            const title = running?.title ?? "That command";
            // Back to the root list rather than staying on a view whose
            // extension is gone. Sitting there looks exactly like a slow
            // load, and nothing would ever arrive to correct the impression.
            void goBack();
            status = `${title} stopped: ${payload.reason}`;
            break;
          }
          case "closed": {
            /*
             * Sill let the command go, so what is on screen is a picture.
             *
             * A view is a worker holding a React tree, and Sill lets one go
             * once nobody can see it: when the launcher has been put away long
             * enough to sleep, and when the host has sat idle for minutes.
             * Both happen while the window is hidden, and neither is a
             * failure, so this says what happened rather than that something
             * went wrong.
             *
             * Left alone, the window comes back showing the view it had. It
             * looks like a working command and it is not: nothing renders into
             * it again and every action on it fails with "no such session".
             */
            if (session !== payload.session) break;

            const closed = running?.title ?? "That command";
            void goBack();
            status = `${closed} closed: ${payload.reason}`;
            break;
          }
        }
      });

      // Applications are discovered in two phases and the second one lands
      // about a second after the window first drew its list. Nothing re-asks
      // on its own, so without this the user stares at the shorter first
      // list until they happen to type.
      indexed = await listen<number>("sill://registry-updated", () => {
        if (mode === "root") void refreshRoot();
      });


      // Summoning must hand the keyboard straight to the search field.
      // Focusing only on mount is not enough: that runs once, while the
      // window is still hidden, and focus does not survive hide and show.
      // Once on mount as well as on every summon. The launcher starts hidden,
      // so this usually stops itself on the first answer, which costs one call
      // and means a window that was already up when this page loaded is not
      // waiting for a summon that already happened.
      startTicking();

      finishedScript = await listen<Finished>("sill://script-done", (event) => {
        // Only the one being watched. A script started, left running, and
        // followed by another would otherwise overwrite the one on screen with
        // whichever finished last.
        if (!output || output.job !== event.payload.job) return;

        output = {
          ...output,
          running: false,
          stdout: event.payload.stdout,
          stderr: event.payload.stderr,
          code: event.payload.code,
          ended: event.payload.ended,
        };
      });

      // Through `whenVisible` rather than a `listen` of its own, because an
      // event reaches every window and only that module knows which one this
      // page is. Taking focus and stamping a summon that another window was
      // shown by would be this same mistake with worse consequences.
      shown = whenVisible(() => {
        /*
         * Focus first, before anything that can wait.
         *
         * The window is already up by the time Rust says so, and the field is
         * already in the page, so there is nothing to wait for. It used to be
         * taken a frame later, and every character typed in that frame landed
         * on the document and was lost. Waiting is still done below, for the
         * selection and the measurement, but not for this.
         */
        searchInput?.focus();

        // Measuring starts when the window appears and stops when Rust says
        // nobody is looking, so a launcher nobody can see costs nothing.
        startTicking();

        /*
         * What this window last failed to read, forgotten before it asks
         * again.
         *
         * On the summon rather than on mount, because every read the launcher
         * reports sits behind this one: file search below, and the clipboard
         * view, which can only be opened after a summon. One message, on a
         * path that is already several, and it is what stops a failure that
         * has since been fixed from being reported for the life of the page.
         *
         * Scoped to this window. A flat group would mean opening settings, or
         * the capture overlay, erased what the launcher had found.
         */
        void forgetUnreadable("launcher");

        // Re-asked on every summon. A file indexer can be started or stopped
        // between two uses of the launcher, and the alternative to asking here
        // is asking on every keystroke.
        void fileSearchMissing().then((why) => (fileSearchGap = why));

        // Asked for explicitly, because the launcher is not where you were
        // when you left: reopening on a half-finished command is only right
        // if you meant to come back to it.
        if (prefs?.hotkey.resetOnSummon && mode === "command") void goBack();

        // Re-read on every summon rather than only at startup: a search done
        // a minute ago should be one Up away, and the launcher is long-lived.
        walked = -1;
        void queryHistory().then((seen) => (past = seen));

        // A frame's grace so focus lands after the window is actually up.
        requestAnimationFrame(() => {
          searchInput?.focus();
          // Selected rather than cleared, so typing replaces the old query
          // but it is still there if the summon was accidental.
          if (prefs?.hotkey.selectQueryOnSummon ?? true) searchInput?.select();

          /*
           * The summon is over, and only here is that true.
           *
           * Rust can see when it told the window to show itself. It cannot see
           * when this frame ran, and this frame is the one somebody was
           * waiting for: a window that is up and blank is not a launcher you
           * can type into. So the number is finished from here.
           *
           * Inside the same frame that takes focus rather than a frame later,
           * because taking focus is the last thing that has to happen before
           * a keystroke lands somewhere useful.
           */
          void summonPainted();
        });
      });

      // The switcher key, which shows the launcher already on the window
      // list. A separate event rather than a flag read on show, because the
      // page has to react to being opened this way even when it was already
      // sitting on something else.
      /*
       * Each piece of an answer as it arrives.
       *
       * Appended to a separate string rather than to the last turn, so a
       * half-written answer is visibly in progress and a failure part way
       * through does not leave something that looks like a finished reply.
       */
      said = await listen<string>("sill://ai-said", ({ payload }) => {
        answering += payload;
      });

      /*
       * What the model reached for, said before it runs rather than after.
       *
       * Reading the screen takes a moment, and a window showing nothing during
       * it looks like a window that has stopped. This is also the only place
       * anybody can see that a question about their machine was answered by
       * looking at their machine.
       */
      using = await listen<AiStep>("sill://ai-using", ({ payload }) => {
        steps = [...steps, payload];
      });

      /*
       * Something the model wants to do, waiting on a decision.
       *
       * The turn is genuinely paused behind this: it asked, the loop stopped,
       * and nothing else happens until Enter or Escape answers it.
       */
      wants = await listen<AiAsking>("sill://ai-asking", ({ payload }) => {
        asked = payload;
      });

      finished = await listen("sill://ai-done", () => {
        if (answering) {
          conversation = [...conversation, { role: "assistant", text: answering, steps }];
        }
        answering = "";
        asking = false;
      });

      wentWrong = await listen<string>("sill://ai-failed", ({ payload }) => {
        // Whatever arrived before it failed is kept: half an answer is often
        // enough to see what went wrong.
        if (answering) {
          conversation = [...conversation, { role: "assistant", text: answering, steps }];
        }
        answering = "";
        asking = false;
        status = payload;
      });

      switcher = await listen("sill://switcher", () => {
        void openSwitcher();
      });

      // Something outside the launcher asked for a command, which today means
      // the notification-area menu. Rust has already put the window up; this
      // is only what to show now that it is there.
      /*
       * Asked to run something by the tray menu, so it goes the way a click
       * goes: this window's own commands are opened here, and only what is
       * left is launched. Calling `launchCommand` straight out skipped every
       * one of the eight `openHere` handles, which is why the tray's
       * "Clipboard History" answered "unknown Sill command: clipboard"
       * instead of opening the history.
       */
      ran = await listen<string>("sill://run", ({ payload }) => {
        void (async () => {
          if (await openHere(payload)) return;
          await launchCommand(payload);
        })().catch((err) => {
          status = `${err}`;
        });
      });

      // The settings window writes preferences in another webview, so the
      // launcher hears about them rather than re-reading on a timer.
      changed = await listen<Preferences>("sill://preferences-changed", async ({ payload }) => {
        prefs = payload;
        applyAppearance(payload);
        // The navigation preset may have changed, and the map is resolved in
        // Rust, so it is asked for again rather than recomputed here.
        navKeys = await navigationChords();
        // Choosing a different model in Settings is exactly when the chip is
        // wrong, so it is asked again here rather than read once at startup.
        void refreshWhoAnswers();
      });

      if (disposed) return;
      clipboardActions = await actionsFor("clipboard");
      prefs = await getPreferences();
      applyAppearance(prefs);
      browser = await defaultBrowser();
      navKeys = await navigationChords();
      past = await queryHistory();
      void refreshWhoAnswers();
      await refreshRoot();
      searchInput?.focus();
    })();

    return () => {
      disposed = true;
      // A pending file query has nowhere to land once this is torn down.
      clearTimeout(fileTimer);
      // And a measurement has nobody to show it to.
      stopTicking();
      unlisten?.();
      shown?.();
      finishedScript?.();
      switcher?.();
      indexed?.();
      changed?.();
      ran?.();
      said?.();
      using?.();
      wants?.();
      finished?.();
      wentWrong?.();
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<main>
  <div class="search">
    <img class="mark" src="/sill.png" alt="" width="26" height="26" draggable="false" />
    {#if mode === "argument" && awaiting}
      <span class="crumb">{awaiting.title}</span>
    {:else if mode === "output" && output}
      <span class="crumb">{output.title}</span>
    {:else if mode === "clipboard"}
      <span class="crumb">Clipboard History</span>
    {:else if mode === "switcher"}
      <span class="crumb">Open Windows</span>
    {:else if mode === "emoji"}
      <span class="crumb">Emoji</span>
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
    {:else if mode === "destination" && moving}
      <span class="crumb">Move {moving.title}</span>
    {:else if mode === "collection"}
      <span class="crumb">Collection</span>
    {:else if mode === "alias" && naming}
      <span class="crumb">{naming.title}</span>
    {:else if running}
      <span class="crumb">{running.extensionTitle}</span>
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
      bind:this={searchInput}
      bind:value={query}
      placeholder={mode === "argument"
        ? "Type what to search for, then Enter…"
        : mode === "emoji"
          ? "Search emoji by name…"
        : mode === "ai"
          ? asking
            ? "Waiting for the answer…"
            : conversation.length === 0
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
            : String(view?.props.searchBarPlaceholder ?? "Search…")}
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

  {#if mode === "output" && output}
    <!--
      What a script printed. For `fullOutput` that is the answer rather than a
      description of where the answer is, and it stays on screen once the
      script has finished, because somebody ran it deliberately to read it.
    -->
    <div class="output">
      <p class="output-said">
        {#if output.running}
          Running {output.title}. Escape stops it.
        {:else if output.ended === "cancelled"}
          {output.title} was stopped.
        {:else if output.ended === "timedOut"}
          {output.title} ran too long and was stopped.
        {:else if output.ended === "started"}
          <!-- Before the exit code, because there is not one. Windows started
               it as administrator and a process at that level hands nothing
               back to one below it: no output, no code, and no way to stop
               it. Saying "finished" here would be claiming to know it
               worked. -->
          {output.title} was started as administrator. Sill cannot see what it does.
        {:else if output.code !== 0}
          {output.title} failed with code {output.code}.
        {:else}
          {output.title} finished.
        {/if}
      </p>

      {#if output.stdout.trim()}
        <pre class="output-text sill-scrolls">{output.stdout}</pre>
      {/if}

      {#if output.stderr.trim()}
        <!-- Kept apart from the output rather than mixed into it. A script
             that printed a result and a warning has said two things, and
             running them together loses which was which. -->
        <pre class="output-text output-wrong sill-scrolls">{output.stderr}</pre>
      {/if}

      <!-- Not for an elevated start, which printed nothing here because Sill
           was never holding its output, rather than because it was quiet. -->
      {#if !output.running && output.ended !== "started" && !output.stdout.trim() && !output.stderr.trim()}
        <p class="output-said">It printed nothing.</p>
      {/if}
    </div>
  {/if}

  {#if mode === "argument" && awaiting}
    <!-- The body would otherwise be a large empty rectangle. Showing the
         target answers the question the empty field raises, which is where
         this is about to send you. -->
    <div class="argument-hint">
      <p class="going">
        {awaiting.what === "snippet" ? `{${awaiting.link}}` : awaiting.link}
      </p>
      <p class="explains">
        <!--
          What the field is being borrowed for differs, so what this says has
          to. Telling somebody filling in a snippet that their words are
          escaped before going into an address would be describing a different
          feature at them.
        -->
        {#if awaiting.what === "snippet"}
          {snippetAsking}
        {:else if awaiting.what === "rename"}
          Enter renames it. Escape leaves it as it was.
        {:else if query.trim()}
          Enter opens it with what you typed in place of the placeholder.
        {:else}
          Type the words to search for. They are escaped before they go into the address.
        {/if}
      </p>
    </div>
  {:else if mode === "clipboard" || mode === "collection"}
    <!-- Kept mounted while the name is being typed. Unmounting it would take
         the picks with it, so changing your mind about the name would silently
         undo the picking that led to it. -->
    <ClipboardView
      bind:this={clipboardView}
      query={mode === "collection" ? "" : query}
      {selected}
      onselect={(i) => (selected = i)}
      oncount={(n) => (clipboardCount = n)}
      onpick={(ids) => (picked = ids)}
      onrich={(rich) => (richEntry = rich)}
      oncollection={(open) => (openCollection = open)}
    />
  {:else if mode === "ai"}
    <!--
      A conversation, with each side on its own.

      A question sits right in a bubble of its own and an answer sits left with
      none. That asymmetry is the point rather than an oversight: the question
      is a few words and reads as a card, the answer is prose and reads as
      prose, and boxing both makes a long answer into a wall inside a wall.
      The field below stays the composer, so a follow-up is typed where the
      question was.
    -->
    <div class="chat sill-scrolls" bind:this={chatScroll}>
      <!--
        An empty conversation says what this is and what it can reach.

        The launcher's own answer to a blank window: not a greeting, but the
        four questions that are worth asking here and nowhere else. They are
        the only place the tools are visible before one runs.
      -->
      {#if conversation.length === 0 && !asking && !answering}
        <div class="opening">
          {#if answersWith?.ready}
            <!-- An invitation rather than a label. The crumb two lines above
                 already names the model; saying it again as a heading reads as
                 the same fact twice, and as part of a sentence it does not. -->
            <p class="lead">
              <AiMark name={answersWith.id} size={15} />
              <span>Ask {answersWith.model || answersWith.name} anything</span>
            </p>
          {/if}
          <p class="reach">
            It can look through this machine to answer: what is installed and
            open, what you have copied or selected, a file or a folder, and
            what is on screen.
          </p>
          <div class="openers">
            {#each OPENERS as opener (opener)}
              <button class="opener" onclick={() => offer(opener)}>{opener}</button>
            {/each}
          </div>
        </div>
      {/if}

      {#each conversation as turn, at (at)}
        {#if turn.role === "user"}
          <article class="turn asked"><p>{turn.text}</p></article>
        {:else}
          <!--
            What was looked at, above the answer it produced.

            One line per tool with what it was used on, because ten lookups
            that all read "Searched" read as a stutter and say nothing about
            what was searched for. It stays after the answer arrives: knowing
            that a question about your machine was answered by reading your
            clipboard is part of the answer.
          -->
          <Steps steps={turn.steps} />
          <article class="turn said md"><Markdown text={turn.text} /></article>
        {/if}
      {/each}

      {#if asking}
        <Steps {steps} live />
      {/if}

      {#if asked}
        <!--
          What it wants to do, and the two keys that answer.

          Enter and Escape rather than buttons, because the field already has
          focus and reaching for a mouse to answer a question about your own
          files is the wrong shape. The keys are drawn anyway: a control that
          exists only as a keystroke nobody was told about is a control nobody
          uses.
        -->
        <div class="permission">
          <p class="wants">{asked.title}</p>
          <p class="subject">{asked.subject}</p>
          <p class="touches">This {asked.touches}.</p>
          <div class="answers">
            <button class="allow" onclick={() => decide(true)}>
              <span class="sill-key">Enter</span> Do it
            </button>
            <button class="refuse" onclick={() => decide(false)}>
              <span class="sill-key">Esc</span> Not now
            </button>
          </div>
        </div>
      {/if}

      {#if answering}
        <article class="turn said md"><Markdown text={answering} /></article>
      {:else if asking && !asked}
        <!-- Something between pressing Tab and the first token arriving,
             because a blank panel reads as nothing having happened. -->
        <p class="thinking">Thinking<span class="dots" aria-hidden="true"></span></p>
      {/if}
    </div>

  <!--
    Not `isListMode`. This set is not the same one: `alias` draws the list too,
    with the field holding a name rather than a query, so the two lists differ
    by exactly that mode and sharing one would be wrong in one direction or the
    other. Written out, and this comment is why.
  -->
  {:else if mode === "widgets"}
    <!-- A board rather than a list: nothing here is selected or run, and the
         only thing you press is the pin that puts one in the chin. -->
    <div class="listing">
      <WidgetBoard
        prefs={prefs ?? null}
        onpin={(id, pinned) => void setPinned(id, pinned)}
      />
    </div>

  {:else if mode === "store"}
    <!-- The launcher's own field is the store's search box, the same way it
         is the clipboard's filter. A view with a second search field would be
         two places to type in one window. -->
    <div class="listing">
      <StoreView
        bind:this={storeView}
        {query}
        {selected}
        prefs={prefs ?? null}
        startOnUpdates={storeOnUpdates}
        onselect={(i) => (selected = i)}
        oncount={(n) => (storeCount = n)}
        oncurrent={(row) => (storeListing = row)}
        onstatus={(said) => (status = said)}
        onchanged={() => void refreshRoot()}
      />
    </div>

  {:else if mode === "conversations"}
    <div class="listing">
      <RootList
        commands={conversationRows}
        {selected}
        {query}
        numeric={false}
        asking={`conversations:${query}`}
        onselect={(i) => (selected = i)}
        onrun={(i) => {
          selected = i;
          void openSelected();
        }}
      />
    </div>

  {:else if !drawsItsOwn(mode)}
    <!-- Kept on screen while a name is typed, so what is being named stays
         visible. -->
    <div class="listing">
      <RootList
        bind:this={rootList}
        {commands}
        {selected}
        {live}
        {query}
        numeric={prefs?.navigation.numeric ?? false}
        asking={`${mode}:${query}`}
        onselect={(i) => (selected = i)}
        onrun={(i) => {
          selected = i;
          void openSelected();
        }}
      />

      <!--
        A picture of the window under the cursor.
        
        Four browser windows are four rows reading almost the same, and a
        title cannot tell them apart. The strip keeps its width whether or not
        there is a picture in it, so arrowing past a window that refuses to be
        photographed does not shuffle the list sideways.
      -->
      {#if mode === "switcher"}
        <aside class="preview" aria-hidden="true">
          {#if preview}
            <img src={preview} alt="" />
          {/if}
        </aside>
      {/if}
    </div>
  {:else if view?.tag === "List"}
    <ListView
      node={view}
      {rows}
      query={searchedFor}
      loading={search.loading}
      {selected}
      onselect={(i) => (selected = i)}
      onrun={(i) => {
        selected = i;
        void openSelected();
      }}
    />
  {:else if view?.tag === "Grid"}
    <GridView
      node={view}
      cells={rows}
      {version}
      query={searchedFor}
      loading={search.loading}
      {selected}
      onselect={(i) => (selected = i)}
      onrun={(i) => {
        selected = i;
        void openSelected();
      }}
    />
  {:else if view?.tag === "Form"}
    <FormView bind:this={formView} {tree} node={view} {version} onsubmit={submitForm} />
  {:else if view?.tag === "Detail"}
    <div class="detail">{tree.text(view) || String(view.props.markdown ?? "")}</div>
  {:else}
    <!-- No spinner. The launcher is meant to feel instant and a spinner
         advertises that it is not; the mark plus a line of text says the same
         thing without making the wait the subject. -->
    <Instead tone="loading" headline={status || "Starting the command"} />
  {/if}

  {#if panelOpen}
    <ActionPanel
      actions={shownActions}
      selected={panelSelected}
      filter={panelFilter}
      onfilter={(text) => {
        panelFilter = text;
        panelSelected = 0;
      }}
      onselect={(i) => (panelSelected = i)}
      onrun={(i) => void runAction(i)}
    />
  {/if}

  <!--
    No divider above the footer.

    The window already carries one under the search field, and the raised
    pill below is its own edge. A second full-width rule turned the quietest
    part of the window into a boxed-in strip.
  -->
  <footer>
    <LauncherMenu
      onbuiltin={(id) => {
        const found = commands.find((c) => c.id === `sill:${id}`);
        if (found) void launchCommand(found.id);
      }}
    />
    {#if toast}
      <span class="toast" data-style={toast.style}>{toast.title}</span>
    {:else if status}
      <span class="toast">{status}</span>
    {/if}
    <span class="spacer"></span>

    <!-- Whatever is pinned, sitting in what was empty space between the
         status and the keys. -->
    <WidgetChin prefs={prefs ?? null} />

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
        onclick={() => void openSelected()}
      >
        {mode === "clipboard" ? "Paste" : mode === "root" ? "Open" : view?.tag === "Form" ? "Submit" : "Run"}
        <span class="sill-key">↵</span>
      </button>
      {#if actions.length}
        <span class="split"></span>
        <button
          class="segment"
          tabindex="-1"
          onmousedown={(e) => e.preventDefault()}
          onclick={() => {
            panelOpen = !panelOpen;
            panelSelected = 0;
          }}
        >
          Actions
          <span class="sill-key">Ctrl K</span>
        </button>
      {/if}
    </div>
  </footer>
</main>

<style>
  .output {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    overflow: hidden;
  }

  .output-said {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .output-text {
    max-height: 40vh;
    margin: 0;
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    line-height: 1.5;
    overflow: auto;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .output-wrong {
    color: var(--text-2);
  }

  /*
   * A conversation, which reads as a column of paragraphs rather than a list.
   *
   * It scrolls on its own so the field below stays put: a composer that moves
   * down the window as the answer grows is a composer you have to chase.
   */
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

  .chat {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    /* Wider between turns than inside one, so the conversation reads as
       exchanges rather than as a single column of paragraphs. */
    gap: var(--space-4);
    padding: var(--space-4) var(--space-4) var(--space-5);
  }

  .turn {
    font-size: var(--text-body);
  }

  /*
   * The question, to the right and in a ground of its own.
   *
   * Short by nature, so it can afford a bubble and gains from one: it is the
   * only thing on screen that somebody wrote themselves, and finding it again
   * in a long conversation is how you remember what you asked.
   */
  .asked {
    align-self: flex-end;
    max-width: 78%;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-lg) var(--radius-lg) var(--radius-sm) var(--radius-lg);
    background: var(--accent-fill);
    box-shadow: var(--ring-accent-faint);
  }

  .asked p {
    margin: 0;
    color: var(--text-1);
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  /*
   * The answer, to the left and unboxed.
   *
   * No ground, because prose in a box is a wall inside a wall, and the width
   * is capped where a line stops being comfortable to read rather than at the
   * window edge.
   */
  .said {
    align-self: flex-start;
    max-width: 68ch;
    width: 100%;
    color: var(--text-1);
  }

  /*
   * What was asked, set apart from what was answered.
   *
   * Quieter rather than boxed. A question is a heading for the answer under
   * it, and drawing a bubble round each turn would make a short exchange look
   * like a chat application rather than a launcher.
   */
  .asked p {
    color: var(--text-2);
  }

  /*
   * The empty conversation.
   *
   * Left aligned with the answers rather than centred, because it sits where
   * the first answer will and centring it would move everything the moment one
   * arrives.
   */
  .opening {
    align-self: flex-start;
    max-width: 62ch;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding-top: var(--space-2);
  }

  .lead {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--text-1);
    font-size: var(--text-heading);
    font-weight: var(--weight-strong);
  }

  .reach {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-body);
    line-height: 1.6;
  }

  .openers {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  /*
   * An example, which fills the field rather than sending it.
   *
   * Drawn as something pressable rather than as a bullet, because it is; and
   * quiet rather than accented, because four accented chips in an empty window
   * read as the main event when they are a way in.
   */
  .opener {
    padding: var(--space-1) var(--space-3);
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .opener:hover {
    background: var(--fill-2);
    color: var(--text-1);
  }

  .opener:focus-visible {
    outline: none;
    box-shadow: var(--ring-accent);
  }

  /*
   * The card that asks before something changes.
   *
   * The one thing in a conversation that is not a message, so it is the one
   * thing with a ground and an outline. It sits where the next answer would,
   * because that is where somebody is already looking.
   */
  .permission {
    align-self: flex-start;
    max-width: 62ch;
    width: 100%;
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    box-shadow: var(--ring-accent-faint);
  }

  .wants {
    margin: 0;
    color: var(--accent);
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  /* What it acts on, which is the line somebody actually decides on. */
  .subject {
    margin: var(--space-1) 0 0;
    color: var(--text-1);
    font-size: var(--text-body);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .touches {
    margin: var(--space-1) 0 var(--space-3);
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .answers {
    display: flex;
    gap: var(--space-2);
  }

  .answers button {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .answers button:hover {
    color: var(--text-1);
  }

  /*
   * The affirmative takes the accent, and only the affirmative.
   *
   * Two coloured buttons is two things shouting; a refusal that looks like a
   * warning also reads as the dangerous one, which is backwards.
   */
  .allow {
    background: var(--accent-fill);
    color: var(--accent);
  }

  .allow:hover {
    background: var(--accent-fill-strong);
  }

  .answers button:focus-visible {
    outline: none;
    box-shadow: var(--ring-accent);
  }

  /*
   * The wait, said without a spinner.
   *
   * A launcher is meant to feel instant and a spinner advertises that it is
   * not. Three dots that fill in say the same thing while making the wait a
   * detail rather than the subject.
   */
  .thinking {
    align-self: flex-start;
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-body);
  }

  .dots::after {
    content: "";
    animation: thinking var(--motion-pulse) steps(4, end) infinite;
  }

  @keyframes thinking {
    0% {
      content: "";
    }
    25% {
      content: ".";
    }
    50% {
      content: "..";
    }
    75% {
      content: "...";
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dots::after {
      animation: none;
      content: "…";
    }
  }

  /*
   * The list, with room beside it for a picture in the switcher.
   *
   * The list keeps its own scrolling, so this is a row of two children rather
   * than anything that reshapes it. Only the switcher gets the second child,
   * and only the switcher pays for the column: `.listing` on its own is one
   * child filling the width, which is every other mode unchanged.
   */
  .listing {
    display: flex;
    min-height: 0;
    flex: 1;
  }

  .listing > :global(*:first-child) {
    flex: 1;
    min-width: 0;
  }

  /*
   * The strip the picture is drawn in.
   *
   * A fixed width whether or not there is a picture, so arrowing past a window
   * that refuses to be photographed does not shuffle the list sideways. That
   * shuffle is worse than an empty strip: the row under the cursor moves while
   * somebody is reading it.
   */
  .preview {
    flex: none;
    width: 280px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-3);
    overflow: hidden;
  }

  .preview img {
    max-width: 100%;
    max-height: 100%;
    border-radius: var(--radius-sm);
    /* The picture is of somebody's window, which may be any colour and may
       end in a flat edge against the launcher's own. A hairline separates the
       two without drawing a frame around it. */
    box-shadow: var(--ring-outside);
    object-fit: contain;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    /* The tint sits on top; Windows composites the acrylic desktop blur
       underneath. An opaque background here would throw that away, and would
       also be the only way to get subpixel text back. See theme.css. */
    background-color: color-mix(
      in srgb,
      var(--core-secondary-background) calc((1 - var(--glass-strength)) * 100%),
      var(--surface-base)
    );
    /* Chroma above the tint. `none` in the themes that paint no wash, and
       `none` is a valid layer, so there is no conditional here. */
    background-image: var(--chroma), linear-gradient(var(--tint), var(--tint));
    border-radius: var(--radius-window);
    /* No border: DWM already clips the window to this radius, so a border here
       only stacks onto that edge. The single light inset is the glass catch. */
    box-shadow: var(--bevel-window);
    overflow: hidden;
  }

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

  .argument-hint {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-5) var(--space-3);
  }

  .going {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--text-2);
    word-break: break-all;
  }

  .explains {
    margin: 0;
    max-width: 62ch;
    font-size: var(--text-meta);
    line-height: 1.6;
    color: var(--text-3);
  }

  .detail {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-4) var(--space-3);
    color: var(--text-1);
    line-height: 1.6;
    white-space: pre-wrap;
    user-select: text;
  }

  /*
   * The chin, and it carries no surface of its own.
   *
   * It briefly had a dark wash, on the reasoning that a plane has to recede
   * for the pill to read as raised. That was wrong in practice: a full-width
   * band draws a hard line across the window and cuts the list off, which is a
   * lot of weight to spend on something whose whole job is to hold two
   * controls.
   *
   * The controls carry the layering instead. The pill is genuinely raised, on
   * its own fill and bevel, and reads that way against the window exactly as
   * the search row's chip does. Nothing else here needs a background at all.
   *
   * 8px of side padding puts the pill on the same right edge as the action
   * panel that rises out of it.
   */
  /*
   * The chin: a plane the two controls sit on, back in flow.
   *
   * It briefly had no surface and let the list dissolve underneath it, which
   * was an attempt to get a blurred chin without an opaque window. That cannot
   * work; see the note on `--chin` in theme.css. A plain recessed wash is what
   * is left, and it is honest about being a bar.
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
