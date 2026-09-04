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
  import Steps from "$lib/components/Steps.svelte";
  import Instead from "$lib/components/Instead.svelte";
  import { standing } from "$lib/instead";
  import { open as pickFiles } from "@tauri-apps/plugin-dialog";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import {
    aiAsk,
    aiAttach,
    aiConversations,
    aiDecide,
    aiFollowUp,
    aiForget,
    aiNew,
    aiOutstanding,
    aiReady,
    aiRefusePending,
    aiResume,
    aiLimits,
    aiStop,
    aiTranscript,
    type AiAsking,
    type AiAttached,
    type AiConversation,
    type AiLimits,
    type AiReady,
    type AiStep,
  } from "$lib/exthost/commands";
  import { applyAppearance, getPreferences, openSettings, type Preferences } from "$lib/settings";
  import { forgetUnreadable, orElse, silently } from "$lib/status";
  import { hint } from "$lib/hint";

  interface Shown {
    role: string;
    text: string;
    steps: AiStep[];
    attachments: AiAttached[];
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
  /** What is waiting to go with the next question. */
  let carrying = $state<AiAttached[]>([]);
  /** Whether something is being dragged over the window right now. */
  let hovering = $state(false);
  /** Which answer has just been copied, so the button can say so. */
  let copied = $state(-1);

  /**
   * The clock the sidebar reads ages against.
   *
   * Ticking rather than baked in when the list is fetched. A window left open
   * for an hour showed every conversation as it was an hour ago, because the
   * age was a string worked out once and never again.
   */
  let now = $state(Math.floor(Date.now() / 1000));
  let composer = $state<HTMLTextAreaElement | null>(null);
  let transcript = $state<HTMLDivElement | null>(null);

  /** Which conversation is open, so the list can mark it. */
  const openId = $derived(past.find((one) => one.open)?.id ?? "");

  /**
   * Takes files and says what could not be taken.
   *
   * Each answers for itself: five files where one is an archive attaches four
   * and explains one, rather than attaching nothing and complaining about the
   * archive.
   */
  async function take(paths: string[]) {
    const refused: string[] = [];

    for (const path of paths) {
      try {
        carrying = [...carrying, await aiAttach(path)];
      } catch (err) {
        refused.push(`${err}`);
      }
    }

    trouble = refused.join(" ");
  }

  async function pick() {
    const chosen = await pickFiles({ multiple: true });
    if (!chosen) return;
    await take(Array.isArray(chosen) ? chosen : [chosen]);
    composer?.focus();
  }

  function drop(name: string) {
    carrying = carrying.filter((one) => one.name !== name);
  }

  /** The ceilings, read once from the one place that defines them. */
  let ceiling = $state<AiLimits>({ image: 4 * 1024 * 1024, text: 100_000 });

  /**
   * A picture pasted from the clipboard.
   *
   * It never touches the disk, so it cannot go through the reader every other
   * attachment goes through, and this is the one path that builds an
   * attachment itself. The ceiling it checks is the same one, asked for rather
   * than repeated.
   */
  async function onPaste(event: ClipboardEvent) {
    const pictures = [...(event.clipboardData?.files ?? [])].filter((one) =>
      one.type.startsWith("image/"),
    );

    if (pictures.length === 0) return;
    event.preventDefault();

    for (const picture of pictures) {
      if (picture.size > ceiling.image) {
        trouble = `That picture is ${size(picture.size)}, and one has to be under ${size(ceiling.image)} to send.`;
        continue;
      }

      /*
       * Said here rather than on the status surface.
       *
       * A picture that cannot be read is a failure about the thing somebody
       * just did, in the window they did it in, and there is already a line
       * for exactly that above: a picture too large to send says so here. A
       * paste that silently attached nothing was the only outcome of this
       * function that left no trace at all.
       */
      const body = await new Promise<string>((done, fail) => {
        const reader = new FileReader();
        reader.onload = () => done(String(reader.result));
        reader.onerror = () => fail(reader.error);
        reader.readAsDataURL(picture);
      }).catch(() => "");

      if (!body) {
        trouble = `That picture could not be read, so it was not attached.`;
        continue;
      }

      carrying = [
        ...carrying,
        {
          // A pasted screenshot has no name of its own on Windows.
          name: picture.name || `pasted picture ${carrying.length + 1}`,
          kind: "image",
          body,
          bytes: picture.size,
        },
      ];
    }
  }

  /** A size somebody would say out loud. */
  function size(bytes: number): string {
    if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
    return `${bytes} bytes`;
  }

  /** When the list was last fetched, so its ages can be anchored. */
  let fetched = $state(Math.floor(Date.now() / 1000));

  async function refreshList() {
    try {
      past = await aiConversations();
      fetched = Math.floor(Date.now() / 1000);
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

    // Something attached is a question in itself. A screenshot with nothing
    // typed is the most ordinary thing anybody does with a picture.
    if ((!question && carrying.length === 0) || asking) return;

    const starting = conversation.length === 0;
    const going = carrying;

    conversation = [
      ...conversation,
      { role: "user", text: question, steps: [], attachments: going },
    ];
    draft = "";
    carrying = [];
    answering = "";
    steps = [];
    asked = null;
    trouble = "";
    asking = true;

    try {
      await (starting ? aiAsk(question, going) : aiFollowUp(question, going));
    } catch (err) {
      trouble = `${err}`;
      asking = false;
    }

    await refreshList();
  }

  /**
   * Asks the last question again.
   *
   * The whole turn is sent afresh rather than the answer being patched,
   * because there is nothing to patch: the conversation held in Rust already
   * has the question in it, and asking again is what everybody means by this.
   */
  async function again() {
    if (asking) return;

    const last = [...conversation].reverse().find((turn) => turn.role === "user");
    if (!last) return;

    draft = last.text;
    carrying = last.attachments;
    await send();
  }

  async function copy(at: number, text: string) {
    try {
      await writeText(text);
      copied = at;
      setTimeout(() => (copied = -1), 1400);
    } catch {
      // The clipboard refusing is rare and the answer is still on screen to be
      // selected. Saying nothing beats a message over a two line reply.
    }
  }

  async function stop() {
    await aiStop();
  }

  /**
   * Grows the composer to fit what is in it.
   *
   * Done here rather than left to `field-sizing`, which is new enough that
   * whether it works depends on which WebView2 runtime is installed. A box
   * that silently stays one line high on somebody's machine is not a thing to
   * find out about later.
   *
   * Height is cleared before it is read, because `scrollHeight` on an element
   * with a height already set reports that height rather than the content.
   */
  function fit(box: HTMLTextAreaElement | null) {
    if (!box) return;
    box.style.height = "auto";
    box.style.height = `${Math.min(box.scrollHeight, GROWS_TO)}px`;
  }

  /** As far as it grows before it starts scrolling instead. */
  const GROWS_TO = 200;

  // Whatever changes what is in it changes how tall it is: typing, sending,
  // and putting a question back to ask again.
  $effect(() => {
    draft;
    fit(composer);
  });

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

  /**
   * How long ago, read against a clock that moves.
   *
   * `one.age` is how old it was when the list was fetched, so a window left
   * open shows every row as it was then. What was fetched is turned back into
   * an absolute moment and compared against now, which ticks.
   */
  function when(one: AiConversation): string {
    const at = fetched - one.age;
    const age = Math.max(0, now - at);

    if (age < 60) return "Just now";
    if (age < 3600) return `${Math.floor(age / 60)} min ago`;
    if (age < 86_400) return `${Math.floor(age / 3600)} hr ago`;
    return `${Math.floor(age / 86_400)} d ago`;
  }

  /*
   * The clock, moved once a minute.
   *
   * The coarsest thing that keeps every row true, because nothing here is
   * measured in seconds after the first one. A window nobody is looking at
   * costs one wakeup a minute, which is the smallest honest price for a list
   * that does not lie about when things happened.
   */
  $effect(() => {
    const ticking = setInterval(() => (now = Math.floor(Date.now() / 1000)), 60_000);
    return () => clearInterval(ticking);
  });

  /** Sticks to the bottom while an answer is arriving. */
  $effect(() => {
    // Read so the effect runs on each of them.
    answering;
    conversation.length;
    steps.length;

    if (transcript) transcript.scrollTop = transcript.scrollHeight;
  });

  onMount(() => {
    let dropped: UnlistenFn | undefined;
    let said: UnlistenFn | undefined;
    let using: UnlistenFn | undefined;
    let wants: UnlistenFn | undefined;
    let finished: UnlistenFn | undefined;
    let wentWrong: UnlistenFn | undefined;
    let changed: UnlistenFn | undefined;

    (async () => {
      const prefs: Preferences = await getPreferences();
      applyAppearance(prefs);

      // Forgotten before these ask again, so a failure that has since been
      // fixed is not still being reported. Scoped to this window: a flat group
      // would mean opening this one erased what the launcher had found.
      void forgetUnreadable("ask");

      // Silent. The fallback is the ceiling this page already holds, which is
      // the same pair of numbers Rust would have answered with unless somebody
      // has changed them, so nothing on screen becomes untrue. What it costs is
      // that a picture right on the edge is judged against yesterday's limit.
      ceiling = await aiLimits().catch(silently(ceiling));

      answersWith = await aiReady().catch(
        orElse("ask", "whether anything is set up to answer", null, "ai"),
      );

      conversation = (await aiTranscript()).map((turn) => ({ ...turn, steps: [] }));
      await refreshList();

      /*
       * Dropping files on the window.
       *
       * Tauri's own event rather than the DOM's: a browser drop hands over a
       * File with no path, and everything here works from paths. The DOM
       * handlers on the window only draw the outline.
       */
      dropped = await listen<{ paths: string[] }>("tauri://drag-drop", ({ payload }) => {
        hovering = false;
        void take(payload.paths ?? []);
      });

      said = await listen<string>("sill://ai-said", ({ payload }) => {
        answering += payload;
      });

      using = await listen<AiStep>("sill://ai-using", ({ payload }) => {
        steps = [...steps, payload];
      });

      wants = await listen<AiAsking>("sill://ai-asking", ({ payload }) => {
        asked = payload;
      });

      /*
       * The card that was raised before this window existed.
       *
       * This window is opened BY a card, when nothing else of Sill's is on
       * screen: a deep link or an MCP client asks for something, there is
       * nowhere to show the question, and Rust builds this page to hold it.
       * The event announcing the card went out while that was happening, so
       * the one question this window exists to ask is the one it cannot hear.
       *
       * After the listener, so a card raised in the gap is not lost either,
       * and it does not overwrite one that has already arrived.
       */
      asked ??= await aiOutstanding().catch(
        orElse("ask", "the question waiting to be answered", null, "ai"),
      );

      finished = await listen("sill://ai-done", () => {
        if (answering) {
          conversation = [
            ...conversation,
            { role: "assistant", text: answering, steps, attachments: [] },
          ];
        }
        answering = "";
        asking = false;
        void refreshList();
      });

      wentWrong = await listen<string>("sill://ai-failed", ({ payload }) => {
        // Half an answer is often enough to see what went wrong.
        if (answering) {
          conversation = [
            ...conversation,
            { role: "assistant", text: answering, steps, attachments: [] },
          ];
        }
        answering = "";
        asking = false;
        trouble = payload;
      });

      changed = await listen<Preferences>("sill://preferences-changed", async ({ payload }) => {
        applyAppearance(payload);
        answersWith = await aiReady().catch(
          orElse("ask", "whether anything is set up to answer", null, "ai"),
        );
      });

      composer?.focus();
    })();

    return () => {
      dropped?.();
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

<!--
  Dropping a file anywhere on the window attaches it.

  The whole window rather than a target inside it: somebody dragging a
  screenshot is looking at the screenshot, not at where to put it. On the body
  rather than on a `div`, because that is literally what "anywhere on the
  window" means.

  It was on the `div` below with `role="application"` to satisfy the rule that
  a drag target needs a role. `application` tells a screen reader to stop
  intercepting keys and hand every one of them straight to the page: no arrow
  keys for reading, no headings, no browse mode. That is a contract for a
  canvas nobody can read any other way, and this is a conversation, a list of
  past ones and a text box. The whole window was unreadable so that a `div`
  could listen for a drag.
-->
<svelte:body
  ondragover={(event) => {
    event.preventDefault();
    hovering = true;
  }}
  ondragleave={() => (hovering = false)}
  ondrop={(event) => {
    event.preventDefault();
    hovering = false;
  }}
/>

<div class="window" class:hovering>
  <TitleBar title="AI Chat" />

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
              use:hint={"Forget this conversation"}
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

        <Instead
          tone={standing({ failed: false, loading: false, count: past.length })}
          inline
          headline="Nothing asked yet"
          hint="Conversations you start are kept here."
        />
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
                Set up AI Chat
              </button>
            {/if}
          </div>
        {/if}

        {#each conversation as turn, at (at)}
          {#if turn.role === "user"}
            <article class="turn asked">
              {#if turn.attachments.length}
                <div class="carried">
                  {#each turn.attachments as one (one.name)}
                    {#if one.kind === "image"}
                      <img class="shot" src={one.body} alt={one.name} />
                    {:else}
                      <span class="paper">{one.name}</span>
                    {/if}
                  {/each}
                </div>
              {/if}
              {#if turn.text}<p>{turn.text}</p>{/if}
            </article>
          {:else}
            <Steps steps={turn.steps} />
            <article class="turn said md">
              <Markdown text={turn.text} />

              <!--
                Copy and ask again, on the answer they belong to.

                Quiet until the answer is hovered, because they are about the
                answer rather than part of it, and a row of controls under
                every reply is a conversation with furniture in it.
              -->
              <div class="afters">
                <button onclick={() => void copy(at, turn.text)}>
                  {copied === at ? "Copied" : "Copy"}
                </button>
                {#if at === conversation.length - 1}
                  <button onclick={() => void again()} disabled={asking}>Again</button>
                {/if}
              </div>
            </article>
          {/if}
        {/each}

        {#if asking}
          <Steps {steps} live />
        {/if}

        {#if asked}
          <div class="permission">
            <p class="wants">{asked.title}</p>
            <p class="subject">{asked.subject}</p>
            <p class="touches">This {asked.touches}.</p>
            <!--
              Said out loud when the stronger gate could not run. Without it a
              keypress and a fingerprint look the same from here.
            -->
            {#if asked.instead}
              <p class="instead">{asked.instead}, so pressing Enter is all this asks for.</p>
            {/if}
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
        {#if carrying.length}
          <div class="waiting">
            {#each carrying as one (one.name)}
              <span class="chip">
                {#if one.kind === "image"}
                  <img class="thumb" src={one.body} alt="" />
                {/if}
                <span class="chip-name">{one.name}</span>
                <span class="chip-size">{size(one.bytes)}</span>
                <button
                  class="chip-drop"
                  aria-label={`Remove ${one.name}`}
                  onclick={() => drop(one.name)}>&times;</button
                >
              </span>
            {/each}
          </div>
        {/if}

        <div class="line">
        <button
          class="round attach"
          onclick={() => void pick()}
          aria-label="Attach a file"
          use:hint={"Attach a file"}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden="true">
            <path
              d="M21 11.5l-8.5 8.5a5.5 5.5 0 01-7.8-7.8l8.7-8.7a3.7 3.7 0 015.2 5.2l-8.6 8.6a1.8 1.8 0 01-2.6-2.6l7.9-7.9"
              stroke="currentColor"
              stroke-width="1.7"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </button>

        <textarea
          bind:this={composer}
          bind:value={draft}
          onkeydown={onComposerKey}
          onpaste={onPaste}
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

        {#if asking}
          <!--
            Stopping keeps what has arrived, so it is a plain control rather
            than a destructive one. A square inside, which is what stop has
            meant on every machine since tape.
          -->
          <button class="round stop" onclick={() => void stop()} aria-label="Stop" use:hint={"Stop"}>
            <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
              <rect x="1.5" y="1.5" width="9" height="9" rx="1.5" fill="currentColor" />
            </svg>
          </button>
        {:else}
          <!--
            An arrow, not a key cap.

            The cap was a hint wearing a button's clothes: it named the key
            that does this rather than the thing itself, so it read as a label
            somebody had put in the wrong place. An arrow is what send has
            looked like in every message box for twenty years, and the key is
            still the key.
          -->
          <button
            class="round send"
            onclick={() => void send()}
            disabled={!draft.trim() && carrying.length === 0}
            aria-label="Send"
            use:hint={"Send"}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path
                d="M8 13V3M8 3L3.5 7.5M8 3l4.5 4.5"
                stroke="currentColor"
                stroke-width="1.9"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>
        {/if}
        </div>
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
    font-family: var(--font);
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

  /*
   * Neutral, and its hover is neutral too.
   *
   * The accent means selection, match, focus or an affirmative state, and this
   * is none of them: it is a button that starts a conversation. It wore
   * `--accent-fill` at rest and deepened to `--accent-fill-strong` on hover,
   * which broke the rule twice, and the rest state broke it in the way that
   * shows: `--accent-fill` is also what `.row.open` uses for the conversation
   * being read, four pixels below, so the button looked like a selected row
   * that could not be deselected.
   */
  .fresh {
    flex: none;
    padding: var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--fill-2);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    font-weight: var(--weight-medium);
    cursor: pointer;
    transition: background-color var(--motion-state) var(--ease);
  }

  .fresh:hover:not(:disabled) {
    background: var(--fill-3);
  }

  .fresh:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-half);
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
    gap: var(--space-hair);
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
    transition:
      opacity var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .row:hover .bin,
  .bin:focus-visible {
    opacity: 1;
  }

  .bin:hover {
    color: var(--danger);
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
    box-shadow: var(--ring-accent-faint);
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

  .permission {
    align-self: flex-start;
    max-width: 62ch;
    width: 100%;
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    box-shadow: var(--ring-accent-faint);
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

  /* Quieter than what it touches, which is what the decision is about. */
  .instead {
    margin: calc(var(--space-3) * -1) 0 var(--space-3);
    color: var(--text-3);
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
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
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
    animation: thinking var(--motion-pulse) steps(4, end) infinite;
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
    color: var(--danger);
    font-size: var(--text-meta);
  }

  /* ---------------------------------------------------------------- composer */

  /*
   * What a drop would land on.
   *
   * The whole window rather than a target inside it: somebody dragging a
   * screenshot is looking at the screenshot, not at where to put it.
   */
  .hovering {
    box-shadow: var(--bevel-window), var(--focus-ring-inset);
  }

  .composer {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-5) var(--space-4);
    border-top: 1px solid var(--hairline);
  }

  .line {
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
  }

  /* What is waiting to go with the next question. */
  .waiting {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-snug) var(--space-1) var(--space-snug) var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--fill-1);
    box-shadow: var(--ring);
    font-size: var(--text-meta);
  }

  /* The picture itself, so what is attached is recognisable rather than named. */
  .thumb {
    width: var(--icon-tile-sm);
    height: var(--icon-tile-sm);
    border-radius: var(--radius-sm);
    object-fit: cover;
  }

  .chip-name {
    color: var(--text-1);
    max-width: 24ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip-size {
    color: var(--text-3);
    font-size: var(--text-micro);
  }

  .chip-drop {
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-3);
    font-size: var(--text-body);
    line-height: 1;
    cursor: pointer;
  }

  .chip-drop:hover {
    background: var(--fill-2);
    color: var(--text-1);
  }

  .attach {
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-2);
  }

  .attach:hover {
    background: var(--fill-2);
    color: var(--text-1);
  }

  /*
   * Stopping keeps what has arrived, so it is a plain control rather than a
   * destructive one. Painting it red would make it the button nobody dares
   * press, which is the opposite of what it is for.
   */
  .stop {
    background: var(--fill-2);
    color: var(--text-1);
  }

  .stop:hover {
    background: var(--hairline-strong);
  }

  /* What was handed over with a question, drawn inside its bubble. */
  .carried {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  .carried:last-child {
    margin-bottom: 0;
  }

  .shot {
    max-width: 220px;
    max-height: 160px;
    border-radius: var(--radius-md);
    display: block;
  }

  .paper {
    padding: var(--space-half) var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: var(--text-micro);
  }

  /*
   * Copy and ask again, quiet until the answer is hovered.
   *
   * They are about the answer rather than part of it, and a row of controls
   * under every reply is a conversation with furniture in it.
   */
  .afters {
    display: flex;
    gap: var(--space-1);
    margin-top: var(--space-2);
    opacity: 0;
    transition: opacity var(--motion-state) var(--ease);
  }

  .said:hover .afters,
  .afters:focus-within {
    opacity: 1;
  }

  .afters button {
    padding: var(--space-half) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-3);
    font: inherit;
    font-size: var(--text-micro);
    cursor: pointer;
    transition:
      color var(--motion-state) var(--ease),
      background-color var(--motion-state) var(--ease);
  }

  .afters button:hover:not(:disabled) {
    background: var(--fill-2);
    color: var(--text-1);
  }

  .afters button:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }

  textarea {
    flex: 1;
    min-width: 0;
    /* The height is set from the content by `fit`; these are the ends of that
       range. Past the top it scrolls rather than eating the conversation. */
    min-height: 38px;
    max-height: 200px;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-body);
    line-height: 1.5;
    resize: none;
    outline: none;
  }

  textarea:focus {
    box-shadow: var(--ring-strong);
  }

  textarea::placeholder {
    color: var(--text-3);
  }

  /*
   * Round, and the same size as the paperclip opposite it.
   *
   * The pair reads as a pair: one thing to add something on the left, one
   * thing to send on the right, and a box of text between them. A wide button
   * with a word in it made the composer read as a form with a submit.
   */
  .round {
    flex: none;
    width: 38px;
    height: 38px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease),
      opacity var(--motion-state) var(--ease);
  }

  .round:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  .send {
    background: var(--accent);
    color: var(--core-background);
  }

  .send:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  /*
   * Nothing to send reads as nothing to press.
   *
   * Quieter than a dimmed accent circle, which still draws the eye to a
   * control that will not do anything.
   */
  .send:disabled {
    background: var(--fill-2);
    color: var(--text-3);
    cursor: default;
  }
</style>
