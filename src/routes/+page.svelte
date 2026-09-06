<script lang="ts">
  // Aliased: this page has a `tick` of its own, which measures a running
  // command rather than waiting for the DOM.
  import { onMount, setContext, tick as rendered, untrack } from "svelte";
  import { VIEW_SESSION, type SessionOf } from "$lib/exthost/assets";
  import { beginCapture, captureScreen, lastImage, openMarkup } from "$lib/capture";
  import { onePerId } from "$lib/list";
  import { openQuicklink } from "$lib/quicklinks";
  import { saveWorkspace } from "$lib/workspaces";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { whenHidden, whenVisible } from "$lib/visible";
  import { Latency, aroundPaint, type Painted } from "$lib/latency";
  import { forgetUnreadable } from "$lib/status";
  import {
    chinLine,
    checkForUpdate,
    installUpdate,
    restartForUpdate,
    updateState,
    whenUpdateChanges,
    type Progress,
  } from "$lib/update";
  import ListView from "$lib/components/ListView.svelte";
  import GridView from "$lib/components/GridView.svelte";
  import FormView from "$lib/components/FormView.svelte";
  import DetailPane from "$lib/components/DetailPane.svelte";
  import RootList from "$lib/components/RootList.svelte";
  import Instead from "$lib/components/Instead.svelte";
  import SearchRow from "$lib/components/SearchRow.svelte";
  import Footer from "$lib/components/Footer.svelte";
  import SwitcherPreview from "$lib/components/SwitcherPreview.svelte";
  import FilePreview from "$lib/components/FilePreview.svelte";
  import AiChat from "$lib/components/AiChat.svelte";
  import { fromQuestion, fromTurn, type Shown } from "$lib/chat/parts";
  import { begin, fresh, reset, type Live } from "$lib/chat/live";
  import { listenToChat } from "$lib/chat/listen";
  import ScriptOutput, { type Ran } from "$lib/components/ScriptOutput.svelte";
  import KeySheet from "$lib/components/KeySheet.svelte";
  import PermissionAsk from "$lib/components/PermissionAsk.svelte";
  import Welcome from "$lib/components/Welcome.svelte";
  import { isBrowsing, selectionAfter, shouldLoadMore } from "$lib/results";
  import { askedForTheKeys, deleteMeansTheRow, isTyping, typedInto } from "$lib/typing";
  import { asUrl, isPath, isUrl } from "$lib/typed";
  import { conversationRows as conversationsAsRows } from "$lib/conversations";
  import { selectionRows, type SelectedObject } from "$lib/selection";
  import { fileSearchRow } from "$lib/results";
  import {
    afterLaunch,
    behaviourOf,
    drawsItsOwn,
    handlesItsOwnEscape,
    hasRowActions,
    searchesOnType,
    type Mode,
  } from "$lib/modes";
  import ActionPanel from "$lib/components/ActionPanel.svelte";
  import LauncherMenu from "$lib/components/LauncherMenu.svelte";
  import ClipboardView from "$lib/components/ClipboardView.svelte";
  import StoreView from "$lib/components/StoreView.svelte";
  import { storeClose, type StoreRow } from "$lib/store";
  import WidgetBoard from "$lib/widgets/Board.svelte";
  import WidgetChin from "$lib/widgets/Chin.svelte";
  import {
    actionFor,
    collectActions,
    isRunnable,
    toastActions,
    type ActionEntry,
    type ToastButton,
  } from "$lib/exthost/actions";
  import { clipboardPanel, rowPanel } from "$lib/panel";
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
    rememberField,
    storedFields,
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
    searchAppVolume,
    searchProcesses,
    searchControls,
    searchDestinations,
    summonPainted,
    reportPainted,
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
    startEverything,
    indexBuilding,
    welcome as askWelcome,
    // Aliased: `Welcome` is the component that draws this, imported above.
    type Welcome as Greeting,
    type FileSearchMissing,
    recordUse,
    queryHistory,
    openPath,
    browserAsCommand,
    tabAsCommand,
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
    type AiReady,
    type AiTurn,
    type RankedCommand,
  } from "$lib/exthost/commands";
  import { ViewTree, isHandlerRef, type ElementNode, type Op } from "$lib/exthost/tree";
  import { SearchRelay, itemsOf, rowsOf, searchProps } from "$lib/exthost/search";
  import { dropdownOf, paginationOf } from "$lib/exthost/present";
  import {
    applyAppearance,
    getPreferences,
    openAsk,
    openSettings,
    type Preferences,
  } from "$lib/settings";
  import "$lib/theme/theme.css";

  type UiEvent =
    | { kind: "render"; session: string; ops: Op[] }
    | {
        kind: "showToast";
        session: string;
        id: string;
        title: string;
        message: string;
        style: string;
        actions: ToastButton[];
      }
    | {
        kind: "updateToast";
        session: string;
        id: string;
        title: string;
        message: string;
        style: string;
        actions: ToastButton[];
      }
    | { kind: "hideToast"; session: string; id: string }
    | { kind: "showHud"; session: string; text: string }
    | { kind: "setSearchText"; session: string; text: string }
    | { kind: "navigation"; session: string; depth: number; pop: string }
    | { kind: "popToRoot"; session: string }
    | { kind: "closeMainWindow"; session: string }
    | { kind: "crashed"; session: string; reason: string }
    | { kind: "closed"; session: string; reason: string };

  const tree = new ViewTree();

  /**
   * Where the running command is in its own view stack.
   *
   * ## Why the window is told rather than counting
   *
   * A command that pushes a screen does it inside its worker: the stack is a
   * stack of React elements in one root, so pushing renders a different
   * element and the op stream that arrives is indistinguishable from the same
   * screen re-rendering. Counting the pushes this window started would miss
   * every `useNavigation().push()` an extension makes on its own, and a
   * miscounted depth is worse than no depth: Escape would close a command
   * somebody was two screens into.
   *
   * ## Why the handler id comes with it
   *
   * Going back means telling the worker, and the only thing this window can
   * say to a worker is "activate this handler". The worker names the one that
   * pops in the same message, so neither side holds a constant the other has
   * to keep matching.
   */
  const NOT_NAVIGATED = { depth: 1, pop: "" };

  /**
   * Rows Rust built out of the query, whose Enter is the registry's primary.
   *
   * These have no entry in the index, so `launch_command` cannot look their
   * ids up and answers "no such command" for every one of them. Every row of
   * that shape before these two got a branch of its own naming the action by
   * hand; this is the same interception with the action asked for instead,
   * which means the launcher no longer holds an opinion about what Enter does.
   *
   * `verify:source` holds this list against the modes `registry.rs` actually
   * builds rows for, so a third one cannot arrive with a dead Enter key.
   */
  const BUILT_HERE: string[] = ["note", "reminder", "font", "display-mode"];

  let nav = $state<{ depth: number; pop: string }>(NOT_NAVIGATED);

  /** Root browses installed commands; command shows one that is running. */
  /*
   * Which face the launcher is showing.
   *
   * The union comes from `modes.ts` rather than being written out again here.
   * It was written out again here, and that is the "two mode unions" the audit
   * lists under duplicate sources of truth: a mode added to the table still
   * would not compile in the window, and one added here would have no entry in
   * the table. One list, and the table is the list.
   *
   * The notes that used to sit against individual arms of this union are on
   * their entries in the table, which is where somebody reading about a mode
   * is already looking.
   */
  let mode = $state<Mode>("root");
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
    what: "quicklink" | "rename" | "snippet" | "script" | "clipRename" | "clipEdit" | "layout";
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
  // A getter, so every icon in the view can ask which session it belongs to
  // without the session being threaded through four components as a prop.
  setContext<SessionOf>(VIEW_SESSION, () => session);

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
  /**
   * What the window took to draw, kept until the launcher is put away.
   *
   * See `$lib/latency` for what is being timed and what it deliberately
   * leaves out. The short version: Rust knows when it answered and not when
   * the answer reached a screen, and the screen is the half somebody waited
   * for.
   */
  const drawn = new Latency();

  /**
   * When the keystroke now being answered was typed, if one is outstanding.
   *
   * Cleared by the first `show` that follows it, so the debounced second page
   * of results is not recorded as another keystroke: it arrives later on
   * purpose and only ever appends below what is already readable.
   *
   * Not `$state`. Nothing on screen depends on it, and making it reactive
   * would invalidate everything that reads it on every letter.
   */
  let typedAt: number | null = null;

  /**
   * The extension whose first view has not arrived yet, and when Enter was
   * pressed on it.
   *
   * Carries the session rather than only the time. An extension that fails to
   * load, or one that runs and exits without drawing, would otherwise leave a
   * stopwatch running for the next render to stop, and that render belongs to
   * something else entirely.
   */
  let launchedAt: { at: number; session: string } | null = null;

  /**
   * Stops a stopwatch around the frame that draws the answer.
   *
   * Scheduled rather than read here, because this runs before Svelte has
   * written anything to the document. See `aroundPaint`: the first frame is
   * "everything needed is in the document and the frame that draws it has
   * begun" and the one after it is "the pixels are out".
   *
   * Given one name, only the second is recorded, which is the honest single
   * number because it includes the paint. Given two, both are, which is what
   * the keystroke budget wants: there the difference between them is the
   * display's refresh interval and the budget is one frame.
   */
  function stopClock(began: number, answered: Painted, presented?: Painted) {
    const since = () => (performance.now() - began) * 1000;

    aroundPaint(
      requestAnimationFrame,
      () => {
        if (presented) drawn.record(answered, since());
      },
      () => drawn.record(presented ?? answered, since()),
    );
  }

  function show(rows: RankedCommand[], forQuery: string) {
    // Before the rows are handed over, so nothing between here and the frame
    // can start a second stopwatch for the same keystroke.
    const began = typedAt;
    typedAt = null;

    rows = onePerId(rows);

    selected = selectionAfter(
      { id: commands[selected]?.id, index: selected },
      rows,
      forQuery === showing,
    );

    commands = rows;
    showing = forQuery;

    if (began !== null) stopClock(began, "keystrokeAnswered", "keystrokePresented");
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
  let output = $state<Ran | null>(null);

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

  /**
   * Whether there is a newer Sill.
   *
   * Held rather than derived, because Rust owns the answer and pushes it here
   * through `sill://update-changed`. The chin decides what, if anything, to
   * draw from it, and for most states the answer is nothing.
   */
  let updateProgress = $state<Progress>({ kind: "unknown" });

  /**
   * Downloads the newer Sill and lets its installer take over.
   *
   * The failure is put on the status line rather than swallowed. This is a
   * button somebody pressed on purpose, so silence would be the interface
   * lying about having done something, which is the one case `status.ts` says
   * must always be reported.
   *
   * On success this call usually never returns: the installer closes Sill.
   */
  async function startUpdate() {
    /*
     * Two different presses behind one button.
     *
     * Before the download the button installs; after it, if the installer
     * handed control back rather than taking the process down, it restarts.
     * The chin says which one it is, and this reads the same state the chin
     * drew from, so the two cannot disagree.
     */
    if (updateProgress.kind === "ready") {
      void restartForUpdate();
      return;
    }

    try {
      await installUpdate();
    } catch (err) {
      status = `the update could not be installed: ${err}`;
    }
  }
  /**
   * The extension's toast, and the buttons it put on it.
   *
   * The buttons are `ActionEntry` values rather than a shape of their own, so
   * pressing one and pressing a row in the action panel are the same call.
   */
  let toast = $state<{ title: string; style: string; actions: ActionEntry[] } | null>(null);
  let status = $state("");

  /**
   * Why file search cannot answer, when it cannot.
   *
   * Held rather than asked for per search. It changes when a program starts or
   * stops, so it is re-read on summon, which is the only moment it can have
   * changed without the launcher hearing about it.
   */
  let fileSearchGap = $state<FileSearchMissing | null>(null);

  /**
   * What the welcome says, on the one start that has one.
   *
   * Every sentence of it is built in Rust from what the machine answered, and
   * the key one is what registering the summon key returned rather than what
   * the settings file asks for. Held so it can be asked again: the rows offer
   * to fix the things the paragraphs above them describe, and coming back from
   * Settings with a working key should not leave the page still saying it is
   * taken.
   */
  let greeting = $state<Greeting | null>(null);

  /**
   * Whether Sill has finished reading this machine for the first time.
   *
   * Only decides what an empty root list says. Asked on mount and again when
   * Rust announces a new index, which is twice a session rather than anything
   * that repeats.
   */
  let building = $state(false);
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
   * The picker beside the search field, when the command offers one.
   *
   * Read here rather than inside `ListView` and `GridView`, because it is
   * drawn in the search row that the page owns and a list and a grid declare
   * it identically. One reading, one control, either kind of view.
   */
  /**
   * What a picker's remembered value is filed under.
   *
   * Its own `id` when it has one, and otherwise the slot it sits in. A list
   * has exactly one picker beside its search field, so the slot names it
   * unambiguously, and it has to: the extensions that ask to be remembered
   * mostly do not name the control. Hacker News is the plain case, and
   * refusing to remember an unnamed picker would have meant `storeValue`
   * doing nothing for the extension that made it worth having.
   *
   * A form field is the other way round and is keyed by its own `id` only.
   * A form has many fields, so there is no slot to fall back on, and Raycast
   * requires the id there for the same reason.
   */
  function pickerKey(picker: { name?: string }): string {
    return picker.name ?? "searchBarAccessory";
  }

  const dropdown = $derived.by(() => {
    version;
    const node = tree.top();
    if (!node || (node.tag !== "List" && node.tag !== "Grid")) return undefined;

    const picker = dropdownOf(tree, node);
    if (!picker?.storeValue) return picker;

    /*
     * What was chosen last time, standing in for the author's default.
     *
     * That is the whole of `storeValue`: the extension says where to start and
     * the launcher says where somebody actually left it, and the second wins.
     * Folded in here rather than inside the control, because the control is
     * also drawn by the preview route with no Rust behind it, and because a
     * value the extension is driving is not this picker's to override.
     */
    const remembered = storedByField[pickerKey(picker)];
    if (remembered === undefined || typeof remembered !== "string") return picker;

    return { ...picker, initial: remembered };
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

  /**
   * What the running command's fields were left set to last time.
   *
   * Raycast's `storeValue`: a picker or a form field marked with it opens on
   * what somebody last chose rather than on what its author defaulted to.
   *
   * Held here rather than asked for by the controls, and fetched as part of
   * the launch rather than after it, because the first draw has to be right:
   * a dropdown that opened on the author's default and corrected itself a
   * moment later would tell the extension twice and fetch twice, once for an
   * answer nobody asked for.
   */
  let storedByField = $state<Record<string, unknown>>({});

  /** Which extension and command those belong to, for writing one back. */
  let storedFor = $state<{ extension: string; command: string } | null>(null);

  /**
   * Remembers what a field was set to, if the extension asked to have it
   * remembered and named the field.
   *
   * A field with no name has nowhere to be kept, so `storeValue` on one that
   * omitted its `id` does nothing rather than inventing a key that would not
   * be the same one next time.
   */
  function remember(name: string | undefined, wanted: boolean, value: unknown) {
    if (!wanted || !name || !storedFor) return;

    void rememberField(storedFor.extension, storedFor.command, name, value).catch(
      (err: unknown) => {
        status = `the launcher could not remember that: ${err}`;
      },
    );
  }

  /** Selectable items, which is what the arrow keys count and Enter runs. */
  const items = $derived(itemsOf(rows));

  /**
   * What the running list says about there being more of it.
   *
   * Read here rather than inside `ListView` for the reason the dropdown is:
   * a list and a grid declare it identically, and the cursor that decides
   * when to ask is this page's.
   */
  const more = $derived.by(() => {
    version;
    const node = tree.top();
    if (!node || (node.tag !== "List" && node.tag !== "Grid")) return undefined;
    return paginationOf(node);
  });

  /**
   * The row count the last request was made at.
   *
   * The guard against asking over and over. An extension answers by rendering
   * more rows, and until it does the cursor is still near the end and the
   * condition is still true; without this the launcher would ask on every
   * keystroke and every redraw for as long as somebody sat at the bottom of
   * the list. Reset by the count changing, which is the extension's answer
   * arriving, and by a different command taking over.
   */
  let askedAtCount = $state(-1);

  $effect(() => {
    const paging = more;
    const count = items.length;

    if (!session || !shouldLoadMore(paging, selected, count, askedAtCount)) return;

    askedAtCount = count;
    void activateHandler(session, paging.onLoadMore, []).catch((err: unknown) => {
      status = `the command could not fetch more: ${err}`;
    });
  });

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
        if (mode === "welcome") return greeting?.steps.length ?? 0;
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
  const actions = $derived.by((): ActionEntry[] => {
    version;

    // At the root the actions belong to the launcher, not to an extension.
    // Raycast's Cmd+K works here too, and a menu that silently does nothing
    // in half the app is worse than no menu.
    //
    // What each of these two offers is a function over what is picked and what
    // the registry answered, so it lives in `$lib/panel` where a test can call
    // it. Which of them applies is a question about the mode, so it is here.
    if (mode === "clipboard") {
      return clipboardPanel({
        picked,
        rich: richEntry,
        openCollection,
        registry: clipboardActions,
      });
    }

    // Whatever Rust says can be done to the selected result.
    if (hasRowActions(mode)) return rowPanel(rootActions, rowForActions);

    const node = tree.top();
    if (!node) return [];

    if (node.tag === "List" || node.tag === "Grid") {
      const item = items[selected];
      return item ? collectActions(tree, item) : [];
    }

    return collectActions(tree, node);
  });

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

    /*
     * The buttons of the window you were in, asked for again every keystroke.
     *
     * Deliberately not narrowed here. The list belongs to somebody else's
     * window and that window is free to redraw itself between two letters, so
     * filtering a list taken once would go on offering a button that has gone.
     */
    if (mode === "controls") {
      try {
        const found = await searchControls(current);
        if (id !== searchId) return;

        show(found, current);
      } catch (err) {
        if (id === searchId) status = `${err}`;
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
            // Above the remembered pages, deliberately. A tab is open right
            // now and a history entry is a page you once had open, so when
            // both match, the one already on screen is the nearer answer.
            ...found.tabs.map(tabAsCommand),
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

    // Its own view for the same reason, and one more: nothing is read out of
    // anybody's window until this row is chosen, which is what makes the
    // feature cost nothing until it is asked for.
    if (id === "sill:controls") {
      void recordUse(id, typed);
      mode = "controls";
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

    // Picking an area puts an overlay over every screen. The launcher stays
    // where it is, under the overlay and in the picture if that is the
    // picture wanted; Rust holds it there while the overlay has the keyboard.
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

  /**
   * The welcome, put on screen when Rust says one is owed.
   *
   * Asked twice on purpose, and `commands.ts` says why: this window exists
   * before the code that registers the summon key, so a question asked on
   * mount can land before the one answer the welcome exists to get right. Rust
   * answers "not yet" then, and hands the welcome to whichever of the two
   * questions arrives after the key has been asked for.
   */
  async function offerWelcome() {
    const said = await askWelcome();
    if (!said) return;

    greeting = said;
    mode = "welcome";
    selected = 0;
    query = "";
  }

  /**
   * What one row of the welcome does.
   *
   * Every branch is something the launcher already does another way. A welcome
   * is a shortcut to behaviour that exists, never a place where new behaviour
   * hides: an action only this page can reach is one no key could bind and no
   * other surface could offer.
   */
  async function runWelcomeStep(index: number) {
    const step = greeting?.steps[index];
    if (!step) return;

    switch (step.does) {
      case "chooseKey":
        await openSettings("shortcuts");
        return;

      case "chooseFolders":
        await openSettings("files");
        return;

      case "startEverything":
        try {
          status = await startEverything();
          // Asked again rather than assumed. Starting a program is a request
          // and not a result, so the row stays until the answer changes.
          greeting = (await askWelcome(true)) ?? greeting;
        } catch (err) {
          status = `${err}`;
        }
        return;

      case "showKeys":
        mode = "keys";
        selected = 0;
        return;

      case "finish":
        mode = "root";
        selected = 0;
        query = "";
        await refreshRoot();
        return;
    }
  }

  async function openSelected() {
    if (mode === "welcome") {
      await runWelcomeStep(selected);
      return;
    }

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

      if (asked.what === "layout") {
        if (!asked.of) return;

        try {
          status = (await runObjectAction("sill.window.layout", asked.of, query)).message;
          awaiting = null;
          mode = "root";
          query = "";
          selected = 0;
          await dismiss();
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      if (asked.what === "clipRename" || asked.what === "clipEdit") {
        if (!asked.of) return;
        const actionId =
          asked.what === "clipRename" ? "sill.clipboard.rename" : "sill.clipboard.edit";

        try {
          const outcome = await runObjectAction(actionId, asked.of, query);
          status = outcome.message;
          lastUndo = outcome.undoneBy ?? null;
          awaiting = null;
          mode = "clipboard";
          query = "";
          selected = 0;
          await clipboardView?.refresh();
        } catch (err) {
          // Left in the field, because the text that failed is the one worth
          // editing rather than retyping.
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
      if (aiLive.asked) {
        decide(true);
        return;
      }

      const next = query.trim();
      if (!next || aiLive.asking) return;

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

    /*
     * Enter presses the control, and then the launcher gets out of the way.
     *
     * Unlike the process list, which stays up because the answer to "did it
     * close" is the row itself. The answer here is in the window behind, and
     * dismissing hands focus back to exactly that window, because it is the
     * one the launcher took the foreground from and the one this just pressed
     * a button in.
     *
     * A refusal keeps the launcher up and says so. "Save is not in that window
     * any more" is a sentence somebody has to be able to read, and dismissing
     * on it would leave them looking at an unchanged window with no idea why.
     */
    if (mode === "controls") {
      const control = commands[selected];
      if (!control) return;

      try {
        const outcome = await runObjectAction("sill.control.press", asTarget(control));
        status = outcome.message;
        await dismiss();
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

    /*
     * Enter on something that was already selected runs whatever Enter means
     * for that kind, which is a question only the registry answers.
     *
     * Named here rather than left to fall through to the root branch because
     * these rows are not in any index: `launch_command` would look the row's
     * id up, find nothing, and answer "no such command", which is the same
     * hole a calculator answer and a media row are intercepted for.
     *
     * Then it gets out of the way. The launcher was summoned by a key to do
     * one thing to one thing, and staying up afterwards would leave a window
     * over the document or the folder somebody is now looking at.
     */
    if (mode === "selection") {
      const row = commands[selected];
      const primary = rootActions.find((action) => action.primary);
      if (!row || !primary) return;

      try {
        const outcome = await runObjectAction(primary.id, asTarget(row));
        status = outcome.message;
        lastUndo = outcome.undoneBy ?? null;
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
          // The sum rides along as the argument: the row's subtitle is what
          // was typed, and Rust remembers the pair so `sums` can bring it
          // back. The window carries it and decides nothing about it.
          const outcome = await runObjectAction(
            "sill.copyAnswer",
            asTarget(command),
            command.subtitle,
          );
          status = outcome.message;
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      /*
       * What is playing, paused or played again without the launcher leaving.
       *
       * Intercepted here for the reason the answer row is: the row was built
       * by the search rather than found in the index, so `launch_command`
       * cannot look its id up.
       *
       * The list stays where it is, like a Windows switch and like the volume
       * list. The question is whether it went quiet, and closing the window is
       * not an answer to it; the row redraws saying which way it is now.
       */
      if (command.mode === "media") {
        try {
          const outcome = await runObjectAction("sill.media.playPause", asTarget(command));
          status = outcome.message;
          await refreshRoot();
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      /*
       * A row Rust built out of the query rather than found in the index, and
       * whose Enter is whatever the registry says is primary for its kind.
       *
       * One branch for both, and for the next one too, because the shape is
       * the same every time: `launch_command` cannot look these up, so it
       * would answer "no such command" the way it did for a calculator answer,
       * a media row and a window. Naming the ids here instead of the modes
       * would be the launcher deciding what Enter means, which is the thing
       * `P0-10` moved into the registry.
       *
       * Both get out of the way afterwards, for opposite reasons that come to
       * the same thing: opening a note puts another window in front, and
       * setting a reminder is finished the moment Windows has been told.
       */
      if (BUILT_HERE.includes(command.mode)) {
        try {
          const actions = await actionsFor(command.mode);
          const primary = actions.find((action) => action.primary);

          if (!primary) {
            status = `Nothing is bound to Enter for ${command.mode}`;
            return;
          }

          const outcome = await runObjectAction(primary.id, asTarget(command));
          status = outcome.message;
          await dismiss();
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

      /*
       * A terminal profile is not in the index either.
       *
       * It is read out of Windows Terminal's settings and the WSL keys in the
       * registry when somebody types one of the words that asks, so
       * `launchCommand` would look its id up and find nothing, which is the
       * failure the comment above describes for files. The action registry is
       * what knows that a Terminal profile is opened by `wt -p` and a WSL
       * distribution with no profile by `wsl -d`, and the row carries which.
       */
      if (command.mode === "terminal-profile") {
        void recordUse(command.id, query);
        try {
          await runObjectAction("sill.terminal.open", asTarget(command));
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

      // A tab is not in the index either, for the same reason and more so: it
      // is read out of a running browser when somebody types and is gone the
      // moment they stop. The action reads the strip again and refuses rather
      // than switching to whatever has taken that tab's place, so a failure
      // here is worth showing instead of dismissing over.
      if (command.mode === "browser-tab") {
        try {
          await runObjectAction("sill.browser.tab.focus", asTarget(command));
          await dismiss();
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      try {
        status = `opening ${command.title}`;
        /*
         * From here, because from here is what somebody waited through.
         *
         * `extension_took` in Rust stops when the host hands back a session,
         * which is before anything is on screen. Between the two are a worker
         * starting, a bundle being evaluated and a first render arriving, and
         * that gap is most of what "extensions are slow" has ever meant.
         */
        const askedAt = performance.now();
        // The query goes with it, so Sill learns the user's own shorthand
        // for this rather than only that they opened it.
        const launched = await launchCommand(command.id, query);

        /*
         * What happens next is decided in `afterLaunch`, not here.
         *
         * It was four branches of prose in this function, which no test can
         * reach: the one that claimed to cover them compared a copy of the
         * list against itself, and stayed green with the `system` branch
         * deleted. The reasoning moved to `$lib/modes` with the branches; this
         * is now only the doing.
         */
        switch (afterLaunch(launched)) {
          /*
           * A switch flips under the cursor instead of the launcher closing.
           *
           * Turning Wi-Fi off and having the window vanish gives no answer to
           * the only question worth asking, which is whether it went off. The
           * row shows the new state, so the switch is watched happening and
           * can be pressed straight back.
           *
           * The state is written onto the row rather than searched for again:
           * one row changed, and re-ranking the whole index would also move
           * it.
           */
          case "switch":
            status = launched.message;
            await refreshSwitches();
            return;

          case "stay":
            // A switch with no state to show says what it did; anything else
            // reaching here ran and exited without rendering.
            status =
              launched.mode === "system" ? launched.message : `Ran ${launched.title}`;
            return;

          case "dismiss":
            if (launched.message) status = launched.message;
            await dismiss();
            return;

          case "view":
            break;
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
        // A new command starts at the bottom of its own stack. Adopted here as
        // well as on the way out, because a command can be opened from a view
        // that never went through `goBack`.
        nav = NOT_NAVIGATED;
        // A different command's row count says nothing about this one's.
        askedAtCount = -1;

        /*
         * What this command's fields were left on, before anything draws.
         *
         * Awaited rather than fetched alongside, because a picker that opened
         * on the author's default and corrected itself a moment later would
         * report both to the extension, and an extension that fetches on that
         * report would fetch twice: once for a feed nobody chose.
         *
         * A colon splits the id because that is how it is built:
         * `<extension>:<command>`. Failing is not worth stopping a launch for,
         * so a command whose last values could not be read opens on its
         * author's defaults, which is what it did before any of this.
         */
        const [extensionName, ...rest] = command.id.split(":");
        storedFor = { extension: extensionName, command: rest.join(":") };
        storedByField = await storedFields(extensionName, rest.join(":")).catch(() => ({}));

        session = launched.session;
        launchedAt = { at: askedAt, session: launched.session };
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
        const actionId = action.tag.slice("Sill.Action:".length);

        /*
         * Naming or correcting an entry borrows the field, the way renaming
         * a file does: the action wants a line of text the panel cannot
         * hold. What is prefilled is what is there now, so a small change
         * is a small edit.
         */
        if (actionId === "sill.clipboard.rename" || actionId === "sill.clipboard.edit") {
          const renaming = actionId === "sill.clipboard.rename";
          awaiting = {
            what: renaming ? "clipRename" : "clipEdit",
            id: String(entry.id),
            title: renaming ? "Name this entry" : "Edit this entry",
            link: text.slice(0, 40),
            of: {
              id: String(entry.id),
              mode: "clipboard",
              target: text,
              title: text.slice(0, 40),
            },
          };
          mode = "argument";
          selected = 0;
          query = renaming ? (whole?.title ?? "") : text;
          return;
        }

        try {
          const outcome = await runObjectAction(actionId, {
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

      if (chosen === "sill.window.layout") {
        // The layout's name, asked for in the field the way a new file name
        // is. Rust finds the layout by it; the window holds only the word.
        awaiting = {
          what: "layout",
          id: command.entrypoint,
          title: command.title,
          link: "Which layout?",
          of: asTarget(command),
        };
        mode = "argument";
        selected = 0;
        query = "";
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

    await runExtensionAction(action);
  }

  /**
   * Runs one of a running command's actions, wherever it was pressed.
   *
   * The action panel is one place. A button on the extension's own toast is
   * the other, and it comes through here rather than through a call of its
   * own: a toast button is a callback the worker registered in the same
   * registry every other callback uses, so the only thing that could differ is
   * a mistake. The session check, the built-in fallback, the message for an
   * action with nothing behind it and the wording when it throws are each
   * written once.
   */
  async function runExtensionAction(action: ActionEntry) {
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
   * The strip beside the switcher's list, while the switcher is on screen.
   *
   * It owns the picture and the capture that produces it. The two moments the
   * window has to reach in are opening the switcher on top of itself and
   * dismissing out of it, and each is a method rather than a variable shared
   * both ways.
   */
  let switcherPreview = $state<ReturnType<typeof SwitcherPreview> | null>(null);

  /**
   * Whether the list has any files in it at all.
   *
   * What decides whether the strip beside the list is drawn, rather than
   * whether the selected row is a file. A strip that came and went as somebody
   * arrowed between a file and a command would move the row they were reading;
   * this way the layout is fixed for as long as one query's results are.
   */
  /**
   * The file the strip is looking inside, when the selected row is one.
   *
   * Undefined on every other row, which empties the strip rather than leaving
   * one file's contents sitting under another row's name.
   */
  let fileUnderCursor = $derived(
    commands[selected]?.mode === "file" ? commands[selected].entrypoint : undefined,
  );

  /**
   * Whether the strip beside the list is drawn at all.
   *
   * When there is something to put in it, and not merely when the list holds
   * a file somewhere.
   *
   * It used to be `commands.some(...)`, to stop the strip appearing and
   * disappearing as somebody arrowed between a file and a command. That is a
   * real thing worth avoiding, because the row being read moves under the
   * cursor. But the price was paid on every query returning **one** file, and
   * the price is a third of the window held open and empty beside the rows
   * somebody is actually reading. Searching "game" reserved it while showing
   * seven games and nothing in the strip.
   *
   * An empty third of the window is worse to look at than a list that shifts
   * when you reach a file, so the shift is what is paid now.
   */
  let showsFilePreview = $derived(fileUnderCursor !== undefined);


  /**
   * The conversation on screen.
   *
   * Held in Rust, not here: this window is closed most of the time and
   * reloaded whenever the page does, and a conversation that lived here would
   * be lost every time somebody pressed Escape. This is a copy for drawing.
   */
  let conversation = $state<Shown[]>([]);

  /**
   * The turn in flight: what has arrived, the card, the trouble.
   *
   * One object mutated through `$lib/chat/live`, which is what the chat
   * window holds too, so the two cannot disagree about what a turn is.
   */
  let aiLive = $state<Live>(fresh());

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
   * Yes or no to the card.
   *
   * At most one is up at a time, because the turn that raised it is paused:
   * nothing else can be asked until this is answered.
   */
  function decide(allowed: boolean) {
    if (!aiLive.asked) return;
    void aiDecide(aiLive.asked.id, allowed);
    aiLive.asked = null;
  }

  /**
   * Fills the field with one of the chat's examples, rather than sending it.
   *
   * One more keystroke, and the keystroke is the point: an example that sends
   * itself spends money on a question somebody was reading rather than
   * asking. Here rather than in the panel because the field is the launcher's.
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
   * Shaped in `$lib/conversations`, where a test can read what a row says.
   */
  const conversationRows = $derived(conversationsAsRows(pastConversations, query));

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
    begin(aiLive);

    // Shown immediately, so the question is on screen before the answer
    // starts rather than after.
    conversation = [...conversation, fromQuestion(question)];
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
      // The working comes back with the answers: Rust kept the parts.
      conversation = (await aiResume(id)).map(fromTurn);
    } catch (err) {
      status = `${err}`;
      return;
    }

    cameFrom = title;
    mode = "ai";
    selected = 0;
    aiWhoNot = "";
    reset(aiLive);
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
    reset(aiLive);
    aiWhoNot = "";
    cameFrom = "";
    query = "";
  }

  /** Starts again, keeping the mode. */
  async function newConversation() {
    await aiClear();
    conversation = [];
    reset(aiLive);
    status = "";
  }

  /**
   * Puts the launcher on what was selected, with the panel already open.
   *
   * The panel opens straight away on a single thing, because that is what the
   * key meant: one file, or the paragraph that was highlighted, and a list of
   * one with a closed panel is a screen that answers nothing. On several it
   * stays closed, because there is a choice to make first about which of them
   * the action is for, and a panel over a list of three files would look like
   * it applied to all three when it applies to the one under the cursor.
   */
  function openSelection(objects: SelectedObject[]) {
    const rows = selectionRows(objects);
    // Rust does not send an empty list, and drawing one would replace whatever
    // was on screen with nothing.
    if (rows.length === 0) return;

    switcherPreview?.drop();
    moving = null;
    awaiting = null;
    mode = "selection";
    commands = rows;
    selected = 0;
    query = "";
    panelOpen = rows.length === 1;
    panelSelected = 0;
  }

  async function openSwitcher() {
    panelOpen = false;
    // Whatever was on screen was a picture of a moment that has passed.
    switcherPreview?.drop();
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
      if (aiLive.asked) {
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
      // The mode does not change on the way out, so the strip is told to let
      // go here rather than being unmounted into doing it.
      switcherPreview?.forget();

      await dismiss();
      return;
    }

    if (mode === "command") {
      const previous = session;
      session = null;
      running = null;
      // The stack went with the worker. Left behind, the next command opened
      // would start life believing it was two screens deep in the last one.
      nav = NOT_NAVIGATED;
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
    /*
     * `?` on an empty field asks what the keys are.
     *
     * The same rule Delete follows: with something typed it is a character,
     * because a launcher that eats a question mark is worse than one whose
     * reference takes two keys to reach. The launcher menu is the other way
     * in and works whatever is typed.
     */
    if (mode === "root" && askedForTheKeys(event, query)) {
      event.preventDefault();
      mode = "keys";
      return;
    }

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
      /*
       * The toast's buttons first, because the toast is the thing in front.
       *
       * Raycast gives a toast action a chord and Sill draws it, and a drawn
       * chord that does nothing is the bug `matchesShortcut` was written for.
       * Ahead of the panel because a toast is a message about what just
       * happened: while one is up with a Retry on Ctrl+R, Ctrl+R means retry
       * that, not whatever the row underneath claims.
       */
      const onToast = toast ? actionFor(event, toast.actions) : -1;
      if (onToast >= 0 && toast) {
        event.preventDefault();
        void runExtensionAction(toast.actions[onToast]);
        return;
      }

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
        /*
         * A step back inside the command before a step out of it.
         *
         * Only here, and deliberately not inside `goBack`. Six other things
         * call that: a crash, Sill letting the view go, `popToRoot`, a summon
         * that resets, and two modes of its own. Every one of them means
         * "leave the command", and a pop hidden inside `goBack` would turn
         * each into "go back one screen and stay in a command whose worker is
         * already gone". Escape is the one caller that means back.
         *
         * Nothing is done here beyond asking: the worker renders the screen
         * underneath and says so, and the `navigation` event is what resets
         * the highlight and the field. Guessing at it from this side would be
         * a second opinion about a stack this window does not own.
         */
        if (mode === "command" && session && nav.depth > 1 && nav.pop) {
          void activateHandler(session, nav.pop);
          panelOpen = false;
          break;
        }

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
    let updating: UnlistenFn | undefined;
    let shown: UnlistenFn | undefined;
    let switcher: UnlistenFn | undefined;
    let selecting: UnlistenFn | undefined;
    let indexed: UnlistenFn | undefined;
    let welcomed: UnlistenFn | undefined;
    let changed: UnlistenFn | undefined;
    let ran: UnlistenFn | undefined;
    let hidden: (() => void) | undefined;
    let chatHeard: UnlistenFn | undefined;
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
      /*
       * A newer Sill, whenever Rust finds one or gets further along.
       *
       * Read once on mount as well as listened for, because the settings
       * window may have found it while this page was closed, and a chin that
       * only learns from events would sit empty until the next check a day
       * later.
       */
      updateProgress = (await updateState("launcher")).progress;
      updating = await whenUpdateChanges((progress) => {
        updateProgress = progress;
      });

      unlisten = await listen<UiEvent>("sill://ui", ({ payload }) => {
        // A late message from a command the user already left would otherwise
        // redraw a view that is no longer on screen.
        if ("session" in payload && session && payload.session !== session) return;

        switch (payload.kind) {
          case "render":
            tree.apply(payload.ops);
            version++;
            status = "";

            // The first render of this command, and only the first: an
            // extension re-renders on every keystroke it handles, and those
            // are not somebody waiting for it to open.
            if (launchedAt?.session === payload.session) {
              stopClock(launchedAt.at, "extensionFirstRender");
              launchedAt = null;
            }
            break;
          case "showToast":
          case "updateToast":
            toast = {
              title: payload.title,
              style: payload.style,
              actions: toastActions(payload.actions ?? []),
            };
            break;
          case "hideToast":
            toast = null;
            break;
          case "showHud":
            // A HUD is a line and never a button. Raycast's has no actions and
            // an empty list here is what says so, rather than the last toast's
            // buttons surviving under a new message.
            toast = { title: payload.text, style: "success", actions: [] };
            break;
          case "setSearchText":
            query = payload.text;
            // Adopted rather than relayed. The extension wrote this, and
            // handing it straight back through `onSearchTextChange` would be
            // Sill reporting the extension's own words to it as news, which
            // for an extension that sets the text from that handler is a loop.
            relay.adopt(payload.text);
            break;
          case "navigation":
            /*
             * A different screen, so everything about the last one is dropped.
             *
             * The same three things a command switch drops, for the same
             * reasons: the highlight belongs to rows that are gone, the field
             * was typed at a screen nobody is looking at any more, and a
             * throttled call still waiting carries a handler id the worker
             * released on the commit that swapped the screens. Left armed it
             * fires into nothing and puts "the command could not search" on a
             * screen that never asked to search.
             */
            nav = { depth: payload.depth, pop: payload.pop };
            selected = 0;
            query = "";
            relay.cancel();
            panelOpen = false;
            status = "";
            break;
          case "popToRoot":
          case "closeMainWindow":
            // Out of the command entirely, however deep it was. `popToRoot`
            // is the extension asking for the root search, not for one step
            // back, and `goBack` steps back while there is a stack to step in.
            nav = NOT_NAVIGATED;
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
        // What an empty list means changes when this lands, so it is re-asked
        // here rather than left at whatever it was on mount.
        void indexBuilding().then((yes) => (building = yes));
        if (mode === "root") void refreshRoot();
      });

      // Rust puts the launcher up on a first run and says what it opened it
      // for. Not the only way this arrives: see `offerWelcome`.
      welcomed = await listen("sill://welcome", () => {
        void offerWelcome();
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

      /*
       * What the window timed, handed to Rust on the way out.
       *
       * Here rather than as each reading is taken, because a keystroke is
       * allowed one search and one delayed page and a third call would be the
       * measuring making the thing worse. By the time the launcher is being
       * put away nobody is waiting, and this is one call carrying everything
       * that visit measured.
       *
       * Failing silently on purpose. A diagnostic that puts an error on
       * screen while somebody is dismissing the launcher has cost them more
       * than it will ever tell anybody.
       */
      hidden = whenHidden(() => {
        /*
         * The keystroke clock stops when the window goes away.
         *
         * Without this a keystroke typed just before the launcher was put
         * away keeps its start time, and the first paint after the **next**
         * summon is charged the whole gap in between. It read 51 seconds for
         * one keystroke in this log, beside a median of 74 ms, which is not a
         * slow keystroke: it is a keystroke, a dismissal, and a summon a
         * minute later, added together.
         *
         * That number is published. `P8-06` puts the worst reading on a page
         * whose whole argument is that a reader can check it, so a figure
         * that is really three things is worse there than anywhere else.
         */
        typedAt = null;

        for (const { what, tookUs } of drawn.flush()) {
          void reportPainted(what, tookUs).catch(() => {});
        }
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

        /*
         * Whether there is a newer Sill.
         *
         * On the summon because that is a moment somebody created, which is
         * the question the constitution asks of anything that wakes up. Rust
         * throttles it to once a day, so calling this on every single summon
         * opens a socket about once a day and does nothing the rest of the
         * time. There is no timer anywhere in this feature.
         */
        void checkForUpdate();

        // Re-asked on every summon. A file indexer can be started or stopped
        // between two uses of the launcher, and the alternative to asking here
        // is asking on every keystroke.
        void fileSearchMissing().then((why) => (fileSearchGap = why));

        /*
         * The welcome, asked again if one is on screen.
         *
         * Its rows send somebody to Settings to change the very things the
         * paragraphs above them describe, and this is the moment they come
         * back. Without it the page goes on saying the summon key is taken
         * after they have chosen one that is not, which is the same kind of
         * stale claim the whole item is about.
         */
        if (mode === "welcome") {
          void askWelcome(true).then((said) => {
            if (said) greeting = said;
          });
        }

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
       * Everything about the turn in flight, heard into `aiLive`.
       *
       * The words, the thinking, each tool reached for and whether it
       * managed, and the card that pauses the turn: all of it lands in the
       * one object the panel draws from. What is left to do here is the end
       * of a turn, which is where the launcher's own list changes.
       */
      chatHeard = await listenToChat(aiLive, {
        done(turn) {
          if (turn) conversation = [...conversation, turn];
        },
        failed(turn) {
          // Whatever arrived before it failed is kept: half an answer is
          // often enough to see what went wrong. The panel says the rest.
          if (turn) conversation = [...conversation, turn];
        },
      });

      switcher = await listen("sill://switcher", () => {
        void openSwitcher();
      });

      /*
       * A universal key was pressed and Rust has read what was selected.
       *
       * Rust has already done the half that had to happen while something else
       * was in front: asked Explorer what is highlighted, or pressed Ctrl+C in
       * whatever had the keyboard. What arrives is a list of objects, and all
       * that is left is to draw them and open the panel.
       */
      selecting = await listen<SelectedObject[]>("sill://act-on", ({ payload }) => {
        openSelection(payload);
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
      building = await indexBuilding();
      await refreshRoot();
      searchInput?.focus();

      // Last, because it is the only thing here that changes what is on
      // screen, and because on all but one start in the life of a machine it
      // does nothing at all.
      await offerWelcome();
    })();

    return () => {
      disposed = true;
      // A pending file query has nowhere to land once this is torn down.
      clearTimeout(fileTimer);
      // And a measurement has nobody to show it to.
      stopTicking();
      unlisten?.();
      updating?.();
      shown?.();
      hidden?.();
      finishedScript?.();
      switcher?.();
      selecting?.();
      indexed?.();
      welcomed?.();
      changed?.();
      ran?.();
      chatHeard?.();
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<main>
  <SearchRow
    {mode}
    bind:query
    bind:field={searchInput}
    ontyped={() => (typedAt = performance.now())}
    {selected}
    {browsing}
    awaitingTitle={awaiting?.title}
    outputTitle={output?.title}
    movingTitle={moving?.title}
    namingTitle={naming?.title}
    runningTitle={running?.extensionTitle}
    {answersWith}
    asking={aiLive.asking}
    conversationEmpty={conversation.length === 0}
    commandPlaceholder={String(view?.props.searchBarPlaceholder ?? "Search…")}
    {dropdown}
    onpick={(value) => {
      if (!session || !dropdown?.onChange) return;
      remember(pickerKey(dropdown), dropdown.storeValue, value);
      void activateHandler(session, dropdown.onChange, [value]).catch((err: unknown) => {
        status = `the command could not change that: ${err}`;
      });
    }}
  />

  {#if mode === "output" && output}
    <ScriptOutput {output} />
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
    <AiChat {conversation} live={aiLive} {answersWith} ondecide={decide} onoffer={offer} />

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

  {:else if mode === "keys"}
    <!-- A page to read, not a list to walk. Every chord on it is assembled in
         Rust from the keys that actually run, so it cannot promise one that
         does not work. -->
    <div class="listing">
      <KeySheet />
    </div>

  {:else if mode === "welcome" && greeting}
    <!-- The first summon on a machine Sill has not run on. Every sentence
         comes from Rust, built from what registering the summon key actually
         answered rather than from the key the settings file asks for. -->
    <div class="listing">
      <Welcome
        said={greeting}
        {selected}
        onselect={(i) => (selected = i)}
        onrun={(i) => {
          selected = i;
          void openSelected();
        }}
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
        {building}
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
        <SwitcherPreview
          bind:this={switcherPreview}
          entrypoint={commands[selected]?.entrypoint}
        />
      {:else if showsFilePreview}
        <!--
          A look inside the file under the cursor.

          Drawn whenever the list holds files at all, rather than only when a
          file is selected. The strip appearing and disappearing as somebody
          arrows between a file and a command would move the row they are
          reading, and within one query this way it does not move at all.
        -->
        <FilePreview path={fileUnderCursor} />
      {/if}
    </div>
  {:else if view?.tag === "List"}
    <ListView
      {tree}
      node={view}
      {version}
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
      {tree}
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
    <FormView
      bind:this={formView}
      {tree}
      node={view}
      {version}
      session={session ?? ""}
      onsubmit={submitForm}
      stored={storedByField}
      onremember={(id, value) => remember(id, true, value)}
    />
  {:else if view?.tag === "Detail"}
    <!--
      Prose, drawn as prose.

      This was a `div` holding the markdown source, so an extension that wrote
      a heading put a hash on screen and one that wrote a table put pipes on
      it. `DetailPane` runs it through the same parser the AI answers use and
      draws the metadata panel beside it.
    -->
    <DetailPane {tree} node={view} {version} />
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

  <Footer
    {mode}
    {toast}
    {status}
    prefs={prefs ?? null}
    viewTag={view?.tag}
    hasActions={actions.length > 0}
    onbuiltin={(id) => {
      const found = commands.find((c) => c.id === `sill:${id}`);
      if (found) void launchCommand(found.id);
    }}
    onrun={() => void openSelected()}
    onactions={() => {
      panelOpen = !panelOpen;
      panelSelected = 0;
    }}
    ontoastaction={(action) => void runExtensionAction(action)}
    update={chinLine(updateProgress)}
    onupdate={() => void startUpdate()}
  />
  <!--
    An extension's permission card, in front of whatever mode is up. The AI
    conversation draws its own, so this stands down there. Inside `main`
    rather than beside it, so the window's rounded corners clip its scrim.
  -->
  <PermissionAsk hidden={mode === "ai"} />
</main>

<style>
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

  main {
    /* The containing block for anything laid over the whole window. */
    position: relative;
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

</style>
