<script lang="ts">
  /**
   * A development route. Reachable with `npm run dev`, never linked from the
   * app, and of no use to anyone running Sill rather than working on it.
   *
   * Every primitive in Sill, rendered side by side on one page, so the design
   * system can be judged as a system rather than by clicking through the app.
   * It imports the real components, so a token change shows up here first.
   *
   * Two things it deliberately does that the app cannot:
   *
   * 1. A wallpaper switcher. The launcher window is transparent, so every
   *    surface and every piece of text is an alpha over whatever is behind it.
   *    A dark desktop hides the one case that breaks: white-alpha text over a
   *    light wallpaper.
   * 2. A face switcher, because `data-font` changes the metrics of every row
   *    and Rust cannot see which face is active.
   */
  import "$lib/theme/theme.css";
  import LaunchIcon from "$lib/components/LaunchIcon.svelte";
  import SettingsIcon from "$lib/components/SettingsIcon.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import Segmented from "$lib/components/settings/Segmented.svelte";
  import Slider from "$lib/components/settings/Slider.svelte";
  import Button from "$lib/components/settings/Button.svelte";
  import Section from "$lib/components/settings/Section.svelte";
  import Row from "$lib/components/settings/Row.svelte";
  import ThemeCards from "$lib/components/settings/ThemeCards.svelte";
  import { popover, swap } from "$lib/motion";

  const WALLS = {
    dark: "radial-gradient(120% 90% at 20% 10%, #23262b, #0a0a0b 70%)",
    mid: "radial-gradient(120% 90% at 30% 20%, #4a5560, #1d2228 75%)",
    light: "radial-gradient(120% 90% at 25% 15%, #e8e4dc, #b9b2a6 75%)",
  } as const;

  const THEMES = ["winters-glass", "oilslick", "graphite", "ember", "moss", "aberration"] as const;

  let wall = $state<keyof typeof WALLS>("dark");
  let theme = $state<(typeof THEMES)[number]>("winters-glass");
  let face = $state("satoshi");
  let toggled = $state(true);
  let segment = $state("acrylic");
  let rows = $state(10);
  let menuOpen = $state(false);
  let swapKey = $state(0);

  $effect(() => {
    document.documentElement.setAttribute("data-font", face);
    document.documentElement.setAttribute("data-theme", theme);
  });

  /**
   * The root list, as it really arrives: most rows carry no subtitle.
   *
   * Long enough to scroll, so the list is exercised rather than posed.
   */
  const SAMPLE = [
    { title: "Visual Studio Code", sub: "", kind: "Application", letter: "V" },
    { title: "Clipboard History", sub: "Everything you have copied", kind: "Command", panel: "clipboard" },
    { title: "Sill Settings", sub: "Startup, hotkey and appearance", kind: "Command", panel: "general" },
    { title: "ffmpeg", sub: "", kind: "Command Line", letter: "F" },
    { title: "Windows Terminal", sub: "", kind: "Store App", letter: "W" },
    { title: "Docker Desktop", sub: "", kind: "Application", letter: "D" },
    { title: "Notion", sub: "", kind: "Application", letter: "N" },
    { title: "Sound Settings", sub: "System > Sound", kind: "System", panel: "general" },
    { title: "rustc", sub: "", kind: "Command Line", letter: "R" },
    { title: "Spotify", sub: "", kind: "Application", letter: "S" },
    { title: "Node.js documentation", sub: "", kind: "Documentation", letter: "N" },
    { title: "Steam", sub: "", kind: "Application", letter: "S" },
  ];
</script>

<svelte:head><title>Sill primitives</title></svelte:head>

