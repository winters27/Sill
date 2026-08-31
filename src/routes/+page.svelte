<script lang="ts">
  import { onMount } from "svelte";
  import { beginCapture, captureScreen, lastImage, openMarkup } from "$lib/capture";
  import { openQuicklink } from "$lib/quicklinks";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import ListView from "$lib/components/ListView.svelte";
  import GridView from "$lib/components/GridView.svelte";
  import FormView from "$lib/components/FormView.svelte";
  import RootList from "$lib/components/RootList.svelte";
  import { LISTBOX, isBrowsing, isListMode, merged, optionId } from "$lib/results";
  import ActionPanel from "$lib/components/ActionPanel.svelte";
  import LauncherMenu from "$lib/components/LauncherMenu.svelte";
  import ClipboardView from "$lib/components/ClipboardView.svelte";
  import { collectActions, isRunnable } from "$lib/exthost/actions";
  import { clipboardMerge } from "$lib/clipboard";
  import {
    chordFrom,
    navigationChords,
    setAlias,
    type Move as MoveKey,
  } from "$lib/settings";
  import {
    activateHandler,
    dismiss,
    launchCommand,
    movePath,
    aiAsk,
    aiClear,
    aiReady,
    aiTranscript,
    forgetPreviews,
    searchAppVolume,
    searchDestinations,
    summonPainted,
    windowPreview,
    systemStates,
    searchCommands,
    unloadExtension,
    performBuiltin,
    searchBrowsers,
    searchFiles,
    searchWindows,
    searchEmoji,
    fileSearchMissing,
    startFileSearch,
    type FileSearchMissing,
    recordUse,
    renamePath,
    queryHistory,
    openPath,
    browserAsCommand,
    defaultBrowser,
    extractTextFromLastImage,
    webSearchRow,
    fileAsCommand,
    actionsFor,
    // `runAction` here already means "run the panel entry at this index".
    runAction as runObjectAction,
    asTarget,
    undoAction,
    type ActionInfo,
    type AiTurn,
    type RankedCommand,
    type UndoToken,
  } from "$lib/exthost/commands";
  import { ViewTree, isHandlerRef, type ElementNode, type Op } from "$lib/exthost/tree";
  import { applyAppearance, getPreferences, openSettings, type Preferences } from "$lib/settings";
  import "$lib/theme/theme.css";

  type UiEvent =
    | { kind: "render"; session: string; ops: Op[] }
    | { kind: "showToast"; session: string; id: string; title: string; message: string; style: string }
    | { kind: "updateToast"; session: string; id: string; title: string; message: string; style: string }
    | { kind: "hideToast"; session: string; id: string }
    | { kind: "showHud"; session: string; text: string }
    | { kind: "setSearchText"; session: string; text: string }
    | { kind: "popToRoot"; session: string }
    | { kind: "closeMainWindow"; session: string }
    | { kind: "crashed"; session: string; reason: string };

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
     * A conversation with a model.
     *
     * Its own mode rather than a row in the list, because an answer is a
     * paragraph and a list is a list. Reached by Tab from the root, which is
     * where the question was already being typed.
     */
    | "ai"
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
    what: "quicklink" | "rename";
    id: string;
    title: string;
    link: string;
  } | null>(null);

  /**
   * What is being moved, while somewhere is being picked for it.
   *
   * Not part of `awaiting`, which borrows the field to collect a line of text.
   * This borrows the whole list instead: the answer is one of the rows rather
   * than whatever was typed, and typing only narrows them.
   */
  let moving = $state<{ path: string; title: string } | null>(null);
  let session = $state<string | null>(null);
  let running = $state<{ title: string; extensionTitle: string } | null>(null);

  let query = $state("");
  let selected = $state(0);
  let commands = $state<RankedCommand[]>([]);
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

  const view = $derived.by(() => {
    version;
    return tree.top();
  });

  /** Selectable items in a running command's List, flattened as ListView does. */
  const items = $derived.by((): ElementNode[] => {
    version;
    const node = tree.top();
    if (!node) return [];

    // List and Grid differ only in how they are drawn; selection walks the
    // same flattened item sequence in both.
    const itemTag = node.tag === "Grid" ? "Grid.Item" : "List.Item";
    const sectionTag = node.tag === "Grid" ? "Grid.Section" : "List.Section";
    if (node.tag !== "List" && node.tag !== "Grid") return [];

    const out: ElementNode[] = [];
    for (const child of tree.elementChildren(node)) {
      if (child.tag === sectionTag) {
        out.push(...tree.elementChildren(child).filter((c) => c.tag === itemTag));
      } else if (child.tag === itemTag) {
        out.push(child);
      }
    }
    return out;
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
   * Whether the field is filtering a list that is on screen and walkable.
   *
   * Only then is it a combobox. The same field is a plain one everywhere else:
   * in the modes that show something other than the root list there is no
   * listbox to point at, and naming a row that is not rendered leaves a screen
   * reader announcing nothing at all. Naming a collection or an alias is
   * typing a name rather than filtering, so there is nothing to arrow through
   * either.
   */
  const browsing = $derived(isBrowsing(mode, commands.length));

  const count = $derived.by(() => {
    if (isListMode(mode)) {
      return commands.length;
    }
    // The field is a name, not a filter, so there is nothing to arrow through.
    if (mode === "collection" || mode === "alias") return 0;
    if (mode === "clipboard") return clipboardCount;
    return items.length;
  });

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
          shortcut: { modifiers: [], key: "delete" },
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
          shortcut: undefined,
        })),
      ] as typeof extensionActions;
    }

    // Whatever Rust says can be done to the selected result. This used to be
    // two entries written here by hand, which meant the panel and the Enter
    // key were two separate opinions about what a result supports.
    if (mode === "root" || mode === "appVolume") {
      const chosen = commands[selected];

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
       * copy of that program.
       */
      const namable =
        chosen &&
        chosen.mode !== "answer" &&
        chosen.mode !== "window" &&
        chosen.mode !== "audio-session";

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
        shortcut: action.primary
          ? { modifiers: [], key: "enter" }
          : undefined,
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

  let rootActions = $state<ActionInfo[]>([]);

  /**
   * What can be done to a clipboard row.
   *
   * Fetched once rather than per selection: every row in the history is the
   * same kind of thing, so the answer cannot differ between them.
   */
  let clipboardActions = $state<ActionInfo[]>([]);

  /**
   * How to reverse the last action, when it said it could be.
   *
   * One deep on purpose. A launcher is not a document editor, and an undo
   * stack that goes back through a morning of copies would mostly be a way to
   * put back something nobody wanted.
   */
  let lastUndo = $state<UndoToken | null>(null);

  $effect(() => {
    const command =
      mode === "root" || mode === "appVolume" ? commands[selected] : undefined;
    if (!command) {
      rootActions = [];
      return;
    }

    // Keyed on the kind, so arrowing through a list of applications asks once
    // rather than once per row.
    const wanted = command.mode;
    void actionsFor(wanted).then((list) => {
      // The selection moved to a different kind while this was in flight.
      if (commands[selected]?.mode === wanted) rootActions = list;
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

        commands = found;
        if (selected >= commands.length) selected = 0;
      } catch (err) {
        if (id === searchId) status = `could not look for folders: ${err}`;
      }
      return;
    }

    if (mode === "appVolume") {
      try {
        const found = await searchAppVolume(current);
        if (id !== searchId) return;

        commands = found;
        if (selected >= commands.length) selected = 0;
      } catch (err) {
        if (id === searchId) status = `could not read the volumes: ${err}`;
      }
      return;
    }

    if (mode === "emoji") {
      try {
        const found = await searchEmoji(current);
        if (id !== searchId) return;

        commands = found;
        if (selected >= commands.length) selected = 0;
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

        commands = open;
        if (selected >= commands.length) selected = 0;
      } catch (err) {
        if (id === searchId) status = `window search failed: ${err}`;
      }
      return;
    }

    try {
      const ranked = await searchCommands(current);
      if (id !== searchId) return;

      commands = ranked;
      if (selected >= commands.length) selected = 0;
    } catch (err) {
      if (id === searchId) status = `search failed: ${err}`;
      return;
    }

    if (!current.trim()) return;

    // Open windows, above files and below the index. Not debounced: this is
    // a Win32 enumeration and a rank in Rust, on the same order as the command
    // search rather than the file one.
    try {
      const open = await searchWindows(current);
      if (id !== searchId) return;
      if (open.length) commands = [...commands, ...open];
    } catch (err) {
      if (id === searchId) status = `window search failed: ${err}`;
    }

    // Emoji, in their own group. A separate corpus rather than part of the
    // index: two thousand entries would swamp fifteen hundred real ones, and
    // ranking them together would mean every keystroke weighed both.
    //
    // Only strong matches come back, which Rust decides. Their names are
    // ordinary words, so loose matching would put a smiley in the middle of
    // every search anybody ever typed.
    try {
      const faces = await searchEmoji(current, true);
      if (id !== searchId) return;
      if (faces.length) commands = merged(commands, faces);
    } catch (err) {
      if (id === searchId) status = `emoji search failed: ${err}`;
    }

    // Nothing will come back, and saying so beats an empty space where files
    // should be. One row, only once something has been typed, and only when
    // file search is switched on: somebody who turned it off does not need
    // telling that it is off.
    if (fileSearchGap) {
      commands = [...commands, fileSearchRow(fileSearchGap)];
    }

    // Files and browser pages are appended after the commands, so a slower
    // query against either can never reorder or delay what is already shown.
    //
    // One timer for both. They are the two sources that read somebody else's
    // files rather than Sill's index, they are the two that are worth waiting
    // a moment before asking, and giving them separate timers would only mean
    // two chances to fire on a query that has already been replaced.
    fileTimer = setTimeout(async () => {
      try {
        const hits = await searchFiles(current);
        if (id !== searchId) return;

        commands = [...commands, ...hits.map(fileAsCommand)];
      } catch (err) {
        if (id === searchId) status = `file search failed: ${err}`;
      }

      try {
        const pages = await searchBrowsers(current);
        if (id !== searchId) return;

        commands = [...commands, ...pages.map(browserAsCommand)];
      } catch (err) {
        if (id === searchId) status = `browser search failed: ${err}`;
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
      if (webSearchEnabled && id === searchId) {
        commands = [...commands, webSearchRow(current.trim(), browser ?? undefined)];
      }
    }, FILE_SEARCH_DEBOUNCE_MS);
  }

  async function openSelected() {
    if (mode === "clipboard") {
      await clipboardView?.paste(true);
      return;
    }

    if (mode === "argument") {
      const asked = awaiting;
      if (!asked) return;

      if (asked.what === "rename") {
        try {
          status = await renamePath(asked.id, query);
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
        const outcome = await movePath(source.path, folder.entrypoint);
        status = outcome.message;
        lastUndo = outcome.undo ?? null;

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

      // Its own view rather than a window: the history is browsed the same
      // way the root list is, with the same field and the same keys.
      // Its own corpus behind its own command, the same shape the clipboard
      // history uses and for the same reason.
      if (command.id === "sill:appVolume") {
        void recordUse(command.id, query);
        mode = "appVolume";
        selected = 0;
        query = "";
        return;
      }

      if (command.id === "sill:emoji") {
        void recordUse(command.id, query);
        mode = "emoji";
        selected = 0;
        query = "";
        return;
      }

      /*
       * Reads the last picture copied, without opening anything.
       *
       * A row rather than only an action buried in the clipboard's panel,
       * because a capability nobody can find is a capability nobody has. This
       * one is reached by typing "ocr", "read text" or "screenshot", and the
       * key bound to it goes through the same action against the same picture.
       */
      // Picking an area puts an overlay over every screen, so the launcher
      // has nothing more to do than get out of the way, which Rust does.
      if (command.id === "sill:capture-area") {
        void recordUse(command.id, query);
        try {
          await beginCapture();
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      if (command.id === "sill:capture-screen") {
        void recordUse(command.id, query);
        try {
          status = await captureScreen();
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      // Marking up opens a window of its own on the last picture copied. It
      // goes through the action registry, so the row and the clipboard's own
      // panel entry are one implementation.
      if (command.id === "sill:mark-up") {
        void recordUse(command.id, query);
        try {
          const image = await lastImage();
          if (image === null) {
            status = "nothing has been copied as a picture yet";
            return;
          }
          await openMarkup(image);
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      if (command.id === "sill:extract-text") {
        void recordUse(command.id, query);
        try {
          status = await extractTextFromLastImage();
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      if (command.id === "sill:clipboard") {
        // Opened here rather than launched, but it is still a use, and
        // ranking has to see it or the history can never rise in the root
        // list however often it is reached for.
        void recordUse(command.id, query);
        mode = "clipboard";
        selected = 0;
        query = "";
        return;
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

        // Launching an app hands the screen to that app, so the launcher
        // gets out of the way rather than sitting on top of what it opened.
        // Sill's own commands open their own window, so the launcher steps
        // aside the same way it does for anything else it hands off to.
        if (
          launched.mode === "app" ||
          launched.mode === "exe" ||
          launched.mode === "setting" ||
          launched.mode === "builtin"
        ) {
          await dismiss();
          return;
        }

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

        tree.reset();
        version++;
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

  /** How far a screenful moves. Matches the rows the window shows at once. */
  const PAGE = 8;

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

      lastUndo = outcome.undo ?? null;
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
        subtitle: "Sill is not indexing any folders and nothing else on this machine is either. Choose this to set it up.",
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

        try {
          const outcome = await runObjectAction(action.tag.slice("Sill.Action:".length), {
            id: String(entry.id),
            mode: "clipboard",
            target: entry.text,
            // The row's own text, trimmed to something a status line can hold.
            title: entry.text.slice(0, 40),
          });
          status = outcome.message;
          lastUndo = outcome.undo ?? null;
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
    if (mode === "root" || mode === "appVolume") {
      const chosen = action.tag.startsWith("Sill.Action:")
        ? action.tag.slice("Sill.Action:".length)
        : "";
      const command = commands[selected];
      if (!chosen || !command) return;

      // The primary action goes through openSelected, which knows the two
      // things the registry does not: a quicklink with a hole in it takes
      // over the field, and an extension command switches the whole view.
      if (rootActions.find((a) => a.id === chosen)?.primary) {
        await openSelected();
        return;
      }

      /*
       * Renaming needs a name, and the asking is the feature.
       *
       * Kept here rather than in the action, which is why the action itself
       * refuses to run: an action is handed an object and acts, and there is
       * nowhere in that for a question. The field is borrowed exactly as a
       * quicklink with a hole in it borrows it.
       */
      /*
       * Moving needs somewhere to move to, and the picking is the feature.
       *
       * The whole list is borrowed rather than the field, because the answer
       * is a folder and typing only narrows which one. Same reason the action
       * itself refuses to run: it is handed one object, and there is nowhere
       * in that for a question with a list of answers.
       */
      if (chosen === "sill.file.move") {
        moving = { path: command.entrypoint, title: command.title };
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
        lastUndo = outcome.undo ?? null;

        /*
         * The panel reaches the same things Enter does, so pressing one here
         * has to leave the rows saying the same thing.
         *
         * A whole re-read for the volume list, because these rows carry a
         * percentage as well as a switch and `refreshSwitches` only puts the
         * switch back: turning something down would have flipped nothing and
         * left "100%" underneath. Elsewhere the switch is the whole state, and
         * only when the row acted on was one, because copying a path moves
         * nothing and a one-shot is closing the window anyway.
         */
        if (mode === "appVolume") {
          await refreshRoot();
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
        status = await performBuiltin(action.tag, action.props);
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
        // worth a message. The strip is simply empty.
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
  let conversation = $state<AiTurn[]>([]);

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

    mode = "ai";
    selected = 0;
    aiWhoNot = "";
    answering = "";
    asking = true;

    // Shown immediately, so the question is on screen before the answer
    // starts rather than after.
    conversation = [...conversation, { role: "user", text: question }];
    query = "";

    try {
      await aiAsk(question);
    } catch (err) {
      status = `${err}`;
    }
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
     * Escape goes back to the results, and the conversation is kept.
     *
     * Leaving is not finishing: somebody who looks something up mid-answer and
     * comes back should find it where it was. Rust holds it, so this only
     * stops drawing it.
     */
    if (mode === "ai") {
      mode = "root";
      selected = 0;
      query = "";
      await refreshRoot();
      return;
    }

    if (mode === "emoji" || mode === "appVolume" || mode === "destination") {
      // Whatever was being moved is no longer being moved.
      moving = null;
      mode = "root";
      selected = 0;
      query = "";
      await refreshRoot();
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
      await refreshRoot();

      // Unloaded after the UI has moved on, so tearing down a worker never
      // delays the frame the user is waiting for.
      if (previous) void unloadExtension(previous);
      return;
    }

    void dismiss();
  }

  function onKeydown(event: KeyboardEvent) {
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
    if (
      event.key === "Tab" &&
      !event.ctrlKey &&
      !event.altKey &&
      mode === "root" &&
      query.trim()
    ) {
      event.preventDefault();
      void askAi(query.trim());
      return;
    }

    // Ctrl+K is the action menu, matching Raycast's Cmd+K.
    if (event.key.toLowerCase() === "k" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      if (actions.length) {
        panelOpen = !panelOpen;
        // A fresh panel is an unfiltered one.
        panelFilter = "";
        panelSelected = 0;
      } else {
        // Never silent: if there is nothing to show, say so, otherwise a
        // working key press is indistinguishable from a dead one.
        status = "no actions here";
      }
      return;
    }

    // While the panel is open it owns the keyboard.
    if (panelOpen) {
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
      if (event.key === "Delete") {
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
        if (count) selected = Math.min(count - 1, selected + PAGE);
        break;
      case "pageUp":
        if (count) selected = Math.max(0, selected - PAGE);
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
          panelSelected = 0;
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
    // Not while the field holds a name rather than a query.
    if (isListMode(mode)) {
      void refreshRoot();
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
    let finished: UnlistenFn | undefined;
    let wentWrong: UnlistenFn | undefined;
    let disposed = false;

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
            break;
          case "popToRoot":
          case "closeMainWindow":
            void goBack();
            break;
          case "crashed": {
            // Read before goBack, which clears `running` synchronously on its
            // way to the root list, so asking afterwards gets null.
            const died = running?.title ?? "That command";
            // Back to the root list rather than staying on a view whose
            // extension is gone. Sitting there looks exactly like a slow
            // load, and nothing would ever arrive to correct the impression.
            void goBack();
            status = `${died} stopped: ${payload.reason}`;
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
      shown = await listen("sill://shown", () => {
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

      finished = await listen("sill://ai-done", () => {
        if (answering) {
          conversation = [...conversation, { role: "assistant", text: answering }];
        }
        answering = "";
        asking = false;
      });

      wentWrong = await listen<string>("sill://ai-failed", ({ payload }) => {
        // Whatever arrived before it failed is kept: half an answer is often
        // enough to see what went wrong.
        if (answering) {
          conversation = [...conversation, { role: "assistant", text: answering }];
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
      ran = await listen<string>("sill://run", ({ payload }) => {
        void launchCommand(payload).catch((err) => {
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
      });

      if (disposed) return;
      clipboardActions = await actionsFor("clipboard");
      prefs = await getPreferences();
      applyAppearance(prefs);
      browser = await defaultBrowser();
      navKeys = await navigationChords();
      past = await queryHistory();
      await refreshRoot();
      searchInput?.focus();
    })();

    return () => {
      disposed = true;
      // A pending file query has nowhere to land once this is torn down.
      clearTimeout(fileTimer);
      unlisten?.();
      shown?.();
      switcher?.();
      indexed?.();
      changed?.();
      ran?.();
      said?.();
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
    {:else if mode === "clipboard"}
      <span class="crumb">Clipboard History</span>
    {:else if mode === "switcher"}
      <span class="crumb">Open Windows</span>
    {:else if mode === "emoji"}
      <span class="crumb">Emoji</span>
    {:else if mode === "ai"}
      <span class="crumb">Ask</span>
    {:else if mode === "appVolume"}
      <span class="crumb">App Volume</span>
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
            : "Ask a follow-up…"
        : mode === "appVolume"
          ? "Filter by program name…"
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
  </div>

  <div class="divider"></div>

  {#if mode === "argument" && awaiting}
    <!-- The body would otherwise be a large empty rectangle. Showing the
         target answers the question the empty field raises, which is where
         this is about to send you. -->
    <div class="argument-hint">
      <p class="going">{awaiting.link}</p>
      <p class="explains">
        {query.trim()
          ? "Enter opens it with what you typed in place of the placeholder."
          : "Type the words to search for. They are escaped before they go into the address."}
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
      A conversation reads as a column of paragraphs, not as a list of rows.
      The field below stays the composer, so a follow-up is typed where the
      question was.
    -->
    <div class="chat" bind:this={chatScroll}>
      {#each conversation as turn, at (at)}
        <article class="turn" class:asked={turn.role === "user"}>
          <p>{turn.text}</p>
        </article>
      {/each}

      {#if answering}
        <article class="turn">
          <p>{answering}</p>
        </article>
      {:else if asking}
        <!-- Something between pressing Tab and the first token arriving,
             because a blank panel reads as nothing having happened. -->
        <p class="thinking">Thinking…</p>
      {/if}
    </div>

  <!--
    Not `isListMode`. This set is not the same one: `alias` draws the list too,
    with the field holding a name rather than a query, so the two lists differ
    by exactly that mode and sharing one would be wrong in one direction or the
    other. Written out, and this comment is why.
  -->
  {:else if mode === "root" || mode === "switcher" || mode === "alias" || mode === "emoji" || mode === "appVolume" || mode === "destination"}
    <!-- Kept on screen while a name is typed, so what is being named stays
         visible. -->
    <div class="listing">
      <RootList
        bind:this={rootList}
        {commands}
        {selected}
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
      {tree}
      node={view}
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
      {version}
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
    <div class="sill-empty">
      <img src="/sill.png" alt="" width="32" height="32" draggable="false" />
      <span class="headline">{status || "Starting the command"}</span>
    </div>
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
  /*
   * A conversation, which reads as a column of paragraphs rather than a list.
   *
   * It scrolls on its own so the field below stays put: a composer that moves
   * down the window as the answer grows is a composer you have to chase.
   */
  .chat {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4) var(--space-4);
  }

  .turn {
    max-width: 68ch;
  }

  .turn p {
    margin: 0;
    color: var(--text-1);
    font-size: var(--text-body);
    line-height: 1.6;
    /* An answer arrives as written, and models write in paragraphs. */
    white-space: pre-wrap;
    overflow-wrap: anywhere;
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

  .thinking {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-body);
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
    box-shadow: 0 0 0 1px var(--hairline);
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
    /* Chroma above the tint. `none` in every theme but Oilslick, and `none`
       is a valid layer, so there is no conditional here. */
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
    padding-left: var(--space-4);
    flex: none;
  }

  /* The mark stands where a magnifier would, so the window is identifiable
     the moment it appears rather than only from its contents.

     The app icon itself, at the size it is drawn everywhere else. There is
     no separate in-app mark any more: the art lost its plaque, so the thing
     on the taskbar is already the right thing to put here. */
  .mark {
    flex: none;
    width: 26px;
    height: 26px;
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
    height: 30px;
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
    transition: background-color 0.15s var(--ease), color 0.15s var(--ease);
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
    color: var(--accent-green);
  }
  .toast[data-style="failure"] {
    color: var(--accent-red);
  }
  .toast[data-style="animated"] {
    color: var(--accent-blue);
  }
</style>
