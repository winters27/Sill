<script lang="ts">
  /**
   * A conversation, with room.
   *
   * The launcher is the quick lane: one question, one answer, gone in fifteen
   * seconds. This is where you stay. It is not a second chat implementation:
   * the same turns, the same markdown, the same steps and the same card,
   * drawn at a size where they can breathe and next to everything asked
   * before.
   *
   * ## One conversation, two windows
   *
   * Rust holds exactly one open conversation and both surfaces look at it. So
   * resuming something here is what the launcher's Tab continues, and an
   * answer arriving lands in both. That is a smaller idea than two independent
   * chats and it is the honest one: there is one thread of thought, and these
   * are two places to see it.
   */
  import "$lib/theme/theme.css";
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  import TitleBar from "$lib/components/TitleBar.svelte";
  import Markdown from "$lib/components/Markdown.svelte";
  import AiMark from "$lib/components/settings/AiMark.svelte";
  import {
    aiAsk,
    aiConversations,
    aiDecide,
    aiFollowUp,
    aiForget,
    aiNew,
    aiReady,
    aiRefusePending,
    aiResume,
    aiTranscript,
    type AiAsking,
    type AiConversation,
    type AiReady,
    type AiStep,
  } from "$lib/exthost/commands";
  import { applyAppearance, getPreferences, openSettings, type Preferences } from "$lib/settings";

  interface Shown {
    role: string;
    text: string;
    steps: AiStep[];
  }

  let conversation = $state<Shown[]>([]);
  let past = $state<AiConversation[]>([]);
  let answersWith = $state<AiReady | null>(null);

  /** What is being written right now, before it becomes a turn. */
  let answering = $state("");
  let steps = $state<AiStep[]>([]);
  let asking = $state(false);
  let asked = $state<AiAsking | null>(null);
  let trouble = $state("");

  let draft = $state("");
  let composer = $state<HTMLTextAreaElement | null>(null);
  let transcript = $state<HTMLDivElement | null>(null);

  /** Which conversation is open, so the list can mark it. */
  const openId = $derived(past.find((one) => one.open)?.id ?? "");

  async function refreshList() {
    try {
      past = await aiConversations();
    } catch (err) {
      trouble = `${err}`;
    }
  }

  /**
   * Sends what is in the composer.
   *
   * The first question of an empty conversation begins one; anything after it
   * continues the one open. Two calls rather than a flag, for the reason
   * written where they are declared.
   */
  async function send() {
    const question = draft.trim();
    if (!question || asking) return;

    const starting = conversation.length === 0;

    conversation = [...conversation, { role: "user", text: question, steps: [] }];
    draft = "";
    answering = "";
    steps = [];
    asked = null;
    trouble = "";
    asking = true;

    try {
      await (starting ? aiAsk(question) : aiFollowUp(question));
    } catch (err) {
      trouble = `${err}`;
      asking = false;
    }

    await refreshList();
  }

  async function open(id: string) {
    if (asking) return;

    try {
      conversation = (await aiResume(id)).map((turn) => ({ ...turn, steps: [] }));
    } catch (err) {
      trouble = `${err}`;
      return;
    }

    answering = "";
    steps = [];
    asked = null;
    trouble = "";
    await refreshList();
    composer?.focus();
  }

  async function begin() {
    if (asking) return;

    await aiNew();
    conversation = [];
    answering = "";
    steps = [];
    asked = null;
    trouble = "";
    await refreshList();
    composer?.focus();
  }

  async function forget(id: string) {
    try {
      past = await aiForget(id);
    } catch (err) {
      trouble = `${err}`;
      return;
    }

    // The one on screen is the one that went, so the screen follows.
    if (id === openId || !past.some((one) => one.open)) {
      conversation = await aiTranscript().then((turns) =>
        turns.map((turn) => ({ ...turn, steps: [] })),
      );
    }
  }

  function decide(allowed: boolean) {
    if (!asked) return;
    void aiDecide(asked.id, allowed);
    asked = null;
  }

  /**
   * Enter sends, Shift and Enter writes a line.
   *
   * The opposite of the launcher, where the field is one line and Enter is the
   * only thing it can mean. Here the composer is a box somebody may want a
   * paragraph in, and every chat window that has ever existed has settled on
   * this pair.
   */
  function onComposerKey(event: KeyboardEvent) {
    if (asked) {
      if (event.key === "Enter") {
        event.preventDefault();
        decide(true);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        decide(false);
        return;
      }
    }

    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  }

  /** What one step did, in words. Shared meaning with the launcher's list. */
  const DID: Record<string, string> = {
    search_sill: "Searched what is on this machine for",
    find_files: "Looked for files called",
    read_file: "Read",
    list_directory: "Looked inside",
    read_clipboard: "Read what you have copied",
    list_windows: "Looked at what is open",
    system_state: "Checked how this machine is set",
    read_selection: "Read what was selected",
    read_screen: "Read what is on screen",
    what_can_be_done: "Worked out what can be done to",
    run_action: "Acted on",
  };

  function didWhat(step: AiStep): string {
    const said = DID[step.tool] ?? step.tool;
    return step.subject ? `${said} ${step.subject}` : said;
  }

  function when(one: AiConversation): string {
    if (one.age < 60) return "Just now";
    if (one.age < 3600) return `${Math.floor(one.age / 60)} min ago`;
    if (one.age < 86_400) return `${Math.floor(one.age / 3600)} hr ago`;
    return `${Math.floor(one.age / 86_400)} d ago`;
  }

  /** Sticks to the bottom while an answer is arriving. */
  $effect(() => {
    // Read so the effect runs on each of them.
    answering;
    conversation.length;
    steps.length;

    if (transcript) transcript.scrollTop = transcript.scrollHeight;
  });

  onMount(() => {
    let said: UnlistenFn | undefined;
    let using: UnlistenFn | undefined;
    let wants: UnlistenFn | undefined;
    let finished: UnlistenFn | undefined;
    let wentWrong: UnlistenFn | undefined;
    let changed: UnlistenFn | undefined;

    (async () => {
      const prefs: Preferences = await getPreferences();
      applyAppearance(prefs);

      answersWith = await aiReady().catch(() => null);

      conversation = (await aiTranscript()).map((turn) => ({ ...turn, steps: [] }));
      await refreshList();

      said = await listen<string>("sill://ai-said", ({ payload }) => {
        answering += payload;
      });

      using = await listen<AiStep>("sill://ai-using", ({ payload }) => {
        steps = [...steps, payload];
      });

      wants = await listen<AiAsking>("sill://ai-asking", ({ payload }) => {
        asked = payload;
      });

      finished = await listen("sill://ai-done", () => {
        if (answering) {
          conversation = [...conversation, { role: "assistant", text: answering, steps }];
        }
        answering = "";
        asking = false;
        void refreshList();
      });

      wentWrong = await listen<string>("sill://ai-failed", ({ payload }) => {
        // Half an answer is often enough to see what went wrong.
        if (answering) {
          conversation = [...conversation, { role: "assistant", text: answering, steps }];
        }
        answering = "";
        asking = false;
        trouble = payload;
      });

      changed = await listen<Preferences>("sill://preferences-changed", async ({ payload }) => {
        applyAppearance(payload);
        answersWith = await aiReady().catch(() => null);
      });

      composer?.focus();
    })();

    return () => {
      said?.();
      using?.();
      wants?.();
      finished?.();
      wentWrong?.();
      changed?.();
      // A card nobody answered would otherwise hold its turn open long after
      // this window is gone.
      void aiRefusePending();
    };
  });
