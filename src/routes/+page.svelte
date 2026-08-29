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
  import {
    activateHandler,
    dismiss,
    launchCommand,
    searchCommands,
    unloadExtension,
    performBuiltin,
    searchFiles,
    searchWindows,
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
  let mode = $state<"root" | "command" | "clipboard" | "argument">("root");
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
    if (mode === "root") return commands.length;
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
      return [
        {
          id: -10,
          title: "Paste",
          tag: "Sill.ClipboardPaste",
          props: {},
          shortcut: { modifiers: [], key: "enter" },
        },
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
      return rootActions.map((action, index) => ({
        id: -1 - index,
        title: action.title,
        tag: `Sill.Action:${action.id}`,
        props: {},
        shortcut: action.primary
          ? { modifiers: [], key: "enter" }
          : undefined,
      })) as typeof extensionActions;
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

    clearTimeout(fileTimer);

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

    if (mode === "root") {
      const command = commands[selected];
      if (!command) return;

      // Its own view rather than a window: the history is browsed the same
      // way the root list is, with the same field and the same keys.
      if (command.id === "sill:clipboard") {
        mode = "clipboard";
        selected = 0;
        query = "";
        return;
      }

      // A quicklink with a hole in it. Kept here rather than launched, so the
      // field can be handed over to filling the hole.
      if (command.mode === "quicklink-arg") {
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
        const launched = await launchCommand(command.id);

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
        case "Sill.ClipboardPin":
          await clipboardView?.togglePin();
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

    if (mode === "clipboard") {
      mode = "root";
      selected = 0;
      query = "";
      await refreshRoot();
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

    // The clipboard's own keys, which only exist while it is open.
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
      if (event.key === "Delete") {
        event.preventDefault();
        void clipboardView?.remove();
        return;
      }
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (count) selected = (selected + 1) % count;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      if (count) selected = (selected - 1 + count) % count;
    } else if (event.key === "Enter") {
      event.preventDefault();
      void openSelected();
    } else if (event.key === "Escape") {
      event.preventDefault();
      void goBack();
    }
  }

  // Typing at the root re-ranks; inside a command the query is the extension's.
  $effect(() => {
    query;
    if (mode === "root") void refreshRoot();
  });

  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    let shown: UnlistenFn | undefined;
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
        // Asked for explicitly, because the launcher is not where you were
        // when you left: reopening on a half-finished command is only right
        // if you meant to come back to it.
        if (prefs?.hotkey.resetOnSummon && mode === "command") void goBack();

        // A frame's grace so focus lands after the window is actually up.
        requestAnimationFrame(() => {
          searchInput?.focus();
          // Selected rather than cleared, so typing replaces the old query
          // but it is still there if the summon was accidental.
          if (prefs?.hotkey.selectQueryOnSummon ?? true) searchInput?.select();
        });
      });

      // The settings window writes preferences in another webview, so the
      // launcher hears about them rather than re-reading on a timer.
      changed = await listen<Preferences>("sill://preferences-changed", ({ payload }) => {
        prefs = payload;
        applyAppearance(payload);
      });

      if (disposed) return;
      clipboardActions = await actionsFor("clipboard");
      prefs = await getPreferences();
      applyAppearance(prefs);
      await refreshRoot();
      searchInput?.focus();
    })();

    return () => {
      disposed = true;
      // A pending file query has nowhere to land once this is torn down.
      clearTimeout(fileTimer);
      unlisten?.();
      shown?.();
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
    {:else if running}
      <span class="crumb">{running.extensionTitle}</span>
    {/if}
    <input
      bind:this={searchInput}
      bind:value={query}
      placeholder={mode === "argument"
        ? "Type what to search for, then Enter…"
        : mode === "clipboard"
          ? "Filter what you have copied…"
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
  {:else if mode === "clipboard"}
    <ClipboardView
      bind:this={clipboardView}
      {query}
      {selected}
      onselect={(i) => (selected = i)}
      oncount={(n) => (clipboardCount = n)}
    />
  {:else if mode === "root"}
    <RootList
      {commands}
      {selected}
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
