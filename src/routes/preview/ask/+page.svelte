<script lang="ts">
  /**
   * A development route. Reachable with `npm run dev`, never linked from the
   * app, and of no use to anyone running Sill rather than working on it.
   *
   * The chat window, rendered outside Tauri, with an answer that arrives the
   * way a real one does: a wait, thinking, two tools, then words in pieces.
   * The bridge is stubbed on `__TAURI_INTERNALS__` before the page is
   * imported, and `listen` is answered by keeping the handlers so the script
   * below can call them the way Rust would.
   */
  import "$lib/theme/theme.css";
  import { onMount, type Component } from "svelte";

  const PREFS = {
    appearance: {
      backdrop: "acrylic",
      theme: "frost",
      chromaStrength: 1,
      font: "satoshi",
      glassStrength: 0.85,
      tintAlpha: 0.1,
      visibleRows: 8,
      windowWidth: 760,
      summonOn: "cursor",
    },
  };

  /**
   * `?paid` swaps the local model for a metered one, so the pill that counts
   * the conversation can be seen pricing an answer rather than timing it.
   */
  const paid = new URLSearchParams(location.search).has("paid");

  const READY = paid
    ? {
        ready: true,
        id: "xai",
        name: "xAI Grok",
        model: "grok-4.6",
        kind: "key",
        price: { input: 2, output: 6 },
        whyNot: "",
      }
    : {
        ready: true,
        id: "ollama",
        name: "Ollama",
        model: "qwen3:1.7b",
        kind: "local",
        price: null,
        whyNot: "",
      };

  const PAST = [
    { id: "chat:2", title: "Find the largest files in my Downloads folder", replies: 1, age: 90, open: false },
    { id: "chat:1", title: "What is my volume set to?", replies: 2, age: 5400, open: false },
  ];

  const REOPENED = [
    { role: "user", text: "Find the largest files in my Downloads folder", attachments: [], parts: [] },
    {
      role: "assistant",
      text: "The three largest are all installers:\n\n1. **Win11_24H2.iso**, 6.2 GB\n2. **cuda_12.8.exe**, 3.1 GB\n3. **Fedora-Workstation.iso**, 2.4 GB",
      attachments: [],
      parts: [
        { kind: "thinking", text: "Downloads is under the home folder. List it, sort by size.", ms: 1840 },
        { kind: "step", id: "a", tool: "list_directory", subject: "C:\\Users\\you\\Downloads", ok: true },
        {
          kind: "text",
          text: "The three largest are all installers:\n\n1. **Win11_24H2.iso**, 6.2 GB\n2. **cuda_12.8.exe**, 3.1 GB\n3. **Fedora-Workstation.iso**, 2.4 GB",
        },
      ],
    },
  ];

  const ANSWER = [
    "Four windows are open right now.\n\n",
    "| Window | Where |\n| --- | --- |\n",
    "| **Zen** | second display |\n",
    "| Terminal | running `cargo test` |\n",
    "| Obsidian | the Brain vault |\n",
    "| Sill Settings | behind this one |\n\n",
    "Your clipboard's last entry mentions an **invoice**, but I could not read the full history: ",
    "the clipboard tool answered with an error, so that part is unknown.\n\n",
    "## What you could do next\n\n",
    "- Ask me to bring **Zen** to the front\n",
    "- Ask what the terminal is doing\n",
  ];

  const THINKING = [
    "The question is about open windows, ",
    "so the window list is the tool to reach for. ",
    "The clipboard mention suggests checking that too, ",
    "though it may not be relevant.",
  ];

  const handlers = new Map<string, Array<(event: unknown) => void>>();

  function emit(name: string, payload: unknown) {
    for (const handler of handlers.get(name) ?? []) handler({ event: name, id: 0, payload });
  }

  const wait = (ms: number) => new Promise((done) => setTimeout(done, ms));

  /** An answer, at roughly the pace a small local model produces one. */
  async function answer(): Promise<string> {
    await wait(700);
    for (const piece of THINKING) {
      emit("sill://ai-thinking", piece);
      await wait(220);
    }
    await wait(300);
    emit("sill://ai-using", { id: "c1", tool: "list_windows", subject: "" });
    await wait(800);
    emit("sill://ai-used", { id: "c1", ok: true });
    emit("sill://ai-using", { id: "c2", tool: "read_clipboard", subject: "invoice" });
    await wait(700);
    emit("sill://ai-used", { id: "c2", ok: false });
    await wait(200);
    for (const piece of ANSWER) {
      emit("sill://ai-said", piece);
      await wait(160);
    }
    turns += 1;
    emit("sill://ai-done", {
      model: READY.model,
      usage: { input: 812, output: 143 },
      durationMs: 4120,
      generatingMs: 3100,
      cost: paid ? 0.002482 : null,
      spent: {
        input: 812 * turns,
        output: 143 * turns,
        cost: paid ? 0.002482 * turns : null,
        unpriced: paid ? 0 : turns,
        rate: paid ? null : 46.1,
        answers: turns,
      },
    });
    return ANSWER.join("");
  }

  /** Answers given so far, so the total on each done event grows. */
  let turns = 0;

  let Page = $state<Component | null>(null);

  onMount(async () => {
    (window as never as Record<string, unknown>).__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: "ask" },
        currentWebview: { windowLabel: "ask", label: "ask" },
      },
      transformCallback: (cb: unknown) => cb,
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        switch (cmd) {
          case "plugin:event|listen": {
            const name = String(args?.event);
            const list = handlers.get(name) ?? [];
            list.push(args?.handler as (event: unknown) => void);
            handlers.set(name, list);
            return list.length;
          }
          case "get_preferences":
            return PREFS;
          case "ai_limits":
            return { image: 4 * 1024 * 1024, text: 100_000 };
          case "ai_ready":
            return READY;
          case "ai_transcript":
            return [];
          case "ai_conversations":
            return PAST;
          case "ai_resume":
            return REOPENED;
          case "ai_outstanding":
            return null;
          case "ai_ask":
          case "ai_follow_up":
            return answer();
          default:
            return null;
        }
      },
    };

    Page = (await import("../../ask/+page.svelte")).default as unknown as Component;
  });
</script>

<div class="frame">
  {#if Page}
    {@const Rendered = Page}
    <Rendered />
  {/if}
</div>

<style>
  .frame {
    width: 1060px;
    height: 760px;
  }
</style>
