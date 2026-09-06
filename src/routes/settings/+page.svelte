<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    checkForUpdate,
    installUpdate,
    restartForUpdate,
    updateState,
    updateWords,
    whenUpdateChanges,
    NOTHING_KNOWN,
    type UpdateState,
  } from "$lib/update";
  import SettingsIcon, { type IconName } from "$lib/components/SettingsIcon.svelte";
  import LaunchIcon from "$lib/components/LaunchIcon.svelte";
  import { COLOURS as MARKUP_COLOURS } from "$lib/markup";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import Instead from "$lib/components/Instead.svelte";
  import Section from "$lib/components/settings/Section.svelte";
  import Row from "$lib/components/settings/Row.svelte";
  import Segmented from "$lib/components/settings/Segmented.svelte";
  import Slider from "$lib/components/settings/Slider.svelte";
  import PathList from "$lib/components/settings/PathList.svelte";
  import DriveList from "$lib/components/settings/DriveList.svelte";
  import TermList from "$lib/components/settings/TermList.svelte";
  import IndexList from "$lib/components/settings/IndexList.svelte";
  import EmojiPanel from "$lib/components/settings/EmojiPanel.svelte";
  import Button from "$lib/components/settings/Button.svelte";
  import AiPanel from "$lib/components/settings/AiPanel.svelte";
  import Select from "$lib/components/settings/Select.svelte";
  import TextField from "$lib/components/settings/TextField.svelte";
  import DictationPanel from "$lib/components/settings/DictationPanel.svelte";
  import TtsPanel from "$lib/components/settings/TtsPanel.svelte";
  import WidgetsPanel from "$lib/components/settings/WidgetsPanel.svelte";
  import ActivityPanel from "$lib/components/settings/ActivityPanel.svelte";
  import ClipboardPanel from "$lib/components/settings/ClipboardPanel.svelte";
  import SnippetsPanel from "$lib/components/settings/SnippetsPanel.svelte";
  import AutomationsPanel from "$lib/components/settings/AutomationsPanel.svelte";
  import McpPanel from "$lib/components/settings/McpPanel.svelte";
  import QuicklinksPanel from "$lib/components/settings/QuicklinksPanel.svelte";
  import ShortcutsPanel from "$lib/components/settings/ShortcutsPanel.svelte";
  import ThemeCards from "$lib/components/settings/ThemeCards.svelte";
  import ExtensionsPanel from "$lib/components/settings/ExtensionsPanel.svelte";
  import { shortRevision, storePins, type Pin } from "$lib/store";
  import {
    applyAppearance,
    clearUsageHistory,
    browserProfiles,
    getDiagnostics,
    getTimings,
    type AfterCapture,
    type KnownBrowser,
    searchEngines,
    type SearchEngine,
    getPreferences,
    hotkeyConflicts,
    openDataFolder,
    openLog,
    exportDiagnostics,
    rebuildIndex,
    listOwnSettings,
    setPreferences,
    exportPreferences,
    importPreferences,
    resetPanel,
    resettablePanels,
    type Backdrop,
    type SettingEntry,
    type InterfaceFont,
    type Theme,
    type Diagnostics,
    type Timings,
    type Preferences,
  } from "$lib/settings";
  import { forgetUnreadable, statusTroubles, type Trouble } from "$lib/status";
  import { swap } from "$lib/motion";
  import "$lib/theme/theme.css";
  import { hint } from "$lib/hint";

  type PanelId = IconName;

  interface Panel {
    id: PanelId;
    name: string;
    /** The one line under the panel title. */
    blurb: string;
    /**
     * The heading this panel sits under in the sidebar.
     *
     * Set on the FIRST panel of a run; the sidebar emits a label whenever the
     * value changes while walking the array in order, so the array's order is
     * the grouping and the two cannot disagree. The first few panels carry no
     * group on purpose: General and Appearance are the ones everybody opens,
     * and putting a heading above them buries them.
     */
    group?: string;
  }

  const PANELS: Panel[] = [
    {
      id: "general",
      name: "General",
      blurb: "Startup, the tray icon and what Sill does when it opens",
    },
    {
      id: "appearance",
      name: "Appearance",
      blurb: "Window size, backdrop material and how deep the glass sits",
    },
    {
      id: "snippets",
      name: "Snippets",
      blurb: "Saved text, expanded by keyword or pasted from the launcher",
      group: "Workflow",
    },
    {
      id: "quicklinks",
      name: "Quicklinks",
      blurb: "Saved addresses that take what you type and go straight there",
    },
    {
      id: "automations",
      name: "Automations",
      blurb: "Triggers Windows runs on a schedule, so Sill runs nothing while it waits",
    },
    {
      id: "mcp",
      name: "MCP Servers",
      blurb: "Programs whose tools appear in the action panel, started only when you run one",
    },
    {
      id: "clipboard",
      name: "Clipboard History",
      blurb: "What is kept from everything you copy, and for how long",
    },
    {
      id: "emoji",
      name: "Emoji",
      blurb: "Skin tone, and what Enter does with the one you picked",
    },
    {
      id: "shortcuts",
      name: "Shortcuts",
      blurb: "Every key Sill answers to, from the summon key down to the action keys",
    },
    {
      id: "screenshot",
      name: "Screenshots",
      blurb: "What happens after you take one, and what the editor opens with",
    },
    {
      id: "sources",
      name: "Sources",
      blurb: "Where results come from: installed programs, browser pages and the web",
      group: "Search",
    },
    {
      id: "files",
      name: "File Search",
      blurb: "Everything integration, match rules and the folders it covers",
    },
    {
      id: "extensions",
      name: "Extensions",
      blurb: "Raycast extensions installed into Sill's host",
    },
    {
      id: "scripts",
      name: "Scripts",
      blurb: "Folders of scripts the launcher can find and run",
    },
    {
      id: "ai",
      name: "AI Chat",
      blurb: "Who answers when you press Tab in the launcher",
      group: "AI Chat",
    },
    {
      id: "dictation",
      name: "Dictation",
      blurb: "The trigger, where the transcript goes, and which engine hears it",
      group: "Voice",
    },
    {
      id: "tts",
      name: "Text to Speech",
      blurb: "Which voice reads text out loud, and where it comes from",
    },
    {
      id: "widgets",
      name: "Widgets",
      blurb: "The clock, the weather, and what rides along in the launcher",
      group: "Widgets",
    },
    {
      id: "advanced",
      name: "Advanced",
      blurb: "The index, usage history and where Sill keeps its data",
      group: "System",
    },
    {
      id: "about",
      name: "About",
      blurb: "Version, licence and what Sill is built on",
    },
  ];

  /**
   * The themes on offer, in the order they are shown.
   *
   * Names and one-line notes only. Not a colour in sight: the swatches render
   * themselves from `[data-theme]`, so this list cannot drift from the
   * palettes the way a table of hex values here would.
   */
  /** Themes that paint a chroma wash. A slider for a theme with none is a
      control that visibly does nothing, which is worse than no control. */
  const CHROMATIC: Theme[] = ["oilslick", "aberration"];

  type SourceKey = Exclude<
    keyof Preferences["sources"],
    "excluded" | "hidden" | "folders"
  >;

  /** The switches that decide what a scan goes and looks at. */
  const SOURCES: { key: SourceKey; title: string; hint: string }[] = [
    {
      key: "shortcuts",
      title: "Start Menu, Desktop and taskbar",
      hint: "Shortcuts from every folder Windows itself lists, including pinned items",
    },
    {
      key: "packagedApps",
      title: "Store and packaged applications",
      hint: "Calculator, Terminal, Photos and anything installed from the Microsoft Store",
    },
    {
      key: "appPaths",
      title: "Registered executables",
      hint: "Programs an installer registered by name, resolved the way the Run dialog does",
    },
    {
      key: "installedPrograms",
      title: "Installed programs",
      hint: "Read from the uninstall registry, filtered to the entries that can actually launch",
    },
    {
      key: "pathExecutables",
      title: "Executables on PATH",
      hint: "Around 1,200 command line tools. Always ranked below real applications",
    },
    {
      key: "windowsSettings",
      title: "Windows settings pages",
      hint: "Settings pages, Control Panel applets and management consoles",
    },
    {
      key: "games",
      title: "Installed games",
      hint: "Read from your Steam and Epic libraries, which nothing else here can see",
    },
  ];

  /**
   * Every individual setting, so search finds the row rather than the panel.
   *
   * Read from Rust rather than kept here: the launcher searches the same
   * catalogue, and two copies would drift the first time a setting was added
   * to one and not the other.
   */
  let index = $state<SettingEntry[]>([]);

  let prefs = $state<Preferences | null>(null);
  let info = $state<Diagnostics | null>(null);

  /**
   * The last settings this window sent, so it does not adopt its own echo.
   *
   * `set_preferences` tells every window what the settings now are, this one
   * included. Without this, saving here would immediately reassign `prefs` to
   * the answer, which throws away anything typed in the moment between.
   */
  let echo: string | null = null;

  /**
   * The same object always spells the same, whatever order its keys arrived in.
   *
   * Comparing `JSON.stringify` output directly would work almost always and
   * fail in the way that is hardest to see: a payload built by Rust and one
   * built by spreading an object in the window can carry identical settings in
   * a different order, and the difference would read as somebody else having
   * changed something.
   */
  function canonical(value: unknown): string {
    if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;

    if (value && typeof value === "object") {
      const entries = Object.entries(value as Record<string, unknown>)
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([key, one]) => `${JSON.stringify(key)}:${canonical(one)}`);
      return `{${entries.join(",")}}`;
    }

    return JSON.stringify(value) ?? "null";
  }
  /** Where each installed extension came from, read off disk. */
  let pins = $state<Pin[]>([]);

  /**
   * The provenance line for one installed extension.
   *
   * Empty when nothing recorded it, which is true of anything installed before
   * origins existed. Saying nothing is right there: inventing "from a folder"
   * for something nobody wrote a note about would be a guess presented as a
   * fact.
   */
  function provenance(extension: string): string {
    const pin = pins.find((it) => it.extension === extension);
    if (!pin) return "";
    if (pin.source === "store") return ` · store, ${shortRevision(pin.revision)}`;
    return ` · ${pin.path}`;
  }

  /**
   * What reaching the launcher has cost this session.
   *
   * Read once when this window opens rather than watched: these are facts
   * about summons that have already happened, and none can happen while this
   * window is the one in front.
   */
  let timings = $state<Timings | null>(null);
  /** The engines Sill knows, named by Rust so the list is stated once. */
  let engines = $state<SearchEngine[]>([]);
  /** Which browsers are installed, so the pane can show them. */
  let browsers = $state<KnownBrowser[]>([]);

  /**
   * The browsers, written the way somebody would say them.
   *
   * Named rather than counted. "Reads 3 browsers" says how much is on offer
   * and not one thing about whether you want it.
   */
  const browsersFound = $derived.by(() => {
    const names = browsers.map((b) => b.name);
    if (names.length === 0) return "";
    if (names.length === 1) return names[0];

    return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
  });
  let active = $state<PanelId>("general");
  let status = $state("");
  /**
   * Which key is being recorded, rather than whether one is.
   *
   * There is more than one now, and a shared boolean would send whatever the
   * user pressed to the summon key regardless of which row they clicked.
   */
  /** Accelerators Windows refused because something else already has them. */
  let conflicts = $state<string[]>([]);

  /**
   * Everything Sill is quietly not doing.
   *
   * The tray icon that was not created, the startup entry that was not
   * written, the copied image that was not stored, a saved file that did not
   * save, and anything this window itself could not read. All of them used to
   * end at a log line, so the switch said one thing and the machine did
   * another with nothing on screen to tell them apart.
   *
   * Shown above the panels rather than inside one, because it is about the
   * application and not about whichever section happens to be open. Kept
   * current by an event, so something that breaks while this window is up
   * appears without reopening it.
   */
  let troubles = $state<Trouble[]>([]);

  /** The trouble about one thing, for the row that sets that thing. */
  function troubleOf(id: string): Trouble | undefined {
    return troubles.find((one) => one.id === id);
  }

  /**
   * The panel a trouble is about, when it names one this window still has.
   *
   * Checked rather than trusted. The section is a string Rust chose beside the
   * code that failed, and a panel that gets renamed or removed would otherwise
   * leave an offer to go somewhere that no longer exists.
   */
  function panelFor(trouble: Trouble): PanelId | null {
    return PANELS.find((panel) => panel.id === trouble.section)?.id ?? null;
  }
  let filter = $state("");
  let clearing = $state(false);
  let rebuilding = $state(false);
  let exporting = $state(false);
  /** Where the last bundle was written, so the row can say rather than imply. */
  let exported = $state("");

  const panel = $derived(PANELS.find((p) => p.id === active) ?? PANELS[0]);

  const matches = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    if (!needle) return null;

    return index.filter(
      (entry) =>
        entry.title.toLowerCase().includes(needle) ||
        entry.keywords.includes(needle) ||
        entry.panelName.toLowerCase().includes(needle),
    );
  });

  /**
   * Saves on every change.
   *
   * Rust writes the file immediately rather than debouncing: a debounced write
   * has to be flushed on shutdown, and that flush is the part that gets
   * forgotten. Settings change rarely enough that the cost does not matter.
   *
   * Takes nothing: it saves what `prefs` already holds, and most panels write
   * into `prefs` directly. A panel that builds a new object instead calls
   * `commitWith`, which is a separate function on purpose. See below.
   */
  async function commit() {
    if (!prefs) return;
    try {
      const next = $state.snapshot(prefs);
      // Before the call, not after: the event can arrive while it is still in
      // flight.
      echo = canonical(next);
      await setPreferences(next);
      applyAppearance(next);
      // A rebind is exactly when a key turns out to be taken, so this is
      // asked again rather than read once at startup.
      conflicts = await hotkeyConflicts();

      status = "Saved";
      setTimeout(() => (status = ""), 1200);
    } catch (err) {
      status = `Could not save: ${err}`;
    }
  }

  /**
   * Adopts a whole settings object, then saves it.
   *
   * For the panels that build a new object rather than writing into `prefs`.
   * Its own function rather than an optional parameter on `commit`, and the
   * reason is worth keeping: `commit` is handed straight to controls as their
   * change callback in twenty-eight places, and a `Toggle` calls its callback
   * with a boolean. One function taking an optional `Preferences` would let a
   * switch assign `true` over the entire settings object, which is a worse bug
   * than the one being fixed and would type-check.
   *
   * The bug being fixed: the Shortcuts panel declares
   * `commit: (next: Preferences) => void` and was handed the zero-argument
   * `commit`, so everything it wrote was dropped for three days. A
   * zero-argument function satisfies a one-argument type, so nothing said so.
   * `scripts/verify-source.mjs` now refuses that pairing.
   */
  async function commitWith(next: Preferences) {
    prefs = next;
    await commit();
  }

  async function forgetHistory() {
    clearing = true;
    try {
      await clearUsageHistory();
      info = await getDiagnostics();
      status = "Usage history cleared";
      setTimeout(() => (status = ""), 1600);
    } catch (err) {
      status = `Could not clear history: ${err}`;
    } finally {
      clearing = false;
    }
  }

  /**
   * Writes the bundle and says where it went.
   *
   * The path is kept on screen afterwards rather than shown for a moment and
   * cleared: somebody who has just been asked for diagnostics needs to find
   * the file again, possibly after reading it, and a message that has already
   * faded is no help.
   */
  async function exportBundle() {
    exporting = true;
    try {
      exported = await exportDiagnostics();
      status = "Diagnostics written";
      setTimeout(() => (status = ""), 1600);
    } catch (err) {
      exported = "";
      status = `Could not write diagnostics: ${err}`;
    } finally {
      exporting = false;
    }
  }

  /** The panels Rust says have something of their own to put back. */
  let resettable = $state<string[]>([]);
  /** Which panel is a click away from being reset, or none. */
  let confirmingReset = $state<PanelId | null>(null);

  /*
   * Leaving a panel disarms its Reset button.
   *
   * There are four places `active` is assigned, and an armed button left over
   * from a panel somebody has since walked away from is one stray click from
   * resetting settings they were not even looking at. Written as an effect on
   * `active` rather than as a line in each of the four, because the fourth one
   * added later is exactly the one that would not get it.
   */
  $effect(() => {
    void active;
    confirmingReset = null;
  });
  /** What the last export or import did, said where the buttons are. */
  let transfer = $state("");
  let transferring = $state(false);

  /**
   * Writes every setting to a file, and says where it went.
   *
   * The sentence about credentials is here rather than only in the file,
   * because somebody about to send this to a colleague deserves to know what
   * is in it before they choose where to put it.
   */
  async function sendOut() {
    transfer = "";
    transferring = true;

    try {
      const where = await exportPreferences();
      if (where) transfer = `Written to ${where}. No API keys or tokens are in it.`;
    } catch (err) {
      transfer = `${err}`;
    } finally {
      transferring = false;
    }
  }

  /**
   * Reads a settings file over what is here, and says exactly what changed.
   *
   * Named rather than counted, because "12 settings changed" is a number
   * nobody can check and "appearance, hotkey, sources" is one they can go and
   * look at. Nothing is reassigned here: the save emits
   * `sill://preferences-changed` and the listener above adopts it, which is
   * the same path a change made in the launcher takes.
   */
  async function bringIn() {
    transfer = "";
    transferring = true;

    try {
      const done = await importPreferences();
      if (!done) return;

      const said: string[] = [];

      if (done.sections.length) said.push(`read ${done.sections.join(", ")}`);
      if (done.snippets) said.push(`${done.snippets.added} snippets added`);
      if (done.quicklinks) said.push(`${done.quicklinks.added} quicklinks added`);
      if (done.keptKeys) {
        said.push(
          `${done.keptKeys} ${done.keptKeys === 1 ? "key was" : "keys were"} not in the file, ` +
            "so the ones already here were kept",
        );
      }

      transfer = said.length
        ? `From ${done.readAs}: ${said.join("; ")}.`
        : `Nothing in ${done.readAs} that Sill could use.`;

      conflicts = await hotkeyConflicts();
    } catch (err) {
      transfer = `${err}`;
    } finally {
      transferring = false;
    }
  }

  /**
   * Puts the open panel back to what it shipped with.
   *
   * Two clicks, because there is no undo and the second click is the whole of
   * the warning. Which sections go back is Rust's decision, so this cannot
   * quietly reset the panel next to it.
   */
  async function putBack(panel: PanelId) {
    if (confirmingReset !== panel) {
      confirmingReset = panel;
      return;
    }

    confirmingReset = null;

    try {
      await resetPanel(panel);
      status = "Put back to the defaults";
      setTimeout(() => (status = ""), 1600);
    } catch (err) {
      status = `Could not reset: ${err}`;
    }
  }

  async function rebuild() {
    rebuilding = true;
    status = "Rescanning";
    try {
      await rebuildIndex();
      // The scan runs in the background, so the count is asked for again once
      // it has had a moment rather than read back immediately.
      setTimeout(async () => {
        info = await getDiagnostics();
        rebuilding = false;
        status = "Index rebuilt";
        setTimeout(() => (status = ""), 1600);
      }, 1500);
    } catch (err) {
      rebuilding = false;
      status = `Could not rebuild: ${err}`;
    }
  }

  /**
   * What the hook is actually doing, in a sentence.
   *
   * Installed and seeing keys is the only healthy answer. Installed and stuck
   * at zero is the one worth saying out loud: Windows takes a low-level hook
   * away without telling anybody if its callback ever runs long, and from the
   * inside everything still looks armed.
   */
  const hookStory = $derived.by(() => {
    if (!info) return "Reading.";
    if (!info.keyboardHookInstalled) return "Not installed. Nothing on it is running.";
    if (info.keyboardKeysSeen === 0) {
      return "Installed, but it has not seen a keystroke. If you have typed since Sill started, Windows has taken it away and expansion will not fire.";
    }
    return "Installed and seeing keys.";
  });

  /** Only jump if the name is real, so a stale link cannot blank the page. */
  function jumpTo(name: string | null) {
    if (name && PANELS.some((p) => p.id === name)) {
      active = name as PanelId;
      filter = "";
    }
  }

  /**
   * Shows the window, once there is something in it to look at.
   *
   * Rust builds this window hidden, because Tauri shows one the moment it is
   * built and the frame then sits empty until SvelteKit has loaded, hydrated
   * and painted. That empty frame is the first thing anybody sees of
   * settings.
   *
   * Two frames rather than one. `onMount` runs after the component tree
   * exists and before the browser has drawn it; the first `requestAnimationFrame`
   * is the frame that draws it, and the second is after that frame is on the
   * glass. Showing on the first one still catches the window mid-paint.
   *
   * Focus is taken here rather than at creation, together with visibility: a
   * window created focused and invisible takes the foreground from whatever
   * somebody was in, which is a bug `lazy_windows` already paid for once.
   *
   * Failing is not fatal and deliberately quiet. A window that cannot show
   * itself is a window nobody can see, so there is nowhere to put the message
   * anyway; the log has it.
   */
  async function showOnceDrawn() {
    await new Promise((paint) => requestAnimationFrame(() => requestAnimationFrame(paint)));

    try {
      const self = getCurrentWindow();
      await self.show();
      await self.setFocus();
    } catch (err) {
      console.error("the settings window could not show itself", err);
    }
  }

  /**
   * Whether there is a newer Sill, as Rust last said.
   *
   * Starts at "nothing known" rather than "up to date", so a window that
   * cannot reach Rust says it does not know instead of claiming to be current.
   */
  let update = $state<UpdateState>(NOTHING_KNOWN);

  /**
   * Installs the newer Sill, or restarts into one already downloaded.
   *
   * Two presses behind one button, decided by the state the row just drew, so
   * the label and the action cannot disagree. The failure is left to the
   * update row itself, which shows it in words on the next event: this window
   * has somewhere to say it, which is exactly why the launcher does not.
   */
  async function applyUpdate() {
    if (update.progress.kind === "ready") {
      void restartForUpdate();
      return;
    }
    await installUpdate().catch(() => {});
  }

  onMount(() => {
    void showOnceDrawn();

    let unlisten: UnlistenFn | undefined;
    let changed: UnlistenFn | undefined;
    let wrong: UnlistenFn | undefined;
    let updating: UnlistenFn | undefined;

    (async () => {
      // A deep link opens straight at its panel: "About Sill" landing on
      // whatever was last shown would not be an About link at all.
      jumpTo(new URLSearchParams(window.location.search).get("section"));

      // The same link arriving while settings is already open.
      unlisten = await listen<string>("sill://settings-section", ({ payload }) =>
        jumpTo(payload),
      );

      /*
       * Whether there is a newer Sill.
       *
       * Read, then watched. The launcher may have found one already, and this
       * window opening is not a reason to ask again: `checkForUpdate` without
       * `force` does nothing unless a day has passed, and the button below is
       * how somebody asks on purpose.
       */
      update = await updateState("settings");
      updating = await whenUpdateChanges((progress) => {
        update = { ...update, progress };
      });
      void checkForUpdate();

      /*
       * Settings written anywhere else.
       *
       * This window is not the only thing that writes them. Naming a result in
       * the launcher, binding a key to one, or hiding one all go through
       * `set_preferences`, and this held a copy taken when it opened. So the
       * next toggle pressed here sent that stale copy back and **undid the
       * alias somebody had just made**, with no error and nothing on screen to
       * suggest it had happened.
       *
       * Adopting the payload is the whole fix. The echo check is what keeps it
       * from being disruptive: every save comes back here too, and reassigning
       * `prefs` mid-edit would take the cursor out of a field.
       */
      changed = await listen<Preferences>("sill://preferences-changed", ({ payload }) => {
        if (canonical(payload) === echo) return;

        prefs = payload;
        applyAppearance(payload);
      });

      // Something that breaks while this window is open appears in it, rather
      // than the next time somebody happens to reopen it.
      wrong = await listen<Trouble[]>("sill://status-changed", ({ payload }) => {
        troubles = payload;
      });

      /*
       * Cleared before the reads below, not after each one.
       *
       * Every read this window does reports itself when it fails, and it does
       * all of them again on every open, so whatever was reported last time is
       * stale before the first answer arrives. Clearing them one by one as
       * they succeed would put an extra call on the path that worked, and the
       * path that worked is the one that should cost nothing.
       */
      await forgetUnreadable("settings");

      try {
        prefs = await getPreferences();
        conflicts = await hotkeyConflicts();
        applyAppearance(prefs);
        index = await listOwnSettings();
        info = await getDiagnostics();
        // Small files beside the bundles, so this is a directory listing
        // rather than anything that reaches the network. Opening settings
        // must not fetch a catalogue.
        pins = await storePins();
        timings = await getTimings();
        browsers = await browserProfiles();
        engines = await searchEngines();
        resettable = await resettablePanels();
      } catch (err) {
        status = `Could not load settings: ${err}`;
      }

      // Last, so it holds whatever the reads above just reported about
      // themselves as well as everything Rust had already recorded.
      troubles = await statusTroubles();
    })();

    return () => {
      unlisten?.();
      changed?.();
      wrong?.();
      updating?.();
    };
  });
