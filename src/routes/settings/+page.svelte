<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import SettingsIcon, { type IconName } from "$lib/components/SettingsIcon.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import Section from "$lib/components/settings/Section.svelte";
  import Row from "$lib/components/settings/Row.svelte";
  import Segmented from "$lib/components/settings/Segmented.svelte";
  import Slider from "$lib/components/settings/Slider.svelte";
  import PathList from "$lib/components/settings/PathList.svelte";
  import TermList from "$lib/components/settings/TermList.svelte";
  import Button from "$lib/components/settings/Button.svelte";
  import DictationPanel from "$lib/components/settings/DictationPanel.svelte";
  import ClipboardPanel from "$lib/components/settings/ClipboardPanel.svelte";
  import SnippetsPanel from "$lib/components/settings/SnippetsPanel.svelte";
  import QuicklinksPanel from "$lib/components/settings/QuicklinksPanel.svelte";
  import ShortcutsPanel from "$lib/components/settings/ShortcutsPanel.svelte";
  import {
    acceleratorFrom,
    applyAppearance,
    clearUsageHistory,
    getDiagnostics,
    getPreferences,
    openDataFolder,
    openLog,
    rebuildIndex,
    listOwnSettings,
    setPreferences,
    type Backdrop,
    type SettingEntry,
    type InterfaceFont,
    type Diagnostics,
    type Preferences,
  } from "$lib/settings";
  import "$lib/theme/theme.css";

  type PanelId = IconName;

  interface Panel {
    id: PanelId;
    name: string;
    /** The one line under the panel title. */
    blurb: string;
  }

  const PANELS: Panel[] = [
    {
      id: "general",
      name: "General",
      blurb: "Startup, the summon hotkey and what Sill does when it opens",
    },
    {
      id: "appearance",
      name: "Appearance",
      blurb: "Window size, backdrop material and how deep the glass sits",
    },
    {
      id: "dictation",
      name: "Dictation",
      blurb: "The trigger, where the transcript goes, and which engine hears it",
    },
    {
      id: "snippets",
      name: "Snippets",
      blurb: "Saved text, expanded by keyword or pasted from the launcher",
    },
    {
      id: "shortcuts",
      name: "Shortcuts",
      blurb: "Keys that act on the selected text without opening the launcher",
    },
    {
      id: "quicklinks",
      name: "Quicklinks",
      blurb: "Saved addresses that take what you type and go straight there",
    },
    {
      id: "clipboard",
      name: "Clipboard History",
      blurb: "What is kept from everything you copy, and for how long",
    },
    {
      id: "sources",
      name: "Sources",
      blurb: "Where Sill looks for applications, commands and settings pages",
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
      id: "advanced",
      name: "Advanced",
      blurb: "The index, usage history and where Sill keeps its data",
    },
    {
      id: "about",
      name: "About",
      blurb: "Version, licence and what Sill is built on",
    },
  ];

  type SourceKey = Exclude<keyof Preferences["sources"], "excluded">;

  const SOURCES: [SourceKey, string, string][] = [
    [
      "shortcuts",
      "Start Menu, Desktop and taskbar",
      "Shortcuts from every folder Windows itself lists, including pinned items",
    ],
    [
      "packagedApps",
      "Store and packaged applications",
      "Calculator, Terminal, Photos and anything installed from the Microsoft Store",
    ],
    [
      "appPaths",
      "Registered executables",
      "Programs an installer registered by name, resolved the way the Run dialog does",
    ],
    [
      "installedPrograms",
      "Installed programs",
      "Read from the uninstall registry, filtered to the entries that can actually launch",
    ],
    [
      "pathExecutables",
      "Executables on PATH",
      "Around 1,200 command line tools. Always ranked below real applications",
    ],
    [
      "windowsSettings",
      "Windows settings pages",
      "Settings pages, Control Panel applets and management consoles",
    ],
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
  let active = $state<PanelId>("general");
  let status = $state("");
  let recording = $state(false);
  let filter = $state("");
  let clearing = $state(false);
  let rebuilding = $state(false);

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
   */
  async function commit() {
    if (!prefs) return;
    try {
      const next = $state.snapshot(prefs);
      await setPreferences(next);
      applyAppearance(next);
      status = "Saved";
      setTimeout(() => (status = ""), 1200);
    } catch (err) {
      status = `Could not save: ${err}`;
    }
  }

  function onRecord(event: KeyboardEvent) {
    if (!recording || !prefs) return;
    event.preventDefault();

    if (event.key === "Escape") {
      recording = false;
      return;
    }

    const accelerator = acceleratorFrom(event);
    if (!accelerator) return;

    prefs.hotkey.summon = accelerator;
    recording = false;
    void commit();
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

  /** Only jump if the name is real, so a stale link cannot blank the page. */
  function jumpTo(name: string | null) {
    if (name && PANELS.some((p) => p.id === name)) {
      active = name as PanelId;
      filter = "";
    }
  }

  onMount(() => {
    let unlisten: UnlistenFn | undefined;

    (async () => {
      // A deep link opens straight at its panel: "About Sill" landing on
      // whatever was last shown would not be an About link at all.
      jumpTo(new URLSearchParams(window.location.search).get("section"));

      // The same link arriving while settings is already open.
      unlisten = await listen<string>("sill://settings-section", ({ payload }) =>
        jumpTo(payload),
      );

      try {
        prefs = await getPreferences();
        applyAppearance(prefs);
        index = await listOwnSettings();
        info = await getDiagnostics();
      } catch (err) {
        status = `Could not load settings: ${err}`;
      }
    })();

    return () => unlisten?.();
  });
</script>

<svelte:window onkeydown={onRecord} />

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
        <input bind:value={filter} placeholder="Search settings" spellcheck="false" />
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
        <div class="group">Settings</div>
        <nav>
          {#each PANELS as item (item.id)}
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
      </header>

      <div class="content">
        {#if !prefs}
          <div class="loading">{status || "Loading…"}</div>
        {:else}
          {@const p = prefs}
          {#if active === "general"}
          <Section label="Startup">
            <Row
              title="Open at login"
              description="Sill starts with Windows and waits quietly for the hotkey."
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
              description="Sill has no taskbar button, so the tray icon is the only sign it is running. Left click summons it, right click opens a menu."
            >
              {#snippet control()}
                <Toggle
                  bind:checked={p.general.showInTray}
                  onchange={commit}
                  label="Show in tray"
                />
              {/snippet}
            </Row>
          </Section>

          <Section
            label="Hotkey"
            description="The combination that brings Sill to the front from anywhere."
          >
            <Row
              title="Summon hotkey"
              description="Press the combination you want, or Escape to keep the current one."
            >
              {#snippet control()}
                <button class="recorder" class:recording onclick={() => (recording = !recording)}>
                  {recording ? "Press a combination" : p.hotkey.summon.split("+").join(" ")}
                </button>
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
              description="Otherwise Sill reopens on whatever command was last running."
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
            label="Material"
            description="Windows composites the desktop behind the window. These decide how much of it shows through."
          >
            <Row
              title="Backdrop"
              description="Acrylic adds a luminosity layer of its own, so it always lightens a little. Blur lets the tint below decide the depth. None is the deepest."
            >
              {#snippet control()}
                <Segmented
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
          {:else if active === "dictation"}
            <DictationPanel prefs={p} {commit} />
          {:else if active === "snippets"}
            <SnippetsPanel prefs={p} {commit} />
          {:else if active === "shortcuts"}
            <ShortcutsPanel prefs={p} {commit} />
          {:else if active === "quicklinks"}
            <QuicklinksPanel />
          {:else if active === "clipboard"}
            <ClipboardPanel prefs={p} {commit} />
          {:else if active === "sources"}
          <Section
            label="What Sill indexes"
            description="Turning a source off removes its entries from the next search. Nothing is rescanned."
          >
            {#each SOURCES as [key, name, hint] (key)}
              <Row title={name} description={hint}>
                {#snippet control()}
                  <Toggle bind:checked={p.sources[key]} onchange={commit} label={name} />
                {/snippet}
              </Row>
            {/each}
          </Section>

          <Section
            label="Exclusions"
            description="Matched against both the name and the path, so one folder name can hide a whole vendor at once."
          >
            <Row title="Hidden entries">
              {#snippet children()}
                <TermList bind:terms={p.sources.excluded} onchange={commit} />
              {/snippet}
            </Row>
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
            label="Everything"
            description="File search is provided by voidtools Everything, which has to be installed and running. Sill talks to it over its own IPC, so nothing is spawned per keystroke."
          >
            <Row
              title="Search files"
              description={info?.everythingRunning
                ? "Everything is running and answering."
                : "Everything is not running, so file results will be empty."}
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
            label="Scope"
            description="With no folders listed Sill searches everywhere Everything indexes."
          >
            <Row title="Folders to search" disabled={!p.files.enabled}>
              {#snippet children()}
                <PathList bind:paths={p.files.onlyIn} onchange={commit} />
              {/snippet}
            </Row>
          </Section>
          {:else if active === "extensions"}
          <Section
            label="Installed"
            description="Raycast extensions running in Sill's host. Each contributes its commands to the root list."
            bare={!info?.extensions.length}
          >
            {#if info?.extensions.length}
              {#each info.extensions as extension (extension.id)}
                <Row
                  title={extension.title}
                  description="{extension.commands} {extension.commands === 1
                    ? 'command'
                    : 'commands'} · {extension.id}"
                />
              {/each}
            {:else}
              <p class="empty">
                No extensions are installed yet. Sill runs unmodified Raycast extensions through its
                own host, but installing them from a repository is not built.
              </p>
            {/if}
          </Section>
          {:else if active === "advanced"}
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
          </Section>
          {:else}
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
            <Row
              title="Tauri and Rust"
              description="The window, the Windows integration and the index."
            />
            <Row
              title="Svelte"
              description="Everything drawn on screen, including a command's own views."
            />
            <Row
              title="Node"
              description="The extension host, which runs each command in its own worker."
            />
            <Row
              title="Everything"
              description="File search, by voidtools. Optional, and talked to over IPC."
            />
          </Section>
        {/if}
        {/if}
      </div>
    </main>
  </div>
</div>

<style>
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
    background-image: linear-gradient(var(--tint), var(--tint));
    border-radius: var(--radius-window);
    box-shadow: var(--bevel-window);
    overflow: hidden;
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  aside {
    display: flex;
    flex-direction: column;
    width: 244px;
    flex: none;
    padding: 2px 0 10px;
    border-right: 1px solid var(--hairline);
  }

  .search {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 0 12px 14px;
    padding: 0 8px;
    height: 30px;
    border-radius: var(--radius-sm);
    background: rgba(var(--accent-rgb), 0.05);
    box-shadow: inset 0 0 0 1px var(--hairline);
    color: var(--text-faint);
    transition:
      background-color 0.15s var(--ease),
      box-shadow 0.15s var(--ease);
  }

  .search:focus-within {
    background: rgba(var(--accent-rgb), 0.08);
    box-shadow: inset 0 0 0 1px var(--border-light);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--core-foreground);
    font: inherit;
    font-size: 12.5px;
    outline: none;
    user-select: text;
  }

  .search input::placeholder {
    color: var(--text-faint);
  }

  .clear {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
  }

  .clear:hover {
    color: var(--core-foreground);
  }

  .group {
    padding: 0 16px 8px;
    font-size: 11px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--text-faint);
  }

  nav {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 8px;
    scrollbar-width: thin;
    scrollbar-color: rgba(var(--accent-rgb), 0.3) transparent;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 11px;
    width: 100%;
    padding: 6px 8px;
    margin-bottom: 2px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition:
      background-color 0.15s var(--ease),
      color 0.15s var(--ease);
  }

  .nav-item:hover {
    background-color: rgba(var(--accent-rgb), 0.05);
    color: var(--core-foreground);
  }

  .nav-item.selected {
    background-color: rgba(var(--accent-rgb), 0.11);
    color: var(--core-foreground);
  }

  .result {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 9px;
    margin-bottom: 2px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--core-foreground);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background-color 0.15s var(--ease);
  }

  .result:hover {
    background-color: rgba(var(--accent-rgb), 0.07);
  }

  .result-tile {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: 5px;
    background: rgba(var(--accent-rgb), 0.1);
    box-shadow: var(--bevel-tile);
    color: var(--core-foreground);
  }

  .result-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .result-title {
    font-size: 12.5px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .result-panel {
    font-size: 11px;
    color: var(--text-faint);
  }

  .no-results {
    margin: 4px 9px;
    font-size: 12px;
    color: var(--text-faint);
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
    gap: 13px;
    flex: none;
    padding: 2px 32px 20px;
  }


  .hero-text {
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    line-height: 1.2;
  }

  header p {
    margin: 3px 0 0;
    font-size: 12px;
    color: var(--text-muted);
  }

  .status {
    margin-left: auto;
    flex: none;
    font-size: 12px;
    color: var(--core-accent);
  }

  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 32px 32px;
    scrollbar-width: thin;
    scrollbar-color: rgba(var(--accent-rgb), 0.3) transparent;
  }

  .loading {
    padding: 28px 0;
    color: var(--text-faint);
  }

  .empty {
    margin: 0;
    max-width: 56ch;
    font-size: 13px;
    line-height: 1.7;
    color: var(--text-muted);
  }

  .recorder {
    min-width: 150px;
    padding: 6px 14px;
    border: 0;
    border-radius: var(--radius-sm);
    background: rgba(var(--accent-rgb), 0.1);
    box-shadow: var(--bevel-tile);
    color: var(--core-foreground);
    font-family: var(--font-mono);
    font-size: 12px;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition:
      background-color 0.15s var(--ease),
      color 0.15s var(--ease);
  }

  .recorder:hover {
    background: rgba(var(--accent-rgb), 0.18);
  }

  .recorder.recording {
    background: rgba(var(--accent-rgb), 0.22);
    color: var(--accent-bright);
  }

  .fact {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-muted);
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(116px, 1fr));
    gap: 8px;
  }

  .stat {
    padding: 11px 13px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.02);
  }

  .stat-value {
    display: block;
    font-size: 18px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .stat-label {
    display: block;
    margin-top: 2px;
    font-size: 11px;
    color: var(--text-faint);
  }

  .about {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 30px;
  }

  .about h3 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }

  .about p {
    margin: 4px 0 0;
    font-size: 13px;
    color: var(--text-muted);
  }
</style>