<div class="page" style:background={WALLS[wall]}>
  <div class="controls">
    <strong>Sill primitives</strong>
    <span class="sp"></span>
    <label>Wallpaper
      <select bind:value={wall}>
        <option value="dark">Dark</option>
        <option value="mid">Mid</option>
        <option value="light">Light</option>
      </select>
    </label>
    <label>Theme
      <select bind:value={theme}>
        {#each THEMES as t (t)}<option value={t}>{t}</option>{/each}
      </select>
    </label>
    <label>Face
      <select bind:value={face}>
        <option value="satoshi">Satoshi</option>
        <option value="inter">Inter</option>
        <option value="system">Segoe</option>
      </select>
    </label>
  </div>

  <h2>Colour: neutral (everything that is a surface)</h2>
  <div class="swatches">
    {#each ["--grey-0", "--grey-1", "--grey-2", "--grey-3", "--tint", "--fill-1", "--fill-2", "--fill-3", "--hairline", "--hairline-strong", "--scrollbar-thumb"] as token (token)}
      <div class="swatch">
        <div class="chip" style:background="var({token})"></div>
        <code>{token}</code>
      </div>
    {/each}
  </div>

  <h2>Colour: accent (selection, match, focus, affirmative state only)</h2>
  <div class="swatches">
    {#each ["--core-accent", "--accent-fill", "--accent-fill-strong", "--accent-line", "--accent-bright"] as token (token)}
      <div class="swatch">
        <div class="chip" style:background="var({token})"></div>
        <code>{token}</code>
      </div>
    {/each}
  </div>

  <h2>Theme cards (the appearance picker; picking one restyles this page)</h2>
  <div class="themecards">
    <ThemeCards value={theme} onpick={(t) => (theme = t)} />
  </div>

  <h2>Text</h2>
  <div class="swatches">
    {#each ["--text-1", "--text-2", "--text-3", "--text-4"] as token (token)}
      <div class="swatch">
        <div class="chip type" style:color="var({token})">Agy 013</div>
        <code>{token}</code>
      </div>
    {/each}
  </div>

  <h2>Type scale</h2>
  <div class="launcher scale">
    {#each [["--text-hero", "40"], ["--text-display", "26"], ["--text-title", "20"], ["--text-query", "17"], ["--text-heading", "15"], ["--text-body", "13"], ["--text-meta", "12"], ["--text-label", "11"], ["--text-micro", "10"]] as [token, px] (token)}
      <div class="scale-row">
        <span class="scale-name">{token}</span>
        <span class="scale-sample" style:font-size="var({token})">Search apps, files and commands</span>
        <span class="scale-px">{px}</span>
      </div>
    {/each}
  </div>

  <h2>Launcher</h2>
  <div class="launcher">
    <div class="l-search">
      <img src="/sill.png" alt="" width="26" height="26" />
      <span class="l-crumb">Clipboard History</span>
      <input placeholder="Search for apps and commands…" spellcheck="false" />
    </div>
    <div class="l-divider"></div>
    <div class="sill-list l-body">
      <div class="sill-group">Applications</div>
      {#each SAMPLE as item, i (item.title)}
        <div class="sill-row" class:selected={i === 1}>
          {#if item.panel}
            <SettingsIcon name={item.panel as never} size={26} />
          {:else}
            <LaunchIcon path="" label={item.title} resolvable={false} />
          {/if}
          <span class="text">
            <span class="line"><span class="l-title">{item.title}</span></span>
            {#if item.sub}<span class="l-sub">{item.sub}</span>{/if}
          </span>
          <span class="l-spacer"></span>
          <span class="l-kind">{item.kind}</span>
          <span class="sill-key jump">Ctrl {i + 1}</span>
        </div>
      {/each}
    </div>
    <footer class="l-footer">
      <button class="l-context">
        <img src="/sill.png" alt="" width="18" height="18" />
        <svg width="9" height="9" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M2.5 7.5 6 4l3.5 3.5" stroke="currentColor" stroke-width="1.6"
            stroke-linecap="round" stroke-linejoin="round" fill="none" />
        </svg>
      </button>
      <span class="l-spacer"></span>
      <span class="l-escape">Close <span>Esc</span></span>
      <div class="l-pill">
        <button class="l-seg">Open <span class="sill-key">↵</span></button>
        <span class="l-split"></span>
        <button class="l-seg">Actions <span class="sill-key">Ctrl K</span></button>
      </div>
    </footer>
  </div>

  <h2>Row states</h2>
  <div class="launcher tight">
    <div class="sill-list">
      <div class="sill-group">Every state</div>
      <div class="sill-row">
        <LaunchIcon path="" label="Default" resolvable={false} />
        <span class="text"><span class="line"><span class="l-title">Default, no subtitle</span></span></span>
        <span class="l-spacer"></span><span class="l-kind">Application</span>
      </div>
      <div class="sill-row hoverish">
        <LaunchIcon path="" label="Hover" resolvable={false} />
        <span class="text"><span class="line"><span class="l-title">Hover</span></span></span>
        <span class="l-spacer"></span><span class="l-kind">Application</span>
      </div>
      <div class="sill-row selected">
        <LaunchIcon path="" label="Selected" resolvable={false} />
        <span class="text"><span class="line"><span class="l-title">Selected</span></span></span>
        <span class="l-spacer"></span><span class="l-kind">Application</span>
      </div>
      <div class="sill-row">
        <LaunchIcon path="" label="Subtitle" resolvable={false} />
        <span class="text">
          <span class="line"><span class="l-title">With a subtitle</span></span>
          <span class="l-sub">C:\Program Files\Example\Nested\Deeper\example.exe</span>
        </span>
        <span class="l-spacer"></span><span class="l-kind">Application</span>
      </div>
      <div class="sill-row">
        <LaunchIcon path="" label="Alias" resolvable={false} />
        <span class="text">
          <span class="line"><span class="l-title">With an alias</span><span class="l-alias">vsc</span></span>
        </span>
        <span class="l-spacer"></span><span class="l-kind">Application</span>
        <span class="sill-key jump">Ctrl 5</span>
      </div>
      <div class="sill-row">
        <span class="l-emoji">🙂</span>
        <span class="text"><span class="line"><span class="l-title">Emoji row</span></span></span>
        <span class="l-spacer"></span><span class="l-kind">Emoji</span>
      </div>
      <div class="sill-row answer">
        <span class="l-equals">=</span>
        <span class="text">
          <span class="line"><span class="l-title answer">21.1875 g</span></span>
          <span class="l-sub ltr">1 tbsp honey to g</span>
        </span>
        <span class="l-spacer"></span><span class="l-kind">Calculator</span>
      </div>
      <div class="sill-row">
        <LaunchIcon path="" label="Very Long" resolvable={false} />
        <span class="text">
          <span class="line"><span class="l-title">A title long enough that it has to give way before anything else on the row does</span></span>
          <span class="l-sub">C:\Users\Brandon\AppData\Local\Programs\Something\With\A\Long\Path\binary.exe</span>
        </span>
        <span class="l-spacer"></span><span class="l-kind">Command Line</span>
        <span class="sill-key jump">Ctrl 8</span>
      </div>
    </div>
  </div>

  <h2>Motion</h2>
  <div class="motion-demo">
    <button class="m-btn" onclick={() => (menuOpen = !menuOpen)}>
      {menuOpen ? "Close" : "Open"} popover
    </button>
    <button class="m-btn" onclick={() => (swapKey += 1)}>Swap panel</button>
    <div class="m-stage">
      {#if menuOpen}
        <div class="sill-menu m-pop"
             in:popover={{ origin: "bottom left" }}
             out:popover={{ origin: "bottom left", out: true }}>
          <div class="p-row selected"><span>Settings</span></div>
          <div class="p-row"><span>Appearance</span></div>
          <div class="p-row"><span>Extensions</span></div>
        </div>
      {/if}
    </div>
    <div class="m-stage">
      {#key swapKey}
        <div class="m-panel" in:swap out:swap={{ out: true }}>Panel {swapKey}</div>
      {/key}
    </div>
  </div>

  <h2>Popovers</h2>
  <div class="pops">
    <div class="sill-menu pop">
      <div class="p-section">Sections</div>
      <div class="p-row selected"><span>Copy to Clipboard</span><span class="l-spacer"></span><span class="l-keys">Ctrl C</span></div>
      <div class="p-row"><span>Open in Browser</span><span class="l-spacer"></span><span class="l-keys">Ctrl O</span></div>
      <div class="p-rule"></div>
      <div class="p-row danger"><span>Delete</span><span class="l-spacer"></span><span class="l-keys">Del</span></div>
    </div>

    <!-- The notification-area menu, at its real size. It is a window now
         rather than a native shell menu, so it takes the WINDOW recipe, not
         `.sill-menu`: there is no page content behind it for a backdrop
         filter to blur. -->
    <div class="pop tray">
      {#each [["Open Sill", "Alt Space", false, false], ["Clipboard History", "", false, false], ["Snippets", "", false, false], ["Dictate", "", false, false], ["Settings", "Ctrl ,", true, false], ["Quit Sill", "", true, true]] as [label, hint, breaks, danger], i (label)}
        {#if breaks}<div class="p-rule"></div>{/if}
        <div class="p-row tray-row" class:selected={i === 0} class:danger>
          <span class="tray-glyph"></span>
          <span>{label}</span>
          <span class="l-spacer"></span>
          {#if hint}<span class="sill-key">{hint}</span>{/if}
        </div>
      {/each}
    </div>
  </div>

  <h2>Settings</h2>
  <div class="settings">
    <aside>
      <div class="s-search">Search settings</div>
      <button class="s-nav selected"><SettingsIcon name="general" size={26} />General</button>
      <button class="s-nav"><SettingsIcon name="appearance" size={26} />Appearance</button>
      <div class="s-group">Workflow</div>
      <button class="s-nav"><SettingsIcon name="snippets" size={26} />Snippets</button>
      <button class="s-nav"><SettingsIcon name="clipboard" size={26} />Clipboard History</button>
      <div class="s-group">Search</div>
      <button class="s-nav"><SettingsIcon name="sources" size={26} />Sources</button>
      <button class="s-nav"><SettingsIcon name="files" size={26} />File Search</button>
      <div class="s-group">System</div>
      <button class="s-nav"><SettingsIcon name="advanced" size={26} />Advanced</button>
    </aside>
    <main>
      <header class="s-head">
        <SettingsIcon name="appearance" size={38} />
        <div>
          <h3>Appearance</h3>
          <p>Window size, backdrop material and how deep the glass sits</p>
        </div>
      </header>

      <Section label="Surface" description="How the launcher sits over what is behind it.">
        <Row title="Backdrop" description="Which material Windows composites behind the window.">
          {#snippet control()}
            <Segmented
              bind:value={segment}
              options={[
                { value: "acrylic", label: "Acrylic" },
                { value: "blur", label: "Blur" },
                { value: "none", label: "None" },
              ]}
              onchange={() => {}}
            />
          {/snippet}
        </Row>
        <Row title="Open at login" description="Sill starts with Windows and waits quietly for the hotkey.">
          {#snippet control()}
            <Toggle bind:checked={toggled} onchange={() => {}} label="Open at login" />
          {/snippet}
        </Row>
        <Row title="Visible rows" description="Rows shown before the list scrolls. This sets the window height.">
          {#snippet control()}
            <Slider bind:value={rows} min={4} max={16} label="Visible rows" format={(v) => `${v} rows`} onchange={() => {}} />
          {/snippet}
        </Row>
        <Row title="Index" description="Rebuild everything Sill knows how to find.">
          {#snippet control()}
            <div class="btns">
              <Button label="Reload" onclick={() => {}} />
              <Button label="Forget history" tone="danger" onclick={() => {}} />
            </div>
          {/snippet}
        </Row>
      </Section>
    </main>
  </div>

  <h2>Empty and loading states</h2>
  <div class="states">
    {#each [["Nothing found", "Try fewer letters, or a word from further along the name."], ["No results", "This command found nothing to show."], ["Starting the command", ""], ["This form has no fields", "The extension declared a form with nothing in it."]] as [head, hint] (head)}
      <div class="launcher tiny">
        <div class="sill-empty">
          <img src="/sill.png" alt="" width="32" height="32" />
          <span class="headline">{head}</span>
          {#if hint}<span class="hint">{hint}</span>{/if}
        </div>
      </div>
    {/each}
  </div>

  <div class="foot">Development route. Not part of the app.</div>
</div>

<style>
  .page {
    min-height: 100vh;
    padding: var(--space-6) var(--space-6) var(--space-10);
    color: var(--text-1);
    font-family: var(--font);
    overflow-y: auto;
  }

  .controls {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
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
    color: var(--text-2);
  }

  .controls select {
    background: #1c1c1f;
    color: #fff;
    border: 1px solid var(--hairline);
    border-radius: var(--radius-sm);
    padding: 2px var(--space-1);
    font: inherit;
  }

  .sp { flex: 1; }

  h2 {
    margin: var(--space-8) 0 var(--space-2);
    font-size: var(--text-label);
    font-weight: var(--weight-strong);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: rgba(255, 255, 255, 0.55);
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.8);
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  /* About what the settings content pane gives the picker at the default
     window size, so the cards are judged at the width they really get. */
  .themecards {
    max-width: 880px;
  }

  .swatch { width: 128px; }

  .chip {
    height: 40px;
    border-radius: var(--radius-md);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.14);
  }

  .chip.type {
    display: grid;
    place-items: center;
    background: var(--core-background);
    font-size: var(--text-query);
  }

  .swatch code {
    display: block;
    margin-top: var(--space-1);
    font-family: var(--font-mono);
    font-size: var(--text-micro);
    color: rgba(255, 255, 255, 0.6);
  }

  /* The launcher window, reproduced exactly: same tint, same bevel, same
     radius, so an alpha here reads the way it reads in the app. */
  .launcher {
    width: 750px;
    max-width: 100%;
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

  .launcher.tight :global(.sill-list) { padding: var(--space-1); }
  .launcher.tiny { width: 360px; }

  .l-search {
    display: flex;
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

  .l-search input::placeholder { color: var(--text-3); }

  .l-divider { height: 1px; background: var(--hairline); }

  .l-body { max-height: 240px; }

  .text {
    display: flex;
    flex-direction: column;
    justify-content: center;
    min-width: 0;
    flex: 1;
  }

  .line { display: flex; align-items: center; gap: var(--space-2); min-width: 0; }

  .l-title {
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: var(--weight-body);
    line-height: var(--line-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .l-title.answer {
    font-family: var(--font-display);
    font-size: var(--text-query);
    font-weight: var(--weight-medium);
    letter-spacing: var(--track-tight);
  }

  .l-sub {
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: var(--line-meta);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: rtl;
    text-align: left;
  }

  .l-sub.ltr { direction: ltr; }

  .l-alias {
    flex: none;
    padding: 1px var(--space-1);
    font-size: var(--text-micro);
    font-weight: var(--weight-medium);
    letter-spacing: var(--track-micro);
    color: var(--accent-bright);
    background: var(--fill-2);
    border-radius: var(--radius-sm);
  }

  .l-emoji, .l-equals {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    flex: none;
    line-height: 1;
  }

  .l-emoji { font-size: var(--glyph-md); }
  .l-equals { font-size: var(--glyph-sm); font-family: var(--font-display); color: var(--text-3); }

  .l-spacer { flex: none; width: var(--space-3); }

  .l-kind {
    flex: none;
    color: var(--text-3);
    font-size: var(--text-meta);
    white-space: nowrap;
  }

  .jump { margin-left: var(--space-2); }

  .hoverish { background-color: var(--fill-1); }

  .l-footer {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--chin-height);
    padding: 0 var(--space-2);
    background: var(--chin);
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  .l-context {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex: none;
    height: 30px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-lg);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    font-weight: var(--weight-medium);
    cursor: default;
  }

  .l-context:hover { background: var(--fill-2); color: var(--text-1); }
  .l-context svg { color: var(--text-4); }

  .l-escape { display: flex; align-items: center; gap: var(--space-1); color: var(--text-4); }
  .l-escape span { font-weight: var(--weight-medium); }

  .l-pill {
    display: flex;
    align-items: center;
    flex: none;
    height: 30px;
    border-radius: var(--radius-lg);
    background: var(--fill-2);
    box-shadow: var(--bevel-tile);
    overflow: hidden;
  }

  .l-seg {
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
  }

  .l-seg:hover { background-color: var(--fill-2); color: var(--text-1); }

  .l-split { width: 1px; height: 16px; flex: none; background: var(--hairline-strong); }

  .pops { display: flex; gap: var(--space-4); }

  .motion-demo { display: flex; align-items: flex-end; gap: var(--space-4); }

  .m-btn {
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--fill-2);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
  }

  .m-stage { position: relative; display: grid; width: 200px; height: 120px; }
  .m-pop { align-self: end; padding: var(--space-1); grid-area: 1 / 1; }

  .m-panel {
    grid-area: 1 / 1;
    align-self: end;
    padding: var(--space-4);
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    box-shadow: inset 0 0 0 1px var(--hairline);
    font-size: var(--text-body);
  }

  .pop { width: 320px; padding: var(--space-1); }

  .p-section {
    padding: var(--space-2) var(--space-2) var(--space-1);
    font-size: var(--text-group);
    font-weight: var(--weight-medium);
    color: var(--text-3);
  }

  .p-rule { height: 1px; margin: var(--space-1) var(--space-1); background: var(--hairline); }

  .p-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 32px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    font-size: var(--text-body);
    font-weight: var(--weight-body);
  }

  .p-row.selected { background-color: var(--accent-fill); color: var(--text-1); }
  .p-row.danger span:first-child { color: var(--accent-red); }

  /* The real window's size, so the mock proves the constant in lib.rs:
     6 rows at 30 + 2 separators at 9 + 4 of padding each side = 206, and no
     border, because a border would sit inside `height: 100vh` and clip the
     last row. */
  .tray {
    width: 216px;
    background-image: var(--chroma), linear-gradient(var(--tint-menu), var(--tint-menu));
    border-radius: var(--radius-window);
    box-shadow: var(--bevel-window), 0 16px 40px -12px rgba(0, 0, 0, 0.7);
  }

  .tray-row { height: 30px; color: var(--text-2); }
  .tray-row.danger.selected { color: var(--accent-red); }

  .tray-glyph {
    flex: none;
    width: 14px;
    height: 14px;
    border-radius: 3px;
    background: var(--fill-3);
  }

  /* The settings window, reproduced the same way. */
  .settings {
    display: flex;
    width: 980px;
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

  .settings aside {
    width: 244px;
    flex: none;
    padding: var(--space-5) var(--space-2) var(--space-2);
    border-right: 1px solid var(--hairline);
  }

  .s-search {
    margin: 0 var(--space-1) var(--space-3);
    padding: 0 var(--space-2);
    height: 30px;
    display: flex;
    align-items: center;
    border-radius: var(--radius-sm);
    background: var(--fill-1);
    box-shadow: inset 0 0 0 1px var(--hairline);
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .s-group {
    margin-top: var(--space-5);
    padding: 0 var(--space-2) var(--space-2);
    font-size: var(--text-label);
    font-weight: var(--weight-strong);
    letter-spacing: var(--track-label);
    text-transform: uppercase;
    color: var(--text-3);
  }

  .scale { padding: var(--space-4); width: 750px; }

  .scale-row {
    display: flex;
    align-items: baseline;
    gap: var(--space-4);
    padding: var(--space-2) 0;
  }

  .scale-row + .scale-row { border-top: 1px solid var(--hairline); }

  .scale-name {
    flex: none;
    width: 130px;
    font-family: var(--font-mono);
    font-size: var(--text-micro);
    color: var(--text-3);
  }

  .scale-sample {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--text-1);
    font-weight: var(--weight-medium);
    letter-spacing: var(--track-tight);
  }

  .scale-px {
    flex: none;
    font-family: var(--font-mono);
    font-size: var(--text-micro);
    color: var(--text-4);
  }

  .s-nav {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    padding: var(--space-1) var(--space-2);
    margin-bottom: 2px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
    text-align: left;
    cursor: pointer;
  }

  .s-nav.selected {
    background-color: var(--fill-2);
    color: var(--text-1);
  }

  .settings main {
    flex: 1;
    min-width: 0;
    padding: var(--space-5) var(--space-8) var(--space-8);
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  .s-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-bottom: var(--space-5);
  }

  .s-head h3 { margin: 0; font-size: var(--text-heading); font-weight: var(--weight-strong); }
  .s-head p { margin: 2px 0 0; font-size: var(--text-meta); color: var(--text-2); }

  .btns { display: flex; gap: var(--space-2); }

  .states { display: flex; flex-wrap: wrap; gap: var(--space-4); }

  .foot {
    margin-top: var(--space-10);
    font-size: var(--text-label);
    color: rgba(255, 255, 255, 0.4);
  }
</style>