</script>


<div class="window">
  <TitleBar />

  <div class="body">
    <aside>
      <div class="search">
        <svg
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.2"
          aria-hidden="true"
        >
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.5-3.5" stroke-linecap="round" />
        </svg>
        <input
          bind:value={filter}
          placeholder="Search settings"
          aria-label="Search settings"
          spellcheck="false"
        />
        {#if filter}
          <button class="clear" aria-label="Clear search" onclick={() => (filter = "")}>
            <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true">
              <path
                d="M1 1l10 10M11 1L1 11"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
              />
            </svg>
          </button>
        {/if}
      </div>

      {#if matches}
        <div class="group">Results</div>
        <nav>
          {#each matches as match (match.panel + match.title)}
            <button class="result" onclick={() => jumpTo(match.panel)}>
              <span class="result-tile">
                <SettingsIcon name={match.panel as PanelId} size={13} />
              </span>
              <span class="result-text">
                <span class="result-title">{match.title}</span>
                <span class="result-panel">{match.panelName}</span>
              </span>
            </button>
          {/each}
          {#if matches.length === 0}
            <p class="no-results">Nothing matches that.</p>
          {/if}
        </nav>
      {:else}
        <!--
          One flat list of thirteen panels was a wall. A heading appears
          wherever `group` is set, which is the first panel of each run, so the
          array's order IS the grouping and a panel cannot end up under the
          wrong heading. General and Appearance lead with no heading at all,
          because they are what somebody opening settings came for.
        -->
        <nav>
          {#each PANELS as item (item.id)}
            {#if item.group}
              <div class="nav-label">{item.group}</div>
            {/if}
            <button
              class="nav-item"
              class:selected={item.id === active}
              onclick={() => (active = item.id)}
            >
              <SettingsIcon name={item.id} size={26} />
              {item.name}
            </button>
          {/each}
        </nav>
      {/if}
    </aside>

    <main>
      <header>
        <SettingsIcon name={panel.id} size={38} />
        <div class="hero-text">
          <h2>{panel.name}</h2>
          <p>{panel.blurb}</p>
        </div>
        {#if status}<span class="status">{status}</span>{/if}
        <!--
          In the header rather than as a row at the foot of each panel, because
          it is about the whole panel and not one setting in it, and because a
          row would have to be repeated in eighteen branches with eighteen
          chances to name the wrong panel.

          Drawn only where Rust says there is something to put back, so the
          button never makes a promise `reset_panel` would then refuse. Two
          clicks: there is no undo, and the second click is the whole warning.
        -->
        {#if resettable.includes(panel.id)}
          <Button
            label={confirmingReset === panel.id ? "Reset, and lose these" : "Reset"}
            tone="danger"
            onclick={() => void putBack(panel.id)}
          />
        {/if}
      </header>

      <!--
        Above the panels, because a thing that is not working is about Sill and
        not about whichever section happens to be open. Outside `.content` as
        well as above it: that is a one-cell grid holding the panel swap, so a
        sibling inside it would be laid over the panel rather than beside it.

        A quiet block of sentences rather than a notice that demands anything.
        Every one of these is a state somebody can come and read, which is why
        none of them is a toast: a failed clipboard image would raise one per
        copy, and the thing that is actually wrong is wrong once.
      -->
      {#if troubles.length > 0}
        <section class="wrong" aria-label="What is not working">
          {#each troubles as trouble (trouble.id)}
            {@const goes = panelFor(trouble)}
            <p>
              {trouble.message}
              {#if goes && goes !== active}
                <button class="goto" onclick={() => (active = goes)}>Go there</button>
              {/if}
            </p>
          {/each}
        </section>
      {/if}

      <div class="content">
        {#if !prefs}
          <Instead tone="loading" headline={status || "Reading your settings"} />
        {:else}
          {@const p = prefs}
          <!--
            Keyed on the panel, so switching one out and the next one in is a
            change the user can follow rather than a jump. Opacity and a 4px
            lift only: the heading above does not move, so the window reads as
            its contents being replaced rather than as the whole pane sliding.
          -->
          {#key active}
          <div class="panel-body" in:swap out:swap={{ out: true }}>
          {#if active === "general"}
          <Section label="Startup">
            <!--
              Both rows say so themselves when the thing they switch did not
              happen, the same way a refused hotkey marks the row that set it.
              A switch reading "on" over a startup entry that was never written
              is the whole problem: the block above says what is wrong, and
              this is what says which control is lying about it.
            -->
            <Row
              title="Open at login"
              description={troubleOf("autostart")?.message ??
                "Sill starts with Windows and waits quietly for the hotkey."}
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.general.openAtLogin}
                  onchange={commit}
                  label="Open at login"
                />
              {/snippet}
            </Row>
            <Row
              title="Show in the system tray"
              description={troubleOf("tray")?.message ??
                "Sill has no taskbar button, so the tray icon is the only sign it is running. Left click summons it, right click opens a menu."}
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.general.showInTray}
                  onchange={commit}
                  label="Show in tray"
                />
              {/snippet}
            </Row>
            <!--
              The row that says Sill is not recording, and the only place in
              Settings that can say it. Every switch it overrides lives on
              another panel and goes on reading "on" while it is: the Clipboard
              panel's own toggle is about whether the history is wanted, not
              about whether anything is being written right now.

              The description carries the standing report while it is on, the
              same way the two rows above carry theirs, so the panel says which
              state it is in rather than only offering the switch.
            -->
            <Row
              title="Private mode"
              description={troubleOf("privacy")?.message ??
                "Pauses the clipboard history, dictation and screen capture together. It stays on until you switch it off, restarts included."}
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.privacy.paused}
                  onchange={commit}
                  label="Private mode"
                />
              {/snippet}
            </Row>
          </Section>

          <Section label="Unfinished">
            <!--
              A prototype, and the row says so rather than the release notes
              doing it. Somebody who reads "Notes" in a launcher expects
              folders, tags and formatting; what is behind this is one note at
              a time in one window, and the honest place to say that is the
              line under the switch they are about to press.
            -->
            <Row
              title="Notes"
              description="A prototype. One note at a time, in its own window. Type note in the launcher to find one. Off means Sill never opens the file at all."
            >
              {#snippet control()}
                <Toggle bind:checked={p.general.notes} onchange={commit} label="Notes" />
              {/snippet}
            </Row>
          </Section>

          <Section label="Opening and closing">
            <Row
              title="Hide when it loses focus"
              description="Clicking away dismisses Sill, the same as pressing Escape."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.hotkey.dismissOnBlur}
                  onchange={commit}
                  label="Hide on blur"
                />
              {/snippet}
            </Row>
            <Row
              title="Select the search text"
              description="Typing replaces the last query instead of appending to it, but it is still there if the summon was accidental."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.hotkey.selectQueryOnSummon}
                  onchange={commit}
                  label="Select query"
                />
              {/snippet}
            </Row>
            <Row
              title="Return to the root list"
              description="Otherwise Sill reopens on whatever command was last running, as long as you come straight back to it."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.hotkey.resetOnSummon}
                  onchange={commit}
                  label="Return to root"
                />
              {/snippet}
            </Row>
          </Section>
          {:else if active === "appearance"}
          <Section
            label="Theme"
            description="A theme moves the canvas and the accent. Everything that carries meaning, the text steps, the hairlines, the fills, is the same in all of them, so nothing gets harder to read whichever you pick."
            bare
          >
            <ThemeCards
              value={p.appearance.theme}
              onpick={(id) => {
                if (!prefs) return;
                p.appearance.theme = id;
                void commit();
              }}
            />

            {#if CHROMATIC.includes(p.appearance.theme)}
              <Row
                title="Chroma"
                description="How strongly the iridescent wash is painted. The three hues move together, so this changes its weight without changing its balance."
              >
                {#snippet control()}
                  <Slider
                    value={Math.round(p.appearance.chromaStrength * 100)}
                    min={0}
                    max={160}
                    step={5}
                    label="Chroma"
                    format={(v) => `${v}%`}
                    onchange={(v) => {
                      if (!prefs) return;
                      p.appearance.chromaStrength = v / 100;
                      void commit();
                    }}
                  />
                {/snippet}
              </Row>
            {/if}
          </Section>

          <Section
            label="Material"
            description="Windows composites the desktop behind the window. These decide how much of it shows through."
          >
            <Row
              title="Backdrop"
              description="Acrylic adds a luminosity layer of its own, so it always lightens a little. Blur lets the tint below decide the depth. None is the deepest."
            >
              {#snippet control()}
                <Segmented
                  label="Backdrop"
                  value={p.appearance.backdrop}
                  options={[
                    { value: "acrylic", label: "Acrylic" },
                    { value: "blur", label: "Blur" },
                    { value: "none", label: "None" },
                  ]}
                  onchange={(next) => {
                    if (!prefs) return;
                    p.appearance.backdrop = next as Backdrop;
                    void commit();
                  }}
                />
              {/snippet}
            </Row>
            <Row
              title="Interface font"
              description="Satoshi and Inter are bundled, so they look the same on every machine; Segoe UI Variable is the one Windows ships. The window is transparent so the desktop can show through, and that means text is drawn without using the display's subpixels, whichever face you pick. Satoshi is the default because it holds its weight best under that. Judge them on your own screen."
            >
              {#snippet control()}
                <Segmented
                  label="Interface font"
                  value={p.appearance.font}
                  options={[
                    { value: "satoshi", label: "Satoshi" },
                    { value: "inter", label: "Inter" },
                    { value: "system", label: "Segoe UI" },
                  ]}
                  onchange={(next) => {
                    if (!prefs) return;
                    prefs.appearance.font = next as InterfaceFont;
                    void commit();
                  }}
                />
              {/snippet}
            </Row>

            <Row
              title="Backdrop depth"
              description="How dark the tint sits behind the glass. Higher hides more of the desktop."
            >
              {#snippet control()}
                <Slider
                  bind:value={p.appearance.tintAlpha}
                  min={120}
                  max={255}
                  label="Backdrop depth"
                  format={(v) => `${Math.round((v / 255) * 100)}%`}
                  onchange={commit}
                />
              {/snippet}
            </Row>
            <Row
              title="Glass strength"
              description="At zero the window paints itself solid, which is the readable choice over a busy desktop."
            >
              {#snippet control()}
                <Slider
                  bind:value={p.appearance.glassStrength}
                  min={0}
                  max={1}
                  step={0.05}
                  label="Glass strength"
                  format={(v) => `${Math.round(v * 100)}%`}
                  onchange={commit}
                />
              {/snippet}
            </Row>
          </Section>

          <Section
            label="Window"
            description="Applied straight away. The launcher re-centres so it does not walk across the screen."
          >
            <Row title="Rows before scrolling" description="Sets the launcher's height.">
              {#snippet control()}
                <Slider
                  bind:value={p.appearance.visibleRows}
                  min={4}
                  max={16}
                  label="Rows before scrolling"
                  format={(v) => `${v} rows`}
                  onchange={commit}
                />
              {/snippet}
            </Row>
            <Row
              title="Where it appears"
              description="With more than one screen, the launcher used to always come up on the primary one. It now follows this on every summon, which is also what brings it back if a display change left it off-screen."
            >
              {#snippet control()}
                <Select
                  value={p.appearance.summonOn}
                  options={[
                    { value: "cursor", label: "Screen with the mouse" },
                    { value: "activeWindow", label: "Screen you were working on" },
                    { value: "primary", label: "Primary screen" },
                  ]}
                  onchange={(next) => {
                    if (!prefs) return;
                    p.appearance.summonOn = next as typeof p.appearance.summonOn;
                    void commit();
                  }}
                  ariaLabel="Where it appears"
                />
              {/snippet}
            </Row>
            <Row title="Window width" description="How wide the launcher sits, in pixels.">
              {#snippet control()}
                <Slider
                  bind:value={p.appearance.windowWidth}
                  min={560}
                  max={1100}
                  step={10}
                  label="Window width"
                  format={(v) => `${v} px`}
                  onchange={commit}
                />
              {/snippet}
            </Row>
          </Section>
          {:else if active === "ai"}
            <AiPanel prefs={p} {commit} />
          {:else if active === "dictation"}
            <DictationPanel prefs={p} {commit} />
          {:else if active === "tts"}
            <TtsPanel prefs={p} {commit} />
          {:else if active === "widgets"}
            <WidgetsPanel prefs={p} {commit} />
          {:else if active === "snippets"}
            <SnippetsPanel prefs={p} {commit} />
          {:else if active === "shortcuts"}
            <ShortcutsPanel prefs={p} commit={commitWith} {conflicts} {hookStory} />
          {:else if active === "quicklinks"}
            <QuicklinksPanel />
          {:else if active === "automations"}
            <AutomationsPanel />
          {:else if active === "mcp"}
            <McpPanel prefs={p} {commit} />
          {:else if active === "emoji"}
            <EmojiPanel prefs={p} {commit} />
          {:else if active === "clipboard"}
            <ClipboardPanel prefs={p} {commit} />
          {:else if active === "sources"}
          <Section
            label="What Sill indexes"
            description="Turning one off takes its entries out. Turning one on goes and looks for them, which takes a few seconds and happens in the background."
          >
            {#each SOURCES as source (source.key)}
              <Row title={source.title} description={source.hint}>
                {#snippet control()}
                  <Toggle
                    bind:checked={p.sources[source.key]}
                    onchange={commit}
                    label={source.title}
                  />
                {/snippet}
              </Row>
            {/each}
          </Section>

          <Section
            label="Browser search"
            description="Pages your browsers remember, alongside everything else. Nothing is copied or read until you type, and it stays on this machine."
          >
            <Row
              title="Search browser pages"
              description={browsersFound
                ? `Reads ${browsersFound}.`
                : "No browser Sill knows how to read is installed on this machine."}
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.browsers.enabled}
                  onchange={commit}
                  label="Search browser pages"
                />
              {/snippet}
            </Row>
            {#if browsers.length}
              <!--
                The browsers themselves, wearing their own marks.
                Naming what will be read is the point of this pane, and a row of
                logos says it faster than a sentence and is harder to misread.
              -->
              <div class="browsers">
                {#each browsers as found (found.name)}
                  <span class="browser">
                    <LaunchIcon
                      path={found.program ?? ""}
                      label={found.name}
                      resolvable={!!found.program}
                    />
                    {found.name}
                  </span>
                {/each}
              </div>
            {/if}
            <Row
              title="Bookmarks"
              description="Pages you saved. The smaller and more deliberate set, so these rank above history."
              disabled={!p.browsers.enabled}
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.browsers.bookmarks}
                  onchange={commit}
                  label="Bookmarks"
                />
              {/snippet}
            </Row>
            <Row
              title="History"
              description="Pages you visited. Much the larger of the two, and the more revealing."
              disabled={!p.browsers.enabled}
            >
              {#snippet control()}
                <Toggle bind:checked={p.browsers.history} onchange={commit} label="History" />
              {/snippet}
            </Row>
            <Row
              title="Open browser tabs"
              description="Tabs your browsers have open right now, so typing a page's name goes to the copy already on screen. Read from the running browsers when you type, and nothing is held between one search and the next."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.browsers.tabs}
                  onchange={commit}
                  label="Open browser tabs"
                />
              {/snippet}
            </Row>
            <Row
              title="Include Firefox and browsers built on it"
              description="Firefox, Zen, LibreWolf and Waterfox keep their accessibility engine switched off until something asks, and reading tabs is the asking. Measured here, it costs that browser about 10 MB in its window's process and 85 MB across its pages, and stays until it is restarted. Chrome and Edge already expose their windows and cost nothing extra."
              disabled={!p.browsers.tabs}
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.browsers.tabsFirefox}
                  onchange={commit}
                  label="Include Firefox and browsers built on it"
                />
              {/snippet}
            </Row>
            <Row
              title="Maximum browser results"
              description="These rank below commands and files, so a high number mostly costs scrolling."
              disabled={!p.browsers.enabled}
            >
              {#snippet control()}
                <Slider
                  bind:value={p.browsers.maxResults}
                  min={2}
                  max={20}
                  step={2}
                  label="Maximum browser results"
                  format={(v) => `${v} pages`}
                  onchange={commit}
                />
              {/snippet}
            </Row>
          </Section>

          <Section
            label="Web search"
            description="The last row Sill offers, after everything that actually matched. It reads nothing and sends nothing until you choose it."
          >
            <Row
              title="Offer to search the web"
              description="Appears at the bottom of the list whenever you have typed something, so it never displaces a result."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.webSearch.enabled}
                  onchange={commit}
                  label="Offer to search the web"
                />
              {/snippet}
            </Row>
            <Row
              title="Engine"
              description="DuckDuckGo by default: it is the one that does not build a profile of whoever is typing."
              disabled={!p.webSearch.enabled || p.webSearch.customUrl.trim() !== ""}
            >
              {#snippet control()}
                <Select
                  value={p.webSearch.engine}
                  options={engines.map((option) => ({ value: option.id, label: option.name }))}
                  onchange={(next) => {
                    if (!prefs) return;
                    p.webSearch.engine = next;
                    void commit();
                  }}
                  ariaLabel="Engine"
                />
              {/snippet}
            </Row>
            <Row
              title="Your own address"
              description="Put {'{query}'} where the words go. Anything here is used instead of the engine above."
              disabled={!p.webSearch.enabled}
            >
              {#snippet children()}
                <TextField
                  value={p.webSearch.customUrl}
                  oninput={(next) => {
                    if (!prefs) return;
                    p.webSearch.customUrl = next;
                    void commit();
                  }}
                  placeholder="https://example.com/search?q={'{query}'}"
                  ariaLabel="Your own address"
                  full
                  mono
                />
              {/snippet}
            </Row>
          </Section>

          <Section
            label="Your own folders"
            description="Walked exactly as the Start Menu is, so a shortcut or a program in one of these appears with its own icon. Useful if you keep portable applications somewhere Windows does not list."
          >
            <Row title="Folders of your own">
              {#snippet children()}
                <PathList bind:paths={p.sources.folders} onchange={commit} />
              {/snippet}
            </Row>
          </Section>

          <Section
            label="Exclusions"
            description="Matched against both the name and the path, so one folder name can hide a whole vendor at once. To switch off a single entry, use the list below instead."
          >
            <Row title="Hidden entries">
              {#snippet children()}
                <TermList bind:terms={p.sources.excluded} onchange={commit} />
              {/snippet}
            </Row>
          </Section>

          <Section
            label="What Sill found"
            description="An alias, a key and being in the list at all are the same question asked three ways, so they are three columns on one row. Setting an alias is also offered on the result itself in the launcher, which is where you usually notice you want one."
            bare
          >
            <IndexList onchange={(next) => (prefs = next)} />
          </Section>

          {#if info}
            <Section label="What is indexed now" bare>
              <div class="stats">
                {#each info.bySource as source (source.mode)}
                  <div class="stat">
                    <span class="stat-value">{source.count.toLocaleString()}</span>
                    <span class="stat-label">{source.mode}</span>
                  </div>
                {/each}
              </div>
            </Section>
          {/if}
          {:else if active === "files"}
          <Section
            label="File search"
            description="Sill keeps its own index of the folders below, so this works with nothing else installed. Where a whole-volume indexer is also running it is asked as well, and it sees the rest of the machine."
          >
            <Row
              title="Search files"
              description={info?.everythingRunning
                ? "Sill's own index, plus Everything, which is running and answering for the rest of the machine."
                : "Sill's own index of the folders below. Everything is not running, so nothing outside them is searched."}
            >
              {#snippet control()}
                <Toggle bind:checked={p.files.enabled} onchange={commit} label="Search files" />
              {/snippet}
            </Row>
            <Row
              title="Maximum file results"
              description="Files rank below commands, so a high number mostly costs scrolling."
              disabled={!p.files.enabled}
            >
              {#snippet control()}
                <Slider
                  bind:value={p.files.maxResults}
                  min={5}
                  max={100}
                  step={5}
                  label="Maximum file results"
                  format={(v) => `${v} files`}
                  onchange={commit}
                />
              {/snippet}
            </Row>
          </Section>

          <Section
            label="Matching"
            description="Passed straight to Everything, where they mean exactly what they mean there."
          >
            <Row
              title="Match the whole path"
              description="Search the full path rather than only the file name."
              disabled={!p.files.enabled}
            >
              {#snippet control()}
                <Toggle bind:checked={p.files.matchPath} onchange={commit} label="Match path" />
              {/snippet}
            </Row>
            <Row
              title="Match case"
              description="Treat the query as case sensitive."
              disabled={!p.files.enabled}
            >
              {#snippet control()}
                <Toggle bind:checked={p.files.matchCase} onchange={commit} label="Match case" />
              {/snippet}
            </Row>
            <Row
              title="Regular expression"
              description="Treat the query as a regular expression instead of plain text."
              disabled={!p.files.enabled}
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.files.regex}
                  onchange={commit}
                  label="Regular expression"
                />
              {/snippet}
            </Row>
          </Section>

          <Section
            label="What Sill reads"
            description="Sill keeps its own index so file search works without anything else installed. Build output and the folders listed in .gitignore are left out, which is what keeps it small: a home folder is 2.2 million files raw and 43,000 once they are."
          >
            <Row
              title="Drives"
              description="A whole drive skips Windows and installed programs. Folders you add are read in full."
              disabled={!p.files.enabled}
            >
              {#snippet children()}
                <DriveList onchange={(roots) => (p.files.roots = roots)} />
              {/snippet}
            </Row>
            <Row
              title="Folders"
              description="Anything else worth reading. Leave empty and Sill reads your home folder."
              disabled={!p.files.enabled}
            >
              {#snippet children()}
                <PathList bind:paths={p.files.roots} onchange={commit} />
              {/snippet}
            </Row>
          </Section>

          <Section
            label="Narrowing results"
            description="A filter on what comes back, which is not the same as what gets read. With nothing listed, everything indexed can be found."
          >
            <Row title="Only show results in" disabled={!p.files.enabled}>
              {#snippet children()}
                <PathList bind:paths={p.files.onlyIn} onchange={commit} />
              {/snippet}
            </Row>
          </Section>
          {:else if active === "screenshot"}
          <Section
            label="Screenshots"
            description="Pick an area, click a window, or take every screen at once. Whatever you take goes to the clipboard, and to the editor if you want it."
          >
            <Row
              title="After taking one"
              description="The editor draws boxes, arrows, highlights and blocks over anything you have hidden. It reaches the clipboard from there either way."
            >
              {#snippet control()}
                <Segmented
                  label="After taking one"
                  value={p.screenshot.after}
                  options={[
                    { value: "copy", label: "Copy it" },
                    { value: "edit", label: "Open the editor" },
                  ]}
                  onchange={(next) => {
                    if (!prefs) return;
                    p.screenshot.after = next as AfterCapture;
                    void commit();
                  }}
                />
              {/snippet}
            </Row>
            <Row
              title="Click a window to take it"
              description="While picking an area, the window under the pointer lights up and a click takes the whole of it, even the parts something else is covering."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.screenshot.clickAWindow}
                  onchange={commit}
                  label="Click a window to take it"
                />
              {/snippet}
            </Row>
          </Section>

          <Section
            label="What the editor opens with"
            description="Where it starts each time. Everything is still changeable once it is open."
          >
            <Row title="Tool">
              {#snippet control()}
                <Segmented
                  label="Tool"
                  value={p.screenshot.tool}
                  options={[
                    { value: "box", label: "Box" },
                    { value: "arrow", label: "Arrow" },
                    { value: "pen", label: "Pen" },
                    { value: "highlight", label: "Highlight" },
                    { value: "hide", label: "Hide" },
                    { value: "step", label: "Badge" },
                  ]}
                  onchange={(next) => {
                    if (!prefs) return;
                    p.screenshot.tool = next;
                    void commit();
                  }}
                />
              {/snippet}
            </Row>
            <Row title="Colour">
              {#snippet control()}
                <div class="swatches">
                  {#each MARKUP_COLOURS as swatch (swatch.value)}
                    <button
                      class="swatch"
                      class:on={p.screenshot.colour === swatch.value}
                      style:background={swatch.value}
                      use:hint={swatch.name}
                      aria-label={swatch.name}
                      aria-pressed={p.screenshot.colour === swatch.value}
                      onclick={() => {
                        if (!prefs) return;
                        p.screenshot.colour = swatch.value;
                        void commit();
                      }}
                    ></button>
                  {/each}
                </div>
              {/snippet}
            </Row>
            <Row
              title="Badges start at"
              description="The number the first numbered badge shows. Writing the second half of a walkthrough starts at seven."
            >
              {#snippet control()}
                <Slider
                  bind:value={p.screenshot.stepFrom}
                  min={0}
                  max={20}
                  step={1}
                  label="Badges start at"
                  format={(v) => `${v}`}
                  onchange={commit}
                />
              {/snippet}
            </Row>
            <Row title="Stroke width">
              {#snippet control()}
                <Slider
                  bind:value={p.screenshot.weight}
                  min={1}
                  max={12}
                  step={1}
                  label="Stroke width"
                  format={(v) => `${v}`}
                  onchange={commit}
                />
              {/snippet}
            </Row>
          </Section>
          {:else if active === "scripts"}
          <Section
            label="Script commands"
            description="A script with a Raycast header at the top becomes a command in the launcher. Sill reads the header it already has and writes nothing back, so a script keeps working everywhere else."
          >
            <Row
              title="Run script commands"
              description="Off scans nothing at all, rather than scanning and hiding the results. The folders below are kept either way."
            >
              <Toggle
                checked={p.scripts.enabled}
                label="Run script commands"
                onchange={(on) => {
                  p.scripts.enabled = on;
                  commit();
                }}
              />
            </Row>

            <Row
              title="Stop a script after"
              description="A script that waits on something that never arrives would otherwise wait for as long as Sill runs. Whatever it printed before it was stopped is still shown."
            >
              <input
                class="number"
                type="number"
                min="1"
                max="3600"
                aria-label="Stop a script after, in seconds"
                value={p.scripts.timeoutSeconds}
                onchange={(event) => {
                  const seconds = Number((event.currentTarget as HTMLInputElement).value);
                  p.scripts.timeoutSeconds = Math.min(3600, Math.max(1, seconds || 60));
                  commit();
                }}
              />
            </Row>
          </Section>

          <Section
            label="Folders"
            description="Scanned one level deep, so a folder with a project in it does not become a scan of the whole project. Nothing is scanned until a folder is named here: a launcher that went looking for runnable things on its own would find commands nobody put there to be found."
          >
            <PathList bind:paths={p.scripts.folders} onchange={commit} />
          </Section>

          <Section
            label="Administrator rights"
            description="A script can ask for administrator rights in its own header, and asking is all it can do. A script file arrives in a checkout, in a zip, in a folder somebody shares, and the prompt Windows shows names powershell.exe rather than the script, so there is nothing on that dialog to decide with. Naming one here is the deciding, made in advance, about one file. A script that asks and is not named does not run at all, rather than quietly running without the rights it said it needed."
          >
            <PathList
              bind:paths={p.scripts.elevated}
              onchange={commit}
              files
              headline="No script may run as administrator"
              hint="A script asking for it in its header will say so and stop."
              add="Allow a script…"
              removes="Stop allowing"
            />
          </Section>

          {:else if active === "extensions"}
          <!-- The Store section stays here because it is a preference; what is
               installed, what it runs and what it may reach is a screen of its
               own, and it is long. -->
          <Section
            label="Store"
            description="Where Sill looks for extensions, and how much of the catalogue it offers."
          >
            <Row
              title="Only Windows extensions"
              description="Raycast ships for macOS and for Windows, and its store is one catalogue for both. The ones that say macOS and not Windows are never offered here at all. This decides what happens to the ones published before extensions declared a platform: hidden, or shown and marked."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.store.windowsOnly}
                  onchange={commit}
                  label="Only Windows extensions"
                />
              {/snippet}
            </Row>

            <Row
              title="GitHub token"
              description="Optional. Extension source is fetched from github.com/raycast/extensions, and GitHub answers sixty requests an hour to a machine that does not identify itself. One install spends about three. A token raises that to five thousand, and is encrypted rather than kept in the settings file."
            >
              {#snippet children()}
                <TextField
                  value={p.store.githubToken ?? ""}
                  oninput={(next) => {
                    if (!prefs) return;
                    p.store.githubToken = next.trim() === "" ? null : next;
                    void commit();
                  }}
                  placeholder="ghp_…"
                  ariaLabel="GitHub token"
                  full
                  secret
                  mono
                />
              {/snippet}
            </Row>
          </Section>

          <ExtensionsPanel nodeInstalled={info?.nodeInstalled ?? false} />
          {:else if active === "advanced"}
          <!--
            What Sill has done sits here rather than in a row of its own. The
            sidebar rule is that a sub-view folds into its parent, and Advanced
            already owns the index and the usage history: a log of what was run
            is the same kind of thing.
          -->
          <ActivityPanel />

          <Section
            label="Index"
            description="Rebuilt in the background. Searching keeps working while it runs."
          >
            <Row
              title="Rebuild the index"
              description={info
                ? `${info.indexedCommands.toLocaleString()} entries indexed.`
                : "Rescan every enabled source."}
            >
              {#snippet control()}
                <Button label="Rebuild" busy={rebuilding} onclick={rebuild} />
              {/snippet}
            </Row>
          </Section>

          <!--
            Measured rather than claimed, which is the whole point of it being
            here. A launcher's pitch is that it is quick, and that is a
            statement about numbers: these are this machine's, this session.
          -->
          <Section
            label="Reaching the launcher"
            description="Timed by Sill itself, from the key being pressed to the moment you can type. The middle of the recent ones, because an occasional slow summon is a display waking rather than the launcher."
          >
            <!-- not a setting: a reading of how long the last summon took, not a control -->
            <Row
              title="Summon"
              description={timings?.summons.length
                ? `The middle of the last ${timings.summons.length}.`
                : "Nothing measured yet this session."}
            >
              {#snippet control()}
                <span class="reading">
                  {timings?.medianMs != null ? `${timings.medianMs} ms` : "not yet"}
                </span>
              {/snippet}
            </Row>

            <!-- not a setting: a reading of how long the last start took, not a control -->
            <Row
              title="Starting up"
              description="From the process starting to the hotkey working."
            >
              {#snippet control()}
                <span class="reading">
                  {timings?.coldStartMs != null ? `${timings.coldStartMs} ms` : "not yet"}
                </span>
              {/snippet}
            </Row>
          </Section>

          <Section
            label="Keyboard"
            description="Snippet expansion, the hyper key and double-tap all run on one low-level hook. Windows removes such a hook silently if it ever runs slow, and everything on it stops at once."
          >
            <!-- not a setting: a reading of what the keyboard hook is costing, not a control -->
            <Row
              title="Keyboard hook"
              description={hookStory}
            >
              {#snippet control()}
                <span class="fact">{info?.keyboardKeysSeen.toLocaleString() ?? "—"}</span>
              {/snippet}
            </Row>
          </Section>

          <Section
            label="Ranking"
            description="Sill ranks by how often and how recently you launch something, which is why the root list is usually right before you type."
          >
            <Row
              title="Usage history"
              description={info
                ? `${info.launchedEntries.toLocaleString()} entries have been launched.`
                : "Clearing it starts ranking over."}
            >
              {#snippet control()}
                <Button
                  label="Forget history"
                  tone="danger"
                  busy={clearing}
                  onclick={forgetHistory}
                />
              {/snippet}
            </Row>
          </Section>

          <!--
            In Advanced rather than in a panel of its own, because it is about
            the settings as a whole and the sidebar rule is that a thing with
            no settings of its own folds into its parent.
          -->
          <Section
            label="Settings file"
            description="A copy to keep, to move to another machine, or to arrive with from another launcher."
          >
            <Row
              title="Export settings"
              description="Every setting in one file. No API keys and no tokens: they are locked to this Windows account, so a copy of one would both leak and not work."
            >
              {#snippet control()}
                <Button label="Export" busy={transferring} onclick={() => void sendOut()} />
              {/snippet}
            </Row>

            <Row
              title="Import settings"
              description="A Sill export, a preferences.json, PowerToys Run's settings, or a Raycast .rayconfig. Anything the file does not mention keeps what it has."
            >
              {#snippet control()}
                <Button label="Import" busy={transferring} onclick={() => void bringIn()} />
              {/snippet}
            </Row>

            {#if transfer}
              <!-- not a setting: what the last export or import did, not a control -->
              <Row title="Last transfer" description={transfer} />
            {/if}
          </Section>

          <Section
            label="Data"
            description="Preferences, the index cache and the log live in one folder."
          >
            <Row title="Data folder" description={info?.dataDir ?? ""}>
              {#snippet control()}
                <Button label="Open folder" onclick={() => void openDataFolder()} />
              {/snippet}
            </Row>

            <Row
              title="Log"
              description="What Sill did and why. A release build has nowhere else to say it, so this is the only place a failure appears."
            >
              {#snippet control()}
                <Button label="Open log" onclick={() => void openLog()} />
              {/snippet}
            </Row>

            <Row
              title="Detailed logging"
              description="Also writes what each search source and each extension cost, on every keystroke. Worth turning on while chasing a fault and worth turning off afterwards. It only ever adds: nothing here can stop a failure or a crash being written."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.general.detailedLog}
                  onchange={commit}
                  label="Detailed logging"
                />
              {/snippet}
            </Row>

            <Row
              title="Export diagnostics"
              description={exported ||
                "One file to send to somebody: the log, what is indexed, what each source cost, and anything not working. It leaves out your preferences, your keys, the clipboard and the file index, and it says so at the end so you can check before sending."}
            >
              {#snippet control()}
                <Button label="Export" busy={exporting} onclick={exportBundle} />
              {/snippet}
            </Row>
          </Section>
          {:else if active === "about"}
            <Section label="Sill" bare>
            <div class="about">
              <img src="/sill.png" alt="" width="52" height="52" />
              <div>
                <h3>Sill {info?.version ?? ""}</h3>
                <p>A launcher for Windows that runs Raycast extensions.</p>
              </div>
            </div>
          </Section>

          <Section label="Build">
            <Row title="Version" description="The running build.">
              {#snippet control()}
                <span class="fact">{info?.version ?? "unknown"}</span>
              {/snippet}
            </Row>
            <!--
              The one place a failed check is said out loud.

              The launcher stays quiet about it on purpose: somebody who opened
              a launcher to run a command is not the audience for "the update
              server did not answer". This is where they came to ask, so this
              is where it is answered, in the words `updateWords` gives both
              surfaces.
            -->
            <Row title="Updates" description={updateWords(update.progress)}>
              {#snippet control()}
                {#if update.progress.kind === "available" || update.progress.kind === "ready"}
                  <Button
                    label={update.progress.kind === "ready" ? "Restart now" : "Update and restart"}
                    busy={false}
                    onclick={() => void applyUpdate()}
                  />
                {:else}
                  <Button
                    label="Check now"
                    busy={update.progress.kind === "downloading"}
                    onclick={() => void checkForUpdate(true)}
                  />
                {/if}
              {/snippet}
            </Row>
            <Row title="Licence" description="Sill's own code, including the extension host.">
              {#snippet control()}
                <span class="fact">MIT</span>
              {/snippet}
            </Row>
            <Row
              title="Indexed entries"
              description="Applications, commands, settings pages and executables."
            >
              {#snippet control()}
                <span class="fact">{info?.indexedCommands.toLocaleString() ?? "—"}</span>
              {/snippet}
            </Row>
          </Section>

          <Section label="Built on">
            <!-- not a setting: a credit, and the words mean other things in a search -->
            <Row
              title="Tauri and Rust"
              description="The window, the Windows integration and the index."
            />
            <!-- not a setting: a credit, and the word means other things in a search -->
            <Row
              title="Svelte"
              description="Everything drawn on screen, including a command's own views."
            />
            <!-- not a setting: a credit, and the word means other things in a search -->
            <Row
              title="Node"
              description="The extension host, which runs each command in its own worker."
            />
            <!-- not a setting: a credit, and the word means other things in a search -->
            <Row
              title="Everything"
              description="File search, by voidtools. Optional, and talked to over IPC."
            />
          </Section>
        {/if}
          </div>
          {/key}
        {/if}
      </div>
    </main>
  </div>
</div>

<style>
  .swatches {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }

  .swatch {
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    box-shadow: var(--ring-shade);
    cursor: default;
  }

  .swatch.on {
    box-shadow: var(--ring-shade), var(--focus-ring-gapped);
  }

  /* The browsers this pane is about, shown rather than described. */
  .browsers {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
    padding: 0 var(--space-3) var(--space-3);
  }

  .browser {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .window {
    display: flex;
    flex-direction: column;
    height: 100vh;
    /* Mixed toward the base colour rather than toward transparency. Glass
       strength still sets the tone, but the surface stays opaque, which is
       what keeps subpixel text rendering switched on. See theme.css. */
    background-color: color-mix(
      in srgb,
      var(--core-secondary-background) calc((1 - var(--glass-strength)) * 100%),
      var(--surface-base)
    );
    background-image: var(--chroma), linear-gradient(var(--tint), var(--tint));
    border-radius: var(--radius-window);
    box-shadow: var(--bevel-window);
    overflow: hidden;
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  /* Both columns start on the same line, `--space-5` below the title bar.
     They sat at 2px and 4px, which read as the content having been shoved up
     against the chrome rather than placed under it. */
  aside {
    display: flex;
    flex-direction: column;
    width: 244px;
    flex: none;
    padding: var(--space-5) 0 var(--space-2);
    border-right: 1px solid var(--hairline);
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0 var(--space-3) var(--space-3);
    padding: 0 var(--space-2);
    height: var(--control-height);
    border-radius: var(--radius-sm);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-3);
    transition:
      background-color var(--motion-state) var(--ease),
      box-shadow var(--motion-state) var(--ease);
  }

  /* Focus is one of the four things the accent is for, and the only one that
     applies to a text field. */
  .search:focus-within {
    background: var(--fill-2);
    box-shadow: var(--ring-accent-faint);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    outline: none;
    user-select: text;
  }

  .search input::placeholder {
    color: var(--text-3);
  }

  .clear {
    display: grid;
    place-items: center;
    width: var(--icon-tile-xs);
    height: var(--icon-tile-xs);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }

  .clear:hover {
    color: var(--text-1);
  }

  /* The one over the search results. Same treatment as a group heading in
     the nav below, so the sidebar reads as one system. */
  .group,
  .nav-label {
    padding: 0 var(--space-2) var(--space-2);
    font-size: var(--text-label);
    font-weight: var(--weight-strong);
    letter-spacing: var(--track-label);
    text-transform: uppercase;
    color: var(--text-3);
  }

  /* Space above a heading, none above the first: the gap is what makes a run
     of items read as a group, and a gap at the top of the list is just a
     hole. */
  .nav-label {
    margin-top: var(--space-5);
  }

  nav {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--space-2);
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    padding: var(--space-1) var(--space-2);
    margin-bottom: var(--space-half);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .nav-item:hover {
    background-color: var(--fill-1);
    color: var(--text-1);
  }

  /* Which panel is open is a selection, so it takes the accent. Hover above
     stays neutral, which is what keeps the two states distinguishable. */
  .nav-item.selected {
    background-color: var(--accent-fill);
    color: var(--text-1);
  }

  .result {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-1) var(--space-2);
    margin-bottom: var(--space-half);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-1);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background-color var(--motion-state) var(--ease);
  }

  .result:hover {
    background-color: var(--fill-1);
  }

  .result-tile {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    box-shadow: var(--bevel-tile);
    color: var(--text-1);
  }

  .result-text {
    display: flex;
    flex-direction: column;
    gap: var(--space-hair);
    min-width: 0;
  }

  .result-title {
    font-size: var(--text-meta);
    font-weight: var(--weight-medium);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .result-panel {
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  .no-results {
    margin: var(--space-1) var(--space-2);
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  header {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    flex: none;
    padding: var(--space-5) var(--space-8) var(--space-6);
  }

  .hero-text {
    min-width: 0;
  }

  /* The loudest thing in the window, and it should be. It names what is on
     screen; the section labels beneath it are structure and stay quiet. */
  h2 {
    margin: 0;
    font-size: var(--text-title);
    font-weight: var(--weight-strong);
    letter-spacing: var(--track-title);
    line-height: 1.2;
    /* The row it shares with the status and a button is one line tall. */
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  header p {
    margin: var(--space-1) 0 0;
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  /*
   * A measured number, in the column the buttons are in.
   *
   * Tabular figures, so two readings under each other line up on the decimal
   * rather than wandering by the width of a 1.
   */
  .reading {
    color: var(--text-2);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .status {
    margin-left: auto;
    flex: none;
    /* One line, like `.reading` above it. "Could not save: <error>" used to
       wrap and grow the whole header. */
    white-space: nowrap;
    font-size: var(--text-meta);
    color: var(--accent);
  }

  /*
   * What is not working, said once and quietly.
   *
   * A tinted band with a single hairline down its leading edge, which is the
   * thinnest thing that separates it from the panel below without becoming a
   * boxed-in notice. No border on the other three sides and no shadow: this
   * has to be readable without being the loudest thing in the window, because
   * most of the time it is not there at all and the times it is, it is the
   * answer to a question somebody already has.
   */
  .wrong {
    margin: 0 var(--space-8) var(--space-5);
    padding: var(--space-3) var(--space-4);
    border-left: 1px solid var(--danger);
    background: var(--danger-fill);
    border-radius: var(--radius-sm);
    display: grid;
    gap: var(--space-2);
  }

  .wrong p {
    margin: 0;
    font-size: var(--text-meta);
    color: var(--text-1);
    line-height: 1.45;
  }

  /* A link rather than a button shape. The house keeps bordered chips out of
     the settings window, and this is one word inside a sentence. */
  .goto {
    border: none;
    background: none;
    padding: 0;
    margin-left: var(--space-1);
    font: inherit;
    color: var(--accent);
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  /* The outgoing and incoming panels overlap for the length of the exit, so
     they are laid on top of each other rather than stacked vertically. Without
     this the pane doubles in height for 100ms and the scrollbar flickers. */
  .content {
    display: grid;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    /* `overflow-y: auto` alone computes overflow-x to auto as well, and a
       control wider than the panel then puts a horizontal scrollbar under a
       vertical one. Controls wrap instead; see Segmented. */
    overflow-x: hidden;
    padding: 0 var(--space-8) var(--space-8);
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  .panel-body {
    grid-area: 1 / 1;
    min-width: 0;
  }


  .fact {
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(116px, 1fr));
    gap: var(--space-2);
  }

  .stat {
    padding: var(--space-3) var(--space-3);
    border-radius: var(--radius-lg);
    background: var(--fill-0);
  }

  .stat-value {
    display: block;
    font-size: var(--text-title);
    font-weight: var(--weight-strong);
    font-variant-numeric: tabular-nums;
  }

  .stat-label {
    display: block;
    margin-top: var(--space-half);
    /* Sentence case, so the meta step: `--text-label` is the tracked-caps
       size and reads as an undersized line without the tracking. */
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  .about {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin-bottom: var(--space-8);
  }

  .about h3 {
    margin: 0;
    font-size: var(--text-query);
    font-weight: var(--weight-strong);
  }

  .about p {
    margin: var(--space-1) 0 0;
    font-size: var(--text-body);
    color: var(--text-2);
  }
</style>
