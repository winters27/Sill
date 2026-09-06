<script lang="ts">
  /**
   * A development route. Reachable with `npm run dev`, never linked from the
   * app, and of no use to anyone running Sill rather than working on it.
   *
   * The Ask AI panel, rendered outside Tauri so its layout can be measured.
   * It talks to Rust for the known-provider list and for models, so both are
   * stubbed on `__TAURI_INTERNALS__` before the panel is imported: the real
   * module's top-level code runs on import and needs the shim already there.
   */
  import "$lib/theme/theme.css";
  import { onMount, type Component } from "svelte";

  // Mirrored from src-tauri/src/ai/provider.rs so the measurements are real.
  const KNOWN = [
    {
      id: "claudeCode",
      name: "Claude Code",
      wire: "claudeCode",
      baseUrl: "",
      apiKey: "",
      model: "",
      note: "The Claude Code already on this machine, signed in as you. No key, and nothing stored by Sill.",
    },
    {
      id: "openai",
      name: "OpenAI",
      wire: "openAi",
      baseUrl: "https://api.openai.com/v1",
      apiKey: "",
      model: "gpt-5.2",
      note: "A developer console key. A ChatGPT subscription is a different thing and does not pay for this.",
    },
    {
      id: "anthropic",
      name: "Anthropic",
      wire: "anthropic",
      baseUrl: "https://api.anthropic.com",
      apiKey: "",
      model: "claude-sonnet-5",
      note: "A console key. A Claude subscription cannot be used here, as their terms do not allow it.",
    },
    {
      id: "google",
      name: "Google Gemini",
      wire: "openAi",
      baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
      apiKey: "",
      model: "gemini-3-flash",
      note: "A key from Google AI Studio. The free tier covers personal use.",
    },
    {
      id: "xai",
      name: "xAI Grok",
      wire: "openAi",
      baseUrl: "https://api.x.ai/v1",
      apiKey: "",
      model: "grok-4",
      note: "A console key. SuperGrok and Premium+ are separate and cannot be used here.",
    },
    {
      id: "openrouter",
      name: "OpenRouter",
      wire: "openAi",
      baseUrl: "https://openrouter.ai/api/v1",
      apiKey: "",
      model: "anthropic/claude-sonnet-5",
      note: "One key for many models, billed in one place.",
    },
    {
      id: "ollama",
      name: "Ollama",
      wire: "openAi",
      baseUrl: "http://localhost:11434/v1",
      apiKey: "",
      model: "",
      note: "A model on this machine, or one you point this at. Nothing leaves for anybody else.",
    },
  ];

  const MODELS = [
    { id: "claude-opus-5", label: "Claude Opus 5" },
    { id: "claude-sonnet-5", label: "Claude Sonnet 5" },
    { id: "claude-haiku-4-5", label: "Claude Haiku 4.5" },
  ];

  // Two set up and one of them answering, which is the state the panel is
  // actually in most of the time. An empty panel would hide every card.
  const prefs = $state({
    ai: {
      provider: "anthropic",
      providers: [
        {
          id: "anthropic",
          name: "Anthropic",
          wire: "anthropic",
          baseUrl: "https://api.anthropic.com/v1",
          apiKey: "sk-ant-xxxxxxxx",
          model: "claude-sonnet-5",
        },
        {
          id: "claudeCode",
          name: "Claude Code",
          wire: "claudeCode",
          baseUrl: "",
          apiKey: "",
          model: "",
        },
        {
          id: "ollama",
          name: "Ollama",
          wire: "openAi",
          baseUrl: "http://localhost:11434/v1",
          apiKey: "",
          model: "",
        },
      ],
    },
  });

  /** A total, the shape Rust's `Spent` serialises to, with its mean speed. */
  const total = (
    answers: number,
    input: number,
    output: number,
    cost: number | null,
    meanRate: number | null,
    unpriced = 0,
  ) => ({
    spent: { input, output, cost, unpriced, rate: meanRate, answers, generatingMs: 0 },
    meanRate,
  });

  /**
   * What two providers have cost, so the breakdown can be looked at: one
   * metered with two models and an unpriced answer, one on this machine
   * with a speed instead of a bill.
   */
  const USAGE = [
    {
      provider: "claudeCode",
      all: total(142, 1_284_000, 96_500, 4.18, null, 3),
      month: total(61, 540_000, 41_200, 1.72, null, 0),
      today: total(4, 31_000, 2_900, 0.11, null, 0),
      models: [
        { model: "claude-sonnet-5", label: "claude-sonnet-5", ...total(121, 1_010_000, 80_100, 2.9, null, 3) },
        { model: "claude-opus-5", label: "claude-opus-5", ...total(21, 274_000, 16_400, 1.28, null, 0) },
      ],
      first: 1_754_000_000,
      last: 1_788_600_000,
    },
    {
      provider: "ollama",
      all: total(37, 210_000, 48_000, null, 41.3, 37),
      month: total(37, 210_000, 48_000, null, 41.3, 37),
      today: total(0, 0, 0, null, null, 0),
      models: [
        { model: "huihui_ai/qwen3.5-abliterated:9b", label: "qwen3.5-abliterated:9b", ...total(30, 180_000, 40_000, null, 39.8, 30) },
        { model: "qwen3:1.7b", label: "qwen3:1.7b", ...total(7, 30_000, 8_000, null, 58.2, 7) },
      ],
      first: 1_786_000_000,
      last: 1_788_500_000,
    },
  ];

  /* The panel is imported at runtime, so its type is stated rather than
     inferred. The mock prefs are a subset of the real Preferences. */
  type PanelProps = { prefs: unknown; commit: () => void };
  let Panel = $state<Component<PanelProps> | null>(null);
  let theme = $state("frost");

  $effect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  });

  onMount(async () => {
    (window as never as Record<string, unknown>).__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "settings" } },
      transformCallback: (cb: unknown) => cb,
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "ai_known") return KNOWN;
        if (cmd === "ai_models") return MODELS;
        if (cmd === "ai_usage") return USAGE;
        if (cmd === "ai_usage_reset") return USAGE.filter((one) => one.provider !== args?.provider);
        return null;
      },
    };

    Panel = (await import("$lib/components/settings/AiPanel.svelte"))
      .default as unknown as Component<PanelProps>;
  });
</script>

<div class="frame">
  <div class="bar">
    <select bind:value={theme} aria-label="Theme">
      <option value="frost">frost</option>
      <option value="frost-light">frost-light</option>
      <option value="oilslick">oilslick</option>
      <option value="graphite">graphite</option>
      <option value="ember">ember</option>
      <option value="moss">moss</option>
      <option value="aberration">aberration</option>
    </select>
  </div>

  <div class="body">
    {#if Panel}
      {@const Rendered = Panel}
      <Rendered {prefs} commit={() => {}} />
    {/if}
  </div>
</div>

<style>
  .frame {
    min-height: 100vh;
    background: var(--core-background);
  }

  .bar {
    padding: var(--space-3);
    border-bottom: 1px solid var(--hairline);
  }

  /*
   * The real content column.
   *
   * The settings window is 1180 wide with a 244px sidebar and --space-8 of
   * padding each side, so the panel gets 872px. At the 940px minimum window it
   * gets 632px, so the narrow case is checked by editing this one number.
   *
   * It was `var(--column, 872px)`, and nothing anywhere defined `--column`, so
   * it had always been the fallback. Written as the number it always was.
   */
  .body {
    /* Everything is border-box, so the padding has to be added on top or the
       measured column comes out 64px narrower than the real one. */
    width: calc(872px + 2 * var(--space-8));
    padding: var(--space-6) var(--space-8);
  }
</style>
