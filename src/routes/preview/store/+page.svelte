<script lang="ts">
  /**
   * The extension store, on a page, with no Rust behind it.
   *
   * A development route in the spirit of `/preview/gallery`: reachable with
   * `npm run dev`, never linked from the app, and of no use to anyone running
   * Sill rather than working on it.
   *
   * ## Why this exists
   *
   * The store is the one surface that cannot be judged from a screenshot of
   * the launcher, because reaching it needs a catalogue, a network and a
   * machine with Node on it. It also cannot be judged from a running dev build:
   * a session cannot click Sill's window, and the extension list is somebody
   * else's service that may be slow or down on the day you want to look at a
   * layout.
   *
   * So the real `StoreView` is rendered here against a fake backend. **The
   * component is not modified and knows nothing about this.** What is faked is
   * `window.__TAURI_INTERNALS__.invoke`, which is the single function every
   * `invoke` in the application goes through, so anything drawn here is drawn
   * by exactly the code that draws it in the launcher.
   *
   * ## What the fixture is for
   *
   * Not a demo. Each row is a case that is awkward to reach on purpose: one
   * installed and current, one installed and behind, one that says nothing
   * about Windows, one whose only command is a menu bar item, one with no
   * description, and one with a title long enough to need truncating.
   */
  import "$lib/theme/theme.css";
  import StoreView from "$lib/components/StoreView.svelte";
  import type { Preferences } from "$lib/settings";
  import type { Browse, Preparation, StoreRow, StoreQuery } from "$lib/store";

  const WALLS = {
    dark: "radial-gradient(120% 90% at 20% 10%, #23262b, #0a0a0b 70%)",
    mid: "radial-gradient(120% 90% at 30% 20%, #4a5560, #1d2228 75%)",
    light: "radial-gradient(120% 90% at 25% 15%, #e8e4dc, #b9b2a6 75%)",
  } as const;

  const THEMES = ["winters-glass", "oilslick", "graphite", "ember", "moss", "aberration"] as const;

  let wall = $state<keyof typeof WALLS>("dark");
  let theme = $state<(typeof THEMES)[number]>("winters-glass");
  let face = $state("satoshi");

  $effect(() => {
    document.documentElement.setAttribute("data-font", face);
    document.documentElement.setAttribute("data-theme", theme);
  });

  // ------------------------------------------------------------- the fixture

  function row(over: Partial<StoreRow>): StoreRow {
    return {
      name: "demo",
      folder: "extensions/demo",
      title: "Demo",
      description: "",
      author: "someone",
      categories: ["Productivity"],
      platforms: ["macOS", "Windows"],
      downloads: 1000,
      icon: "",
      revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      commands: [
        { name: "run", title: "Run", description: "Does the thing", mode: "view", runnable: true },
      ],
      installed: null,
      blocked: null,
      sourceUrl: "https://github.com/raycast/extensions/tree/aaaaaaa/extensions/demo",
      ...over,
    };
  }

  const CATALOGUE: StoreRow[] = [
    row({
      name: "linear",
      folder: "extensions/linear",
      title: "Linear",
      description:
        "Bring Linear to every corner of your desktop. Create, search, and modify your issues.",
      author: "thomaslombart",
      icon: "https://files.raycast.com/l8syncnql5n6vwoxo019qsnvuwkf",
      categories: ["Developer Tools", "Productivity", "AI Extensions"],
      downloads: 356834,
      commands: [
        { name: "create-issue", title: "Create Issue", description: "Create and assign new issues.", mode: "view", runnable: true },
        { name: "search-issues", title: "Search Issues", description: "Search issues globally.", mode: "view", runnable: true },
        { name: "my-issues", title: "My Issues", description: "Issues assigned to you.", mode: "view", runnable: true },
      ],
    }),
    // Installed and current: the row that must not offer to do anything.
    row({
      name: "uuid-generator",
      folder: "extensions/uuid-generator",
      title: "UUID Generator",
      description: "A quick way to generate UUIDs without opening the browser",
      author: "jmaeso",
      icon: "https://files.raycast.com/kk4xwj4wh7m4sko2t1ui0cdb21p0",
      categories: [],
      downloads: 31545,
      revision: "6939fc298cd701b66a652b5bcc6d1c763252391e",
      installed: { revision: "6939fc298cd701b66a652b5bcc6d1c763252391e", source: "store", outdated: false },
      commands: [
        { name: "generate", title: "Generate UUIDs", description: "Copy generated UUIDs to the clipboard", mode: "no-view", runnable: true },
        { name: "viewHistory", title: "View History", description: "View the UUID history", mode: "view", runnable: true },
      ],
    }),
    // Installed and behind: the whole point of pinning a revision.
    row({
      name: "spotify-player",
      folder: "extensions/spotify-player",
      title: "Spotify Player",
      description: "Control Spotify: search, play, queue, and see what is playing.",
      author: "mattisssa",
      icon: "https://files.raycast.com/4iollyf69hyyzvxfzz8zpqfqncp0",
      categories: ["Media", "Productivity"],
      downloads: 1240000,
      revision: "a0fbca34f41fb77a122db71c76ff48d539aa8d42",
      installed: { revision: "1c0d9a7b3e5f2a4c6d8e0b1f3a5c7e9d0b2f4a6c", source: "store", outdated: true },
    }),
    // Installed from a folder: somebody's own working copy, never out of date.
    row({
      name: "my-thing",
      folder: "extensions/my-thing",
      title: "My Thing",
      description: "Built here rather than fetched, so nothing knows what it says now.",
      author: "winters27",
      categories: ["Developer Tools"],
      downloads: 0,
      installed: { revision: "", source: "folder", outdated: false },
    }),
    // Says nothing about Windows: kept, marked, hidden by the switch.
    row({
      name: "quiet",
      folder: "extensions/quiet",
      title: "Published Before Platforms Existed",
      description: "Ordinary JavaScript that predates the field rather than refusing it.",
      author: "olddev",
      categories: ["Web"],
      platforms: [],
      downloads: 4200,
      blocked: "Does not say it runs on Windows",
    }),
    // A menu bar item and nothing else: installs and contributes nothing.
    row({
      name: "clock-bar",
      folder: "extensions/clock-bar",
      title: "Clock Bar",
      description: "Sits next to the system clock.",
      author: "barperson",
      categories: ["System"],
      downloads: 900,
      blocked: "Only has menu bar commands, which Sill has nowhere to put",
      commands: [
        { name: "bar", title: "Show Clock", description: "", mode: "menu-bar", runnable: false },
      ],
    }),
    // No description at all, and a title that has to truncate.
    row({
      name: "no-words",
      folder: "extensions/no-words",
      title: "An Extension Whose Author Gave It A Title Far Longer Than The Column",
      author: "terse",
      categories: ["Other"],
      downloads: 12,
    }),
    row({
      name: "brew",
      folder: "extensions/brew",
      title: "Brew",
      description: "Search and install formulae and casks.",
      author: "nhojb",
      categories: ["Developer Tools", "System"],
      downloads: 88000,
    }),
    row({
      name: "translate",
      folder: "extensions/google-translate",
      title: "Google Translate",
      description: "Translate text between languages without opening a tab.",
      author: "gebeto",
      categories: ["Web", "Productivity"],
      downloads: 210000,
    }),
  ];

  /**
   * The decision screen's fixture.
   *
   * Deliberately an extension that reaches a lot, because the screen is only
   * worth looking at when it has something uncomfortable to say. `mediated`
   * varies, which is the distinction the whole screen turns on.
   */
  const PREPARED: Preparation = {
    name: "spotify-player",
    title: "Spotify Player",
    revision: "a0fbca34f41fb77a122db71c76ff48d539aa8d42",
    folder: "extensions/spotify-player",
    icon: "https://files.raycast.com/4iollyf69hyyzvxfzz8zpqfqncp0",
    sourceUrl:
      "https://github.com/raycast/extensions/tree/a0fbca34f41fb77a122db71c76ff48d539aa8d42/extensions/spotify-player",
    files: 148,
    bytes: 1_240_000,
    commands: [
      { name: "search", title: "Search", description: "Search Spotify", mode: "view", runnable: true },
      { name: "now-playing", title: "Now Playing", description: "What is playing", mode: "view", runnable: true },
      { name: "menu-bar", title: "Menu Bar Player", description: "", mode: "menu-bar", runnable: false },
    ],
    capabilities: [
      {
        id: "network",
        title: "Reach the internet",
        detail: "Sends and receives whatever it likes, to wherever it likes.",
        seenIn: ["src/api/spotify.ts", "src/hooks/useSearch.ts", "src/oauth.ts"],
        mediated: false,
      },
      {
        id: "clipboard",
        title: "Read and change the clipboard",
        detail: "Including pasting into whatever window you were in.",
        seenIn: ["src/components/TrackActions.tsx"],
        mediated: true,
      },
      {
        id: "secrets",
        title: "Read its own settings",
        detail: "Anything you type into it, including passwords and API keys.",
        seenIn: ["src/preferences.ts"],
        mediated: false,
      },
      {
        id: "oauth",
        title: "Sign you in to another service",
        detail: "Opens a browser to authorise it, and keeps the token.",
        seenIn: ["src/oauth.ts"],
        mediated: false,
      },
      {
        id: "open",
        title: "Open files and links",
        detail: "Hands a path or an address to whatever program handles it.",
        seenIn: ["src/components/TrackActions.tsx"],
        mediated: true,
      },
    ],
    packages: ["@spotify/web-api-ts-sdk", "node-fetch", "swr"],
    secrets: ["Client Secret"],
    notEnforced:
      "Sill does not sandbox extensions. This is what the code appears to use, not a limit on " +
      "what it can do: an extension runs as a Node program with your account's access, and a " +
      "dependency it installs can do anything it does. Install what you would run.",
  };

  // ------------------------------------------------------- the fake backend

  /**
   * The same filtering Rust does, in the smallest form that exercises the UI.
   *
   * Not a second implementation of `store::browse`: it is a stand-in whose job
   * is to make the chips, the scopes and the switch move so the layout can be
   * judged. The real ordering, matching and capping live in Rust and are
   * tested there.
   */
  function fakeBrowse(query: StoreQuery): Browse {
    const text = query.text.trim().toLowerCase();

    const categories = new Map<string, number>();
    for (const one of CATALOGUE) {
      for (const category of one.categories) {
        categories.set(category, (categories.get(category) ?? 0) + 1);
      }
    }

    let hidden = 0;
    const rows = CATALOGUE.filter((one) => {
      if (one.blocked) {
        hidden += 1;
        if (query.hideBlocked) return false;
      }
      if (query.installedOnly && !one.installed) return false;
      if (query.updatesOnly && !one.installed?.outdated) return false;
      if (query.category && !one.categories.includes(query.category)) return false;
      if (!text) return true;

      return [one.title, one.name, one.author, one.description]
        .join(" ")
        .toLowerCase()
        .includes(text);
    }).sort((a, b) => b.downloads - a.downloads);

    return {
      rows,
      categories: [...categories]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([name, count]) => ({ name, count })),
      matched: rows.length,
      total: CATALOGUE.length,
      hidden,
      updates: CATALOGUE.filter((one) => one.installed?.outdated).length,
      fetchedAt: Date.now() / 1000 - 40 * 60,
    };
  }

  /**
   * Stands in for the whole Rust side.
   *
   * `window.__TAURI_INTERNALS__.invoke` is the one function every `invoke` in
   * the application reaches, so replacing it is enough to run any component
   * against fixtures without the component knowing. Guarded on `window`
   * because SvelteKit still evaluates this module before the browser has one
   * in some builds, even with `ssr = false`.
   */
  if (typeof window !== "undefined") {
    const answers: Record<string, (args: Record<string, unknown>) => unknown> = {
      store_browse: (args) => fakeBrowse(args.query as StoreQuery),
      store_ready: () => nodeInstalled,
      store_prepare: () => PREPARED,
      store_install: () => ({
        extension: PREPARED.name,
        title: PREPARED.title,
        commands: ["Search", "Now Playing"],
        revision: PREPARED.revision,
      }),
      store_discard: () => null,
      store_uninstall: () => true,
    };

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).__TAURI_INTERNALS__ = {
      // A round trip so the loading state is real rather than instantaneous.
      invoke: (cmd: string, args: Record<string, unknown>) =>
        new Promise((resolve, reject) => {
          const answer = answers[cmd];
          setTimeout(
            () => (answer ? resolve(answer(args ?? {})) : reject(`no such command: ${cmd}`)),
            latency,
          );
        }),
      transformCallback: (callback: unknown) => callback,
    };
  }

  // ------------------------------------------------------------- the harness

  let latency = $state(120);
  let nodeInstalled = $state(true);
  let windowsOnly = $state(true);
  let query = $state("");
  let selected = $state(0);
  let count = $state(0);
  let status = $state("");
  let view = $state<ReturnType<typeof StoreView> | null>(null);

  /* The one preference the store reads. Rust owns it in the real thing. */
  const prefs = $derived({ store: { windowsOnly, githubToken: null } } as unknown as Preferences);

  function onKey(event: KeyboardEvent) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      selected = count ? (selected + 1) % count : 0;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      selected = count ? (selected - 1 + count) % count : 0;
    } else if (event.key === "Enter") {
      event.preventDefault();
      void view?.activate();
    } else if (event.key === "Escape") {
      event.preventDefault();
      if (!view?.back()) status = "Escape here would return to the root list";
    }
  }
