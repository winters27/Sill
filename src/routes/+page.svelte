<script lang="ts">
  import { onMount } from "svelte";
  import { openQuicklink } from "$lib/quicklinks";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import ListView from "$lib/components/ListView.svelte";
  import GridView from "$lib/components/GridView.svelte";
  import FormView from "$lib/components/FormView.svelte";
  import RootList from "$lib/components/RootList.svelte";
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
    searchCommands,
    unloadExtension,
    performBuiltin,
    searchFiles,
    searchWindows,
    searchEmoji,
    fileSearchMissing,
    startFileSearch,
    type FileSearchMissing,
    recordUse,
    queryHistory,
    openPath,
    fileAsCommand,
    actionsFor,
    // `runAction` here already means "run the panel entry at this index".
    runAction as runObjectAction,
    asTarget,
    undoAction,
    type ActionInfo,
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
  >("root");
  /**
   * The quicklink waiting for something to be typed into it.
   *
   * A quicklink with `{query}` in it is a search, and a search with no words
   * is not worth opening. So selecting one does not open it: it takes over
   * the field, and the next thing typed is what goes in the hole.
   */
  let awaiting = $state<{ id: string; title: string; link: string } | null>(null);
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
  let panelSelected = $state(0);
  let prefs = $state<Preferences | null>(null);
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

  const count = $derived.by(() => {
    if (mode === "root" || mode === "switcher" || mode === "emoji") {
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
    if (mode === "root") {
      const chosen = commands[selected];

      // Naming a result is offered on the result, not buried in settings.
      // An alias nobody can reach is one nobody sets, and the launcher is
      // where you are when you notice you want one.
      const naming =
        chosen && chosen.mode !== "answer" && chosen.mode !== "window"
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
    const command = mode === "root" ? commands[selected] : undefined;
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

  async function refreshRoot() {
    const current = query;
    const id = ++searchId;

    // Typing leaves the history behind. Anything else would make the next Up
    // continue walking from a place the user has already edited away from.
    if (!current) walked = -1;

    clearTimeout(fileTimer);

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

    // Files are appended after the commands, so a slower file query can never
    // reorder or delay what is already shown.
    fileTimer = setTimeout(async () => {
      try {
        const hits = await searchFiles(current);
        if (id !== searchId) return;

        commands = [...commands, ...hits.map(fileAsCommand)];
      } catch (err) {
        if (id === searchId) status = `file search failed: ${err}`;
      }
    }, FILE_SEARCH_DEBOUNCE_MS);
  }

  async function openSelected() {
    if (mode === "clipboard") {
      await clipboardView?.paste(true);
      return;
    }

    if (mode === "argument") {
      const link = awaiting;
      if (!link) return;
      try {
        status = `opening ${link.title}`;
        await openQuicklink(link.id, query);
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
      if (command.id === "sill:emoji") {
        void recordUse(command.id, query);
        mode = "emoji";
        selected = 0;
        query = "";
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
          id: command.entrypoint,
          title: command.title,
          link: command.subtitle,
        };
        mode = "argument";
        selected = 0;
        query = "";
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

        // A file is opened by the shell and has no session of its own.
        if (command.mode === "file") {
          await openPath(command.entrypoint);
          await dismiss();
          return;
        }

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
   * Puts a second search's results into the first, by how well each matched.
   *
   * Appending was wrong and measuring showed how wrong: typing "tada" matched
   * eighty-four things in the index, every one of them a coincidence of
   * spelling, and the party popper somebody had plainly named landed
   * eighty-fifth. Groups are ordered by their best member, so where the first
   * one lands decides where the whole group reads.
   *
   * So: above everything the index only half-recognised, below everything it
   * knew by name. Neither list is reordered within itself.
   */
  function merged(into: RankedCommand[], extra: RankedCommand[]): RankedCommand[] {
    let at = 0;
    while (at < into.length && into[at].strong) at += 1;

    return [...into.slice(0, at), ...extra, ...into.slice(at)];
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

    const action = actions[index];
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
    if (mode === "root") {
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

      try {
        const outcome = await runObjectAction(chosen, asTarget(command));
        status = outcome.message;
        lastUndo = outcome.undo ?? null;
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
  async function openSwitcher() {
    panelOpen = false;
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

    if (mode === "emoji") {
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

    // Ctrl+K is the action menu, matching Raycast's Cmd+K.
    if (event.key.toLowerCase() === "k" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      if (actions.length) {
        panelOpen = !panelOpen;
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
      if (event.key === "ArrowDown") {
        event.preventDefault();
        panelSelected = (panelSelected + 1) % actions.length;
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        panelSelected = (panelSelected - 1 + actions.length) % actions.length;
      } else if (event.key === "Enter") {
        event.preventDefault();
        void runAction(panelSelected);
      } else if (event.key === "Escape") {
        // Closes the panel only; the launcher stays put.
        event.preventDefault();
        panelOpen = false;
      }
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
    if (mode === "root" || mode === "switcher" || mode === "emoji") {
      void refreshRoot();
    }
  });

  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    let shown: UnlistenFn | undefined;
    let switcher: UnlistenFn | undefined;
    let indexed: UnlistenFn | undefined;
    let changed: UnlistenFn | undefined;
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
        });
      });

      // The switcher key, which shows the launcher already on the window
      // list. A separate event rather than a flag read on show, because the
      // page has to react to being opened this way even when it was already
      // sitting on something else.
      switcher = await listen("sill://switcher", () => {
        void openSwitcher();
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
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<main>
  <div class="search">
    <img class="mark" src="/sill.png" alt="" width="24" height="24" draggable="false" />
    {#if mode === "argument" && awaiting}
      <span class="crumb">{awaiting.title}</span>
    {:else if mode === "clipboard"}
      <span class="crumb">Clipboard History</span>
    {:else if mode === "switcher"}
      <span class="crumb">Open Windows</span>
    {:else if mode === "emoji"}
      <span class="crumb">Emoji</span>
    {:else if mode === "collection"}
      <span class="crumb">Collection</span>
    {:else if mode === "alias" && naming}
      <span class="crumb">{naming.title}</span>
    {:else if running}
      <span class="crumb">{running.extensionTitle}</span>
    {/if}
    <input
      bind:this={searchInput}
      bind:value={query}
      placeholder={mode === "argument"
        ? "Type what to search for, then Enter…"
        : mode === "emoji"
          ? "Search emoji by name…"
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
  {:else if mode === "root" || mode === "switcher" || mode === "alias" || mode === "emoji"}
    <!-- Kept on screen while a name is typed, so what is being named stays
         visible. -->
    <RootList
      bind:this={rootList}
      {commands}
      {selected}
      asking={mode + " " + query}
      onselect={(i) => (selected = i)}
      onrun={(i) => {
        selected = i;
        void openSelected();
      }}
    />
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
    <div class="status">{status || "loading…"}</div>
  {/if}

  {#if panelOpen}
    <ActionPanel
      {actions}
      selected={panelSelected}
      onselect={(i) => (panelSelected = i)}
      onrun={(i) => void runAction(i)}
    />
  {/if}

  <div class="divider"></div>

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
    <span class="hint">
      {mode === "clipboard" ? "Paste" : mode === "root" ? "Open" : view?.tag === "Form" ? "Submit" : "Run"}
      <span class="keys">↵</span>
    </span>
    {#if actions.length}
      <span class="hint">Actions <span class="keys">Ctrl K</span></span>
    {/if}
    <span class="hint">
      {mode === "root" ? "Close" : "Back"}
      <span class="keys">Esc</span>
    </span>
  </footer>
</main>

<style>
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
    background-image: linear-gradient(var(--tint), var(--tint));
    border-radius: var(--radius-window);
    /* No border: DWM already clips the window to this radius, so a border here
       only stacks onto that edge. The single light inset is the glass catch. */
    box-shadow: var(--bevel-window);
    overflow: hidden;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-left: var(--pad);
    flex: none;
  }

  /* The mark stands where a magnifier would, so the window is identifiable
     the moment it appears rather than only from its contents.

     The app icon itself, at the size it is drawn everywhere else. There is
     no separate in-app mark any more: the art lost its plaque, so the thing
     on the taskbar is already the right thing to put here. */
  .mark {
    flex: none;
    width: 24px;
    height: 24px;
    -webkit-user-drag: none;
  }

  .crumb {
    flex: none;
    font-size: var(--text-row);
    padding: 5px 10px;
    border-radius: var(--radius-sm);
    background-image: var(--sheen);
    box-shadow: var(--bevel-tile);
    color: var(--text-muted);
    font-size: 12px;
    white-space: nowrap;
  }

  .search input {
    flex: 1;
    min-width: 0;
    padding: 14px var(--pad) 14px 0;
    border: 0;
    background: transparent;
    color: var(--core-foreground);
    /* Segoe has a separate cut for text this size; Inter resolves this back
       to itself. */
    font-family: var(--font-display);
    font-size: var(--text-query);
    font-weight: 400;
    /* Large text wants a touch of negative tracking; at 17px Inter's default
       spacing reads loose next to a 13px list. */
    letter-spacing: -0.01em;
    outline: none;
    user-select: text;
  }

  .search input::placeholder {
    color: var(--text-faint);
  }

  .divider {
    flex: none;
    height: 1px;
    background: var(--hairline);
  }

  .argument-hint {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 18px var(--pad);
  }

  .going {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--text-muted);
    word-break: break-all;
  }

  .explains {
    margin: 0;
    max-width: 62ch;
    font-size: var(--text-meta);
    line-height: 1.6;
    color: var(--text-faint);
  }

  .detail {
    flex: 1;
    overflow-y: auto;
    padding: 16px var(--pad);
    color: var(--core-foreground);
    line-height: 1.6;
    white-space: pre-wrap;
    user-select: text;
  }

  .status {
    flex: 1;
    display: grid;
    place-items: center;
    color: var(--text-faint);
  }

  footer {
    display: flex;
    align-items: center;
    gap: 14px;
    flex: none;
    height: 32px;
    padding: 0 var(--pad);
    font-size: var(--text-meta);
    color: var(--text-faint);
  }

  .hint {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-faint);
  }

  /* Plain type, not a keycap. A footer is the quietest row in the window and
     six lit keycaps made it the loudest. */
  /* The same face as the label, one step down and quieter. A monospace key
     name beside a proportional label puts two typefaces on the quietest row
     in the window. */
  .keys {
    font-size: var(--text-meta);
    font-weight: 500;
    color: var(--text-muted);
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
