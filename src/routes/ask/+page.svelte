<script lang="ts">
  /**
   * A conversation, with room.
   *
   * The launcher is the quick lane: one question, one answer, gone in fifteen
   * seconds. This is where you stay. It is not a second chat implementation:
   * the turns, the timeline, the thinking, the card and the wait are the same
   * components the launcher draws, at a size where they can breathe and next
   * to everything asked before.
   *
   * ## The frame
   *
   * The conversation is a measured column of prose on the window's glass.
   * Around it, two things: a rail that lists what was asked by the day it was
   * asked and says who answers, and one raised card to type into. Nothing
   * else in the window carries depth, which is what lets those two read.
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
  import { onMount, tick } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  import TitleBar from "$lib/components/TitleBar.svelte";
  import Instead from "$lib/components/Instead.svelte";
  import AiMark from "$lib/components/settings/AiMark.svelte";
  import ApprovalCard from "$lib/components/chat/ApprovalCard.svelte";
  import Composer from "$lib/components/chat/Composer.svelte";
  import Opening from "$lib/components/chat/Opening.svelte";
  import Trouble from "$lib/components/chat/Trouble.svelte";
  import Turn from "$lib/components/chat/Turn.svelte";
  import Waiting from "$lib/components/chat/Waiting.svelte";
  import { standing } from "$lib/instead";
  import { open as pickFiles } from "@tauri-apps/plugin-dialog";
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
    aiSpent,
    aiLimits,
    aiStop,
    aiTranscript,
    type AiAttached,
    type AiConversation,
    type AiLimits,
    type AiReady,
  } from "$lib/exthost/commands";
  import { applyAppearance, getPreferences, openSettings, type Preferences } from "$lib/settings";
  import { forgetUnreadable, orElse, silently } from "$lib/status";
  import { hint } from "$lib/hint";
  import { begin, fresh, reset, textOf, type Live } from "$lib/chat/live";
  import { listenToChat } from "$lib/chat/listen";
  import { fromQuestion, fromTurn, type Shown } from "$lib/chat/parts";
  import { follow } from "$lib/chat/follow";
  import { byDay, narrow, whereFrom } from "$lib/chat/rail";

  let conversation = $state<Shown[]>([]);
  let past = $state<AiConversation[]>([]);
  let answersWith = $state<AiReady | null>(null);

  /** The turn in flight: what has arrived, the card, the trouble. */
  let live = $state<Live>(fresh());

  let draft = $state("");
  /** What is waiting to go with the next question. */
  let carrying = $state<AiAttached[]>([]);
  /** Whether something is being dragged over the window right now. */
  let hovering = $state(false);
  /** What is typed into the rail's well, narrowing the list. */
  let finding = $state("");

  /**
   * The clock the rail reads ages against.
   *
   * Ticking rather than baked in when the list is fetched. A window left open
   * for an hour showed every conversation as it was an hour ago, because the
   * age was a string worked out once and never again.
   */
  let now = $state(Math.floor(Date.now() / 1000));
  let field = $state<HTMLTextAreaElement | null>(null);
  let transcript = $state<HTMLDivElement | null>(null);

  /** Which conversation is open, so the list can mark it. */
  const openId = $derived(past.find((one) => one.open)?.id ?? "");

  /** The open conversation names the window; nothing open, the window does. */
  const title = $derived(past.find((one) => one.open)?.title || "AI Chat");

  /** When the list was last fetched, so its ages can be anchored. */
  let fetched = $state(Math.floor(Date.now() / 1000));

  /** The rail's rows: narrowed by the well, grouped by the day they were spoken to. */
  const groups = $derived(byDay(narrow(past, finding), fetched, now));

  /**
   * The answer being written, as a turn.
   *
   * Drawn at the end of the same list as the finished turns and keyed by its
   * position, so when it is committed the element that was writing it keeps
   * writing at its pace rather than being replaced by one that draws it whole.
   */
  const writing = $derived<Shown | null>(
    live.parts.length
      ? { role: "assistant", text: textOf(live.parts), parts: live.parts, attachments: [] }
      : null,
  );

  const shown = $derived(writing ? [...conversation, writing] : conversation);

  /** Nothing said yet and nothing on its way: the stage rather than a transcript. */
  const empty = $derived(conversation.length === 0 && !live.asking && !writing);

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

    live.trouble = refused.join(" ");
  }

  async function pick() {
    const chosen = await pickFiles({ multiple: true });
    if (!chosen) return;
    await take(Array.isArray(chosen) ? chosen : [chosen]);
    field?.focus();
  }

  /** The ceilings, read once from the one place that defines them. */
  let ceiling = $state<AiLimits>({ image: 4 * 1024 * 1024, text: 100_000 });

  /** A size somebody would say out loud. */
  function size(bytes: number): string {
    if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
    return `${bytes} bytes`;
  }

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
        live.trouble = `That picture is ${size(picture.size)}, and one has to be under ${size(ceiling.image)} to send.`;
        continue;
      }

      const body = await new Promise<string>((done, fail) => {
        const reader = new FileReader();
        reader.onload = () => done(String(reader.result));
        reader.onerror = () => fail(reader.error);
        reader.readAsDataURL(picture);
      }).catch(() => "");

      if (!body) {
        live.trouble = `That picture could not be read, so it was not attached.`;
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

  async function refreshList() {
    try {
      past = await aiConversations();
      fetched = Math.floor(Date.now() / 1000);
    } catch (err) {
      live.trouble = `${err}`;
    }
  }

  /** To the bottom, once the DOM has what was just added. */
  async function toTheEnd() {
    await tick();
    if (transcript) transcript.scrollTop = transcript.scrollHeight;
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
    if ((!question && carrying.length === 0) || live.asking) return;

    const starting = conversation.length === 0;
    const going = carrying;

    conversation = [...conversation, fromQuestion(question, going)];
    draft = "";
    carrying = [];
    begin(live);
    void toTheEnd();

    try {
      await (starting ? aiAsk(question, going) : aiFollowUp(question, going));
    } catch (err) {
      live.trouble = `${err}`;
      live.asking = false;
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
    if (live.asking) return;

    const last = [...conversation].reverse().find((turn) => turn.role === "user");
    if (!last) return;

    draft = last.text;
    carrying = last.attachments;
    await send();
  }

  async function stop() {
    await aiStop();
  }

  /** An example, put in the field rather than sent. */
  function offer(question: string) {
    draft = question;
    field?.focus();
  }

  async function open(id: string) {
    if (live.asking) return;

    try {
      conversation = (await aiResume(id)).map(fromTurn);
    } catch (err) {
      live.trouble = `${err}`;
      return;
    }

    reset(live);
    // After the reset, which forgets the total of the one just left.
    live.spent = await aiSpent().catch(() => null);
    await refreshList();
    void toTheEnd();
    field?.focus();
  }

  async function beginAnother() {
    if (live.asking) return;

    await aiNew();
    conversation = [];
    reset(live);
    await refreshList();
    field?.focus();
  }

  async function forget(id: string) {
    try {
      past = await aiForget(id);
    } catch (err) {
      live.trouble = `${err}`;
      return;
    }

    // The one on screen is the one that went, so the screen follows.
    if (id === openId || !past.some((one) => one.open)) {
      conversation = (await aiTranscript()).map(fromTurn);
      live.spent = await aiSpent().catch(() => null);
    }
  }

  function decide(allowed: boolean) {
    if (!live.asked) return;
    void aiDecide(live.asked.id, allowed);
    live.asked = null;
  }

  /**
   * Ctrl N starts another conversation, the same key as in the launcher.
   *
   * On the window rather than the field, so it works from the rail too.
   */
  function onWindowKey(event: KeyboardEvent) {
    if (event.key.toLowerCase() === "n" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void beginAnother();
    }
  }

  /** How many answers, for the count at the end of a row. */
  function replies(one: AiConversation): string {
    return one.replies === 1 ? "1 reply" : `${one.replies} replies`;
  }

  /*
   * The clock, moved once a minute.
   *
   * The coarsest thing that keeps every row true, because nothing here is
   * measured in seconds after the first one. A window nobody is looking at
   * costs one wakeup a minute, which is the smallest honest price for a list
   * that does not lie about which day things happened on.
   */
  $effect(() => {
    const ticking = setInterval(() => (now = Math.floor(Date.now() / 1000)), 60_000);
    return () => clearInterval(ticking);
  });

  onMount(() => {
    let dropped: UnlistenFn | undefined;
    let heard: UnlistenFn | undefined;
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
      // has changed them, so nothing on screen becomes untrue.
      ceiling = await aiLimits().catch(silently(ceiling));

      answersWith = await aiReady().catch(
        orElse("ask", "whether anything is set up to answer", null, "ai"),
      );

      conversation = (await aiTranscript()).map(fromTurn);
      live.spent = await aiSpent().catch(() => null);
      await refreshList();
      void toTheEnd();

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

      /*
       * Everything about the turn in flight, heard into `live`.
       *
       * The end of a turn is the one thing left to this window: the turn
       * joins the list, and the rail's counts move.
       */
      heard = await listenToChat(live, {
        done(turn) {
          if (turn) conversation = [...conversation, turn];
          void refreshList();
        },
        failed(turn) {
          // Half an answer is often enough to see what went wrong.
          if (turn) conversation = [...conversation, turn];
        },
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
      live.asked ??= await aiOutstanding().catch(
        orElse("ask", "the question waiting to be answered", null, "ai"),
      );

      changed = await listen<Preferences>("sill://preferences-changed", async ({ payload }) => {
        applyAppearance(payload);
        answersWith = await aiReady().catch(
          orElse("ask", "whether anything is set up to answer", null, "ai"),
        );
      });

      field?.focus();
    })();

    return () => {
      dropped?.();
      heard?.();
      changed?.();
      // A card nobody answered would otherwise hold its turn open long after
      // this window is gone.
      void aiRefusePending();
    };
  });
</script>

<svelte:window onkeydown={onWindowKey} />

<!--
  Dropping a file anywhere on the window attaches it.

  The whole window rather than a target inside it: somebody dragging a
  screenshot is looking at the screenshot, not at where to put it. On the body
  rather than on a `div`, because that is literally what "anywhere on the
  window" means, and because `role="application"` on a div would tell a screen
  reader to stop reading the page.
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
  <TitleBar {title} />

  <div class="body">
    <aside class="rail">
      <!-- A well to narrow the list, quiet until it is needed. -->
      <label class="well">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <path
            d="M10.5 18a7.5 7.5 0 1 0 0-15 7.5 7.5 0 0 0 0 15ZM16 16l5 5"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          />
        </svg>
        <input type="search" bind:value={finding} placeholder="Find a conversation" aria-label="Find a conversation" />
      </label>

      <!--
        A row, not a button. Neutral, because the accent means selection and
        this is the thing that starts something; and it wears the key that
        does the same, so the key is learnable from the thing it does.
      -->
      <button class="fresh" onclick={() => void beginAnother()} disabled={live.asking}>
        <span class="plus" aria-hidden="true">+</span>
        <span>New conversation</span>
        <span class="sill-key" aria-hidden="true">Ctrl N</span>
      </button>

      <div class="list sill-scrolls">
        {#each groups as group (group.label)}
          <p class="day">{group.label}</p>
          {#each group.rows as one (one.id)}
            <div class="row" class:open={one.id === openId}>
              <button class="pick" onclick={() => void open(one.id)} disabled={live.asking}>
                <span class="what">{one.title}</span>
                <span class="count" use:hint={replies(one)}>{one.replies}</span>
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
        {/each}

        <Instead
          tone={standing({ failed: false, loading: false, count: groups.length })}
          inline
          headline={finding.trim() ? "Nothing by that name" : "Nothing asked yet"}
          hint={finding.trim() ? "" : "Conversations you start are kept here."}
        />
      </div>

      <!--
        Who answers, at the foot of the rail.

        The mark, the model, and where it runs. Pressing it goes to where
        that is chosen. Until this the window never said which model you were
        talking to, and switching meant knowing to open Settings.
      -->
      <button
        class="who"
        class:unset={!answersWith?.ready}
        onclick={() => void openSettings("ai")}
        use:hint={answersWith?.ready ? "Change who answers" : (answersWith?.whyNot ?? "Set up AI Chat")}
      >
        {#if answersWith?.ready}
          <span class="pip" aria-hidden="true"></span>
          <AiMark name={answersWith.id} size={14} />
          <span class="model">{answersWith.model || answersWith.name}</span>
          <span class="where">{whereFrom(answersWith)}</span>
        {:else}
          <span class="model">Set up AI Chat</span>
        {/if}
      </button>
    </aside>

    <main class="pane">
      <div class="transcript sill-scrolls" bind:this={transcript} use:follow={live.asking}>
        <div class="flow" class:staged={empty}>
          {#if empty}
            <Opening
              stage
              {answersWith}
              onoffer={offer}
              onsetup={() => void openSettings("ai")}
            />
          {/if}

          {#each shown as turn, at (at)}
            <Turn
              {turn}
              live={at === shown.length - 1 && writing !== null}
              onagain={at === shown.length - 1 && turn.role === "assistant" && !writing
                ? again
                : undefined}
              busy={live.asking}
            />
          {/each}

          {#if live.asked}
            <ApprovalCard asked={live.asked} ondecide={decide} />
          {/if}

          {#if live.asking && !writing && !live.asked}
            <Waiting />
          {/if}

          {#if live.trouble}
            <Trouble why={live.trouble} onagain={again} busy={live.asking} />
          {/if}
        </div>
      </div>

      <Composer
        bind:draft
        bind:carrying
        bind:field
        asking={live.asking}
        asked={live.asked}
        first={conversation.length === 0}
        {answersWith}
        {live}
        onsend={() => void send()}
        onstop={() => void stop()}
        onpick={() => void pick()}
        onpaste={onPaste}
        ondecide={decide}
        onsettings={() => void openSettings("ai")}
      />
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

  /* ----------------------------------------------------------------- the rail */

  .rail {
    width: 232px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-3);
    border-right: 1px solid var(--hairline);
  }

  .well {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-height);
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: var(--well);
    color: var(--text-3);
  }

  .well:focus-within {
    box-shadow: var(--well), var(--ring-strong);
  }

  .well input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    outline: none;
  }

  .well input::placeholder {
    color: var(--text-3);
  }

  .well input::-webkit-search-cancel-button {
    display: none;
  }

  .fresh {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-height);
    padding: 0 var(--space-2) 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    font-weight: var(--weight-medium);
    text-align: left;
    cursor: pointer;
    transition: background-color var(--motion-state) var(--ease);
  }

  .fresh:hover:not(:disabled) {
    background: var(--fill-1);
  }

  .fresh:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }

  .fresh .plus {
    width: var(--icon-tile-xs);
    height: var(--icon-tile-xs);
    display: grid;
    place-items: center;
    border-radius: var(--radius-xs);
    background: var(--fill-2);
    color: var(--text-2);
    font-size: var(--text-body);
    line-height: 1;
  }

  .fresh .sill-key {
    margin-left: auto;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-half);
  }

  /* Which day, said once above its rows. */
  .day {
    margin: var(--space-2) 0 var(--space-half) var(--space-3);
    color: var(--text-3);
    font-size: var(--text-micro);
    font-weight: var(--weight-strong);
    letter-spacing: var(--track-label);
    text-transform: uppercase;
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
    box-shadow: var(--catch);
  }

  .pick {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 32px;
    padding: 0 var(--space-2) 0 var(--space-3);
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
    flex: 1;
    min-width: 0;
    color: var(--text-2);
    font-size: var(--text-meta);
    /* One line. A question can be a paragraph, and a list of paragraphs is
       not a list. */
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row.open .what,
  .row:hover .what {
    color: var(--text-1);
  }

  .count {
    flex: none;
    color: var(--text-3);
    font-size: var(--text-micro);
    font-variant-numeric: tabular-nums;
  }

  /*
   * Forgetting one, kept quiet until the row is under the pointer.
   *
   * A delete that is always visible on every row in a list is a delete
   * somebody eventually hits by accident.
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

  .who {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 34px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    /* The same quiet surface as the composer: the panel tint and one light
       catch, so it reads as part of the rail rather than a button on it. */
    background: var(--tint-panel);
    box-shadow: var(--catch);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .who:hover {
    background: var(--fill-1);
    color: var(--text-1);
  }

  .who:focus-visible {
    outline: none;
    box-shadow: var(--catch), var(--ring-accent);
  }

  /* Live, in the one colour that means something is answering. */
  .who .pip {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--success);
  }

  .who .model {
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .who .where {
    margin-left: auto;
    flex: none;
    color: var(--text-3);
    font-size: var(--text-micro);
  }

  .who.unset .model {
    color: var(--accent);
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
  }

  /*
   * One measured column, centred on the glass.
   *
   * 74ch is where a line stops being comfortable to read; the rest of the
   * pane is margin, which is what makes prose read as a page rather than a
   * log against the left edge. The follow watches this element grow.
   */
  .flow {
    width: min(74ch, 100%);
    box-sizing: border-box;
    margin: 0 auto;
    padding: var(--space-6) var(--space-5) var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  /* The empty stage takes the whole height, so its centre is the pane's. */
  .staged {
    flex: 1;
  }

  /*
   * What a drop would land on.
   *
   * The whole window rather than a target inside it: somebody dragging a
   * screenshot is looking at the screenshot, not at where to put it.
   */
  .hovering {
    box-shadow: var(--bevel-window), var(--focus-ring-inset);
  }
</style>