</script>

<svelte:head><title>Sill store preview</title></svelte:head>

<div class="page sill-scrolls" style:background={WALLS[wall]}>
  <div class="controls">
    <label>
      Wallpaper
      <select bind:value={wall}>
        {#each Object.keys(WALLS) as name (name)}<option value={name}>{name}</option>{/each}
      </select>
    </label>
    <label>
      Theme
      <select bind:value={theme}>
        {#each THEMES as name (name)}<option value={name}>{name}</option>{/each}
      </select>
    </label>
    <label>
      Face
      <select bind:value={face}>
        <option value="satoshi">satoshi</option>
        <option value="inter">inter</option>
        <option value="system">system</option>
      </select>
    </label>
    <label>
      Latency
      <select bind:value={latency}>
        <option value={0}>0 ms</option>
        <option value={120}>120 ms</option>
        <option value={1200}>1200 ms</option>
      </select>
    </label>
    <label><input type="checkbox" bind:checked={windowsOnly} /> Only Windows</label>
    <label><input type="checkbox" bind:checked={nodeInstalled} /> Node installed</label>
    <span class="note">Arrows move, Enter installs, Escape backs out</span>
  </div>

  <!--
    The launcher window, at the width Rust gives it, with the store's own
    breadcrumb and the launcher's field above it. The field is the store's
    search box in the real thing, which is the part that would be misleading to
    leave out.
  -->
  <div class="launcher">
    <div class="l-search">
      <img src="/sill.png" alt="" width="26" height="26" />
      <span class="l-crumb">Extension Store</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        bind:value={query}
        onkeydown={onKey}
        placeholder="Search the extension store…"
        spellcheck="false"
        autofocus
      />
    </div>
    <div class="l-divider"></div>

    <div class="l-body">
      <StoreView
        bind:this={view}
        {query}
        {selected}
        {prefs}
        onselect={(i) => (selected = i)}
        oncount={(n) => (count = n)}
        onstatus={(said) => (status = said)}
        onchanged={() => {}}
      />
    </div>

    <div class="l-chin">
      <span>{status}</span>
      <span class="spacer"></span>
      <span>{count} shown</span>
    </div>
  </div>

  <div class="foot">Development route. Not part of the app, and no Rust behind it.</div>
</div>

<style>
  .page {
    min-height: 100vh;
    padding: var(--space-6);
    color: var(--text-1);
    font-family: var(--font);
    overflow-y: auto;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
    margin-bottom: var(--space-5);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-lg);
    background: rgba(0, 0, 0, 0.72);
    backdrop-filter: blur(10px);
    font-size: var(--text-meta);
  }

  .controls label {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .controls select {
    background: #1b1d20;
    color: #fff;
    border: 1px solid rgba(255, 255, 255, 0.16);
    border-radius: var(--radius-sm);
    font-size: var(--text-meta);
  }

  /* Windows draws an open picker list in a window of its own and starts it
     white however the page is painted, which is the rule `verify:source`
     enforces in settings. This route is not settings and still needs it. */
  .controls select option {
    background: #1b1d20;
    color: #fff;
  }

  .note {
    color: rgba(255, 255, 255, 0.5);
  }

  .launcher {
    display: flex;
    flex-direction: column;
    width: 750px;
    max-width: 100%;
    height: 520px;
    background-color: color-mix(
      in srgb,
      var(--core-secondary-background) calc((1 - var(--glass-strength)) * 100%),
      transparent
    );
    background-image: var(--chroma), linear-gradient(var(--tint), var(--tint));
    border-radius: var(--radius-window);
    box-shadow: var(--bevel-window), 0 24px 60px -20px rgba(0, 0, 0, 0.7);
    overflow: hidden;
  }

  .l-search {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--space-3);
    height: var(--search-height);
    padding-left: var(--space-4);
  }

  .l-crumb {
    flex: none;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    background-image: var(--sheen);
    box-shadow: var(--bevel-tile);
    color: var(--text-2);
    font-size: var(--text-meta);
    white-space: nowrap;
  }

  .l-search input {
    flex: 1;
    min-width: 0;
    padding: 0 var(--space-3) 0 0;
    border: 0;
    background: transparent;
    color: var(--text-1);
    font-family: var(--font-display);
    font-size: var(--text-query);
    letter-spacing: -0.01em;
    outline: none;
  }

  .l-search input::placeholder {
    color: var(--text-3);
  }

  .l-divider {
    flex: none;
    height: 1px;
    background: var(--hairline);
  }

  .l-body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .l-chin {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--space-2);
    height: var(--chin-height);
    padding: 0 var(--space-3);
    border-top: 1px solid var(--hairline);
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .spacer {
    flex: 1;
  }

  .foot {
    margin-top: var(--space-5);
    color: rgba(255, 255, 255, 0.4);
    font-size: var(--text-meta);
  }
</style>