</script>

<div class="window">
  <TitleBar title="Ask" />

  <div class="body">
    <aside class="past">
      <button class="fresh" onclick={() => void begin()} disabled={asking}>
        New conversation
      </button>

      <div class="list sill-scrolls">
        {#each past as one (one.id)}
          <div class="row" class:open={one.id === openId}>
            <button class="pick" onclick={() => void open(one.id)} disabled={asking}>
              <span class="what">{one.title}</span>
              <span class="meta">
                {when(one)} · {one.replies}
                {one.replies === 1 ? "reply" : "replies"}
              </span>
            </button>
            <button
              class="bin"
              aria-label="Forget this conversation"
              title="Forget this conversation"
              onclick={() => void forget(one.id)}
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <path
                  d="M4 7h16M10 4h4M6 7l1 13h10l1-13M10 11v6M14 11v6"
                  stroke="currentColor"
                  stroke-width="1.8"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            </button>
          </div>
        {/each}

        {#if past.length === 0}
          <p class="nothing">Nothing asked yet.</p>
        {/if}
      </div>
    </aside>

    <main class="pane">
      <div class="transcript sill-scrolls" bind:this={transcript}>
        {#if conversation.length === 0 && !asking && !answering}
          <div class="opening">
            {#if answersWith?.ready}
              <p class="lead">
                <AiMark name={answersWith.id} size={18} />
                <span>Ask {answersWith.model || answersWith.name} anything</span>
              </p>
            {/if}
            <p class="reach">
              It can look through this machine to answer: what is installed and
              open, what you have copied or selected, a file or a folder, and
              what is on screen. It can act on what it finds, and anything that
              changes something stops to ask you first.
            </p>
            {#if !answersWith?.ready}
              <button class="setup" onclick={() => void openSettings("ai")}>
                Set up Ask
              </button>
            {/if}
          </div>
        {/if}

        {#each conversation as turn, at (at)}
          {#if turn.role === "user"}
            <article class="turn asked"><p>{turn.text}</p></article>
          {:else}
            {#if turn.steps.length}
              <div class="steps">
                {#each turn.steps as step, n (n)}
                  <p class="step"><span class="pip" aria-hidden="true"></span>{didWhat(step)}</p>
                {/each}
              </div>
            {/if}
            <article class="turn said md"><Markdown text={turn.text} /></article>
          {/if}
        {/each}

        {#if asking && steps.length}
          <div class="steps">
            {#each steps as step, at (at)}
              <p class="step"><span class="pip" aria-hidden="true"></span>{didWhat(step)}</p>
            {/each}
          </div>
        {/if}

        {#if asked}
          <div class="permission">
            <p class="wants">{asked.title}</p>
            <p class="subject">{asked.subject}</p>
            <p class="touches">This {asked.touches}.</p>
            <div class="answers">
              <button class="allow" onclick={() => decide(true)}>
                <span class="sill-key">Enter</span> Do it
              </button>
              <button class="refuse" onclick={() => decide(false)}>
                <span class="sill-key">Esc</span> Not now
              </button>
            </div>
          </div>
        {/if}

        {#if answering}
          <article class="turn said md"><Markdown text={answering} /></article>
        {:else if asking && !asked}
          <p class="thinking">Thinking<span class="dots" aria-hidden="true"></span></p>
        {/if}

        {#if trouble}
          <p class="trouble">{trouble}</p>
        {/if}
      </div>

      <div class="composer">
        <textarea
          bind:this={composer}
          bind:value={draft}
          onkeydown={onComposerKey}
          placeholder={asked
            ? "Answer above first…"
            : asking
              ? "Waiting for the answer…"
              : conversation.length === 0
                ? "Ask anything…"
                : "Ask a follow-up…"}
          rows="1"
          spellcheck="false"
          aria-label="Ask"
        ></textarea>

        <button
          class="send"
          onclick={() => void send()}
          disabled={!draft.trim() || asking}
          aria-label="Send"
        >
          <span class="sill-key">Enter</span>
        </button>
      </div>
    </main>
  </div>
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    background: transparent;
    overflow: hidden;
  }

  .window {
    display: flex;
    flex-direction: column;
    height: 100vh;
    /* The same recipe every other Sill window uses: mixed toward the base
       colour rather than toward transparency, so the surface stays opaque and
       subpixel text rendering stays on. */
    background-color: color-mix(
      in srgb,
      var(--core-secondary-background) calc((1 - var(--glass-strength)) * 100%),
      var(--surface-base)
    );
    background-image: var(--chroma), linear-gradient(var(--tint), var(--tint));
    border-radius: var(--radius-window);
    box-shadow: var(--bevel-window);
    overflow: hidden;
    color: var(--text-1);
    font-family: var(--font-ui);
    font-size: var(--text-body);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  /* ------------------------------------------------- everything asked before */

  .past {
    width: 260px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border-right: 1px solid var(--hairline);
  }

  .fresh {
    flex: none;
    padding: var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--accent-fill);
    color: var(--accent);
    font: inherit;
    font-size: var(--text-meta);
    font-weight: var(--weight-medium);
    cursor: pointer;
    transition: background-color 0.15s var(--ease);
  }

  .fresh:hover:not(:disabled) {
    background: var(--accent-fill-strong);
  }

  .fresh:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .row {
    display: flex;
    align-items: stretch;
    border-radius: var(--radius-md);
  }

  .row:hover {
    background: var(--fill-1);
  }

  /* The one being read, marked rather than merely hovered. */
  .row.open {
    background: var(--accent-fill);
  }

  .pick {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: var(--space-2);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .pick:disabled {
    cursor: default;
  }

  .what {
    color: var(--text-1);
    font-size: var(--text-meta);
    /* One line. A question can be a paragraph, and a list of paragraphs is
       not a list. */
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    color: var(--text-3);
    font-size: var(--text-micro);
  }

  /*
   * Forgetting one, kept quiet until the row is under the pointer.
   *
   * The opposite of the approval card's reasoning and for the same reason: a
   * delete that is always visible on every row in a list is a delete somebody
   * eventually hits by accident.
   */
  .bin {
    flex: none;
    width: 28px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s var(--ease), color 0.15s var(--ease);
  }

  .row:hover .bin,
  .bin:focus-visible {
    opacity: 1;
  }

  .bin:hover {
    color: var(--accent-red);
  }

  .nothing {
    margin: var(--space-2);
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  /* --------------------------------------------------------- the conversation */

  .pane {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .transcript {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-5) var(--space-5);
  }

  .turn {
    font-size: var(--text-body);
  }

  .asked {
    align-self: flex-end;
    max-width: 72%;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-lg) var(--radius-lg) var(--radius-sm) var(--radius-lg);
    background: var(--accent-fill);
    box-shadow: inset 0 0 0 1px var(--accent-line);
  }

  .asked p {
    margin: 0;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .said {
    align-self: flex-start;
    max-width: 74ch;
    width: 100%;
  }

  .steps {
    align-self: flex-start;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .step {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    margin: 0;
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: 1.5;
  }

  .pip {
    width: 4px;
    height: 4px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.7;
  }

  .permission {
    align-self: flex-start;
    max-width: 62ch;
    width: 100%;
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    box-shadow: inset 0 0 0 1px var(--accent-line);
  }

  .wants {
    margin: 0;
    color: var(--accent);
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .subject {
    margin: var(--space-1) 0 0;
    font-size: var(--text-body);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .touches {
    margin: var(--space-1) 0 var(--space-3);
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .answers {
    display: flex;
    gap: var(--space-2);
  }

  .answers button {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
    transition: background-color 0.15s var(--ease), color 0.15s var(--ease);
  }

  .answers button:hover {
    color: var(--text-1);
  }

  .allow {
    background: var(--accent-fill);
    color: var(--accent);
  }

  .allow:hover {
    background: var(--accent-fill-strong);
  }

  .opening {
    align-self: flex-start;
    max-width: 62ch;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .lead {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-title);
    font-weight: var(--weight-strong);
  }

  .reach {
    margin: 0;
    color: var(--text-2);
    line-height: 1.65;
  }

  .setup {
    align-self: flex-start;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--accent-fill);
    color: var(--accent);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
  }

  .thinking {
    align-self: flex-start;
    margin: 0;
    color: var(--text-2);
  }

  .dots::after {
    content: "";
    animation: thinking 1.4s steps(4, end) infinite;
  }

  @keyframes thinking {
    0% {
      content: "";
    }
    25% {
      content: ".";
    }
    50% {
      content: "..";
    }
    75% {
      content: "...";
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dots::after {
      animation: none;
      content: "…";
    }
  }

  .trouble {
    align-self: flex-start;
    margin: 0;
    color: var(--accent-red);
    font-size: var(--text-meta);
  }

  /* ---------------------------------------------------------------- composer */

  .composer {
    flex: none;
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-5) var(--space-4);
    border-top: 1px solid var(--hairline);
  }

  textarea {
    flex: 1;
    min-width: 0;
    /* Grows with what is written, up to the point where the conversation above
       it would start disappearing. */
    min-height: 38px;
    max-height: 200px;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: inset 0 0 0 1px var(--hairline);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-body);
    line-height: 1.5;
    resize: vertical;
    outline: none;
    field-sizing: content;
  }

  textarea:focus {
    box-shadow: inset 0 0 0 1px var(--hairline-strong);
  }

  textarea::placeholder {
    color: var(--text-3);
  }

  .send {
    flex: none;
    height: 38px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--accent-fill);
    color: var(--accent);
    cursor: pointer;
    transition: background-color 0.15s var(--ease), opacity 0.15s var(--ease);
  }

  .send:hover:not(:disabled) {
    background: var(--accent-fill-strong);
  }

  .send:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
