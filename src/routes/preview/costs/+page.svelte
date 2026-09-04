<script lang="ts">
  /**
   * A development route. Reachable with `npm run dev`, never linked from the
   * app, and of no use to anyone running Sill rather than working on it.
   *
   * The Extensions panel's cost table, drawn against real measurements rather
   * than made-up ones. Every figure below came out of
   * `scripts/run-extension.mjs --measure` against a real host, and they are
   * the same numbers written down in `docs/budgets.md`.
   *
   * That is the point of having this. A screen whose job is to name the
   * expensive extension can only be judged against real numbers: invented ones
   * can be given any spread, including the one that makes the wording look
   * right. These are what five extensions people actually use really cost, and
   * what the panel says about them is what it will say on somebody's machine.
   *
   * Three states, because the third is the one worth checking. A run where
   * nothing stands out has to say so rather than blaming whichever row sorted
   * first.
   */
  import "$lib/theme/theme.css";
  import ExtensionCosts from "$lib/components/settings/ExtensionCosts.svelte";
  import type { CostRow } from "$lib/costs";
  import type { RunningCommand } from "$lib/store";

  const MB = 1024 * 1024;

  const WALLS = {
    dark: "radial-gradient(120% 90% at 20% 10%, #23262b, #0a0a0b 70%)",
    mid: "radial-gradient(120% 90% at 30% 20%, #4a5560, #1d2228 75%)",
    light: "radial-gradient(120% 90% at 25% 15%, #e8e4dc, #b9b2a6 75%)",
  } as const;

  const THEMES = ["winters-glass", "oilslick", "graphite", "ember", "moss", "aberration"] as const;

  let wall = $state<keyof typeof WALLS>("dark");
  let theme = $state<(typeof THEMES)[number]>("winters-glass");

  $effect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  });

  function row(
    extension: string,
    title: string,
    coldMs: number,
    warmMs: number,
    heldMb: number | null,
    running: RunningCommand[] = [],
  ): CostRow {
    return {
      extension,
      title,
      cost: {
        extension,
        coldMs,
        coldOpens: 1,
        warmMs,
        warmOpens: 1,
        heldBytes: heldMb === null ? null : heldMb * MB,
        running,
      },
    };
  }

  /** Measured 2026-09-03, five real extensions, slowest to open first. */
  const REAL: CostRow[] = [
    row("hacker-news", "Hacker News", 525, 114, 15),
    row("emoji", "Emoji Search", 516, 74, 63),
    row("kill-process", "Kill Process", 531, 50, 12),
    row("password-generator", "Password Generator", 527, 39, 11),
    row("uuid-generator", "UUID Generator", 526, 36, 11),
  ];

  /** The same, with one of them open and one of them wedged. */
  const RUNNING: CostRow[] = [
    row("emoji", "Emoji Search", 516, 74, 63, [
      {
        session: "a",
        extension: "emoji",
        command: "Search Emoji",
        heapBytes: 63 * MB,
        heapLimitBytes: 512 * MB,
        corePercent: 0.8,
        answering: true,
      },
    ]),
    row("kill-process", "Kill Process", 531, 50, 37, [
      {
        session: "b",
        extension: "kill-process",
        command: "Kill Process",
        heapBytes: null,
        heapLimitBytes: 512 * MB,
        corePercent: 99.4,
        answering: false,
      },
    ]),
    row("uuid-generator", "UUID Generator", 526, 36, 11),
  ];

  /** A run where naming a culprit would be blaming somebody at random. */
  const EVEN: CostRow[] = [
    row("password-generator", "Password Generator", 527, 39, 11),
    row("uuid-generator", "UUID Generator", 526, 36, 11),
  ];
</script>

<div class="page" style="background: {WALLS[wall]}">
  <div class="controls">
    {#each Object.keys(WALLS) as name (name)}
      <button class:on={wall === name} onclick={() => (wall = name as keyof typeof WALLS)}>
        {name}
      </button>
    {/each}
    {#each THEMES as name (name)}
      <button class:on={theme === name} onclick={() => (theme = name)}>{name}</button>
    {/each}
  </div>

  <div class="panel">
    <h2>Five real extensions</h2>
    <ExtensionCosts rows={REAL} />

    <h2>With one open and one wedged</h2>
    <ExtensionCosts rows={RUNNING} />

    <h2>Nothing to choose between them</h2>
    <ExtensionCosts rows={EVEN} />

    <h2>Before anything has been opened</h2>
    <ExtensionCosts rows={[]} />
  </div>
</div>

<style>
  .page {
    min-height: 100vh;
    padding: var(--space-6);
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-bottom: var(--space-5);
  }

  button {
    padding: var(--space-half) var(--space-2);
    font: inherit;
    font-size: var(--text-label);
    color: var(--text-2);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  button.on {
    color: var(--accent-bright);
    background: var(--fill-2);
  }

  .panel {
    max-width: 720px;
  }

  h2 {
    margin: var(--space-5) 0 var(--space-2);
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
    color: var(--text-3);
  }
</style>
