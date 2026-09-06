<script lang="ts">
  /**
   * A development route. Reachable with `npm run dev`, never linked from the
   * app, and of no use to anyone running Sill rather than working on it.
   *
   * The two surfaces P5-07 lifted out of `+page.svelte` that draw only in a
   * mode the launcher cannot be put into outside Tauri: the conversation and
   * a script's output. Both take everything they draw as props, so both can be
   * looked at here with nothing running behind them.
   *
   * The switcher's strip is deliberately not here. It asks Rust for a picture
   * of a window on mount, which is a thing this page has no answer for.
   */
  import "$lib/theme/theme.css";
  import AiChat from "$lib/components/AiChat.svelte";
  import { fresh } from "$lib/chat/live";
  import type { Shown } from "$lib/chat/parts";
  import ScriptOutput from "$lib/components/ScriptOutput.svelte";
  import Welcome from "$lib/components/Welcome.svelte";
  import RootList from "$lib/components/RootList.svelte";
  import type { AiReady, Welcome as Greeting } from "$lib/exthost/commands";

  const answersWith = {
    ready: true,
    id: "anthropic",
    name: "Anthropic",
    model: "claude-sonnet-5",
    kind: "remote",
    whyNot: "",
  } as unknown as AiReady;

  /*
   * The welcome in its two states, copied out of what Rust returns.
   *
   * A first run happens once per machine, so without this the only way to look
   * at the screen everybody meets first is to throw away a profile. Fixtures
   * rather than a call: the point of looking at these side by side is the
   * difference between them, and a machine can only be in one of them.
   *
   * The chords here are fixture data, the same way the wallpapers in the
   * gallery are. Nothing in this route ships.
   */
  const KEY_WORKS: Greeting = {
    opening: {
      headline: "Press Alt+Space to open Sill",
      body: "Press it again to put Sill away. There is no taskbar button, so Sill stays out of the way until you ask for it.",
    },
    summonTaken: false,
    tray: {
      headline: "The icon in the notification area is Sill",
      body: "Click it to open the launcher and right click it for a menu. Its label is also where Sill says so when something it tried to do did not work.",
    },
    steps: [
      {
        id: "folders",
        title: "Choose which folders Sill searches",
        subtitle:
          "C:\\Users\\someone is indexed to start with. Add more, or point Sill somewhere else, in Settings.",
        does: "chooseFolders",
      },
      {
        id: "everything",
        title: "Search whole drives too",
        subtitle:
          "Sill searches the folders it indexes. It also asks Everything, a separate file indexer, whenever that is running, and it is not on this machine.",
        does: "chooseFolders",
      },
      {
        id: "keys",
        title: "See every key Sill answers to",
        subtitle:
          "Built from the keys that really are bound, so nothing on it is a key that does nothing.",
        does: "showKeys",
      },
      { id: "start", title: "Start searching", subtitle: "Escape does the same.", does: "finish" },
    ],
  };

  /** The case this whole screen exists for, and the one live on this machine. */
  const KEY_TAKEN: Greeting = {
    ...KEY_WORKS,
    opening: {
      headline: "Alt+Space belongs to another application",
      body: "Sill asked Windows for Alt+Space when it started and was refused, so that combination does nothing here. Choose a different one below.",
    },
    summonTaken: true,
    steps: [
      {
        id: "key",
        title: "Choose a key that is free",
        subtitle:
          "Settings opens on the row that sets it, and the new key takes effect straight away.",
        does: "chooseKey",
      },
      ...KEY_WORKS.steps,
    ],
  };

  let works = $state(0);
  let taken = $state(0);

  const conversation: Shown[] = [
    { role: "user", text: "What windows do I have open?", parts: [], attachments: [] },
    {
      role: "assistant",
      text: "Four: **Zen**, a terminal, Obsidian and Sill's own settings window.\n\n- Zen is on the second display\n- The terminal is running `cargo test`",
      parts: [
        { kind: "thinking", text: "The question is about open windows, so list them.", ms: 1240 },
        { kind: "step", id: "call_1", tool: "list_windows", subject: "", ok: true },
        {
          kind: "text",
          text: "Four: **Zen**, a terminal, Obsidian and Sill's own settings window.\n\n- Zen is on the second display\n- The terminal is running `cargo test`",
        },
      ],
      attachments: [],
    },
  ];

  /**
   * A turn part way through, for the pieces that only exist while one is
   * being written: the wait, the open timeline, the thinking as it arrives.
   */
  const writing = {
    ...fresh(),
    asking: true,
    parts: [
      { kind: "thinking" as const, text: "Two folders to look in, Downloads first." },
      { kind: "step" as const, id: "call_2", tool: "list_directory", subject: "C:\\Users\\you\\Downloads", ok: true },
      { kind: "step" as const, id: "call_3", tool: "find_files", subject: "*.iso" },
    ],
  };
</script>

<div class="stage">
  <section>
    <h2>A first run where the summon key registered</h2>
    <div class="window">
      <Welcome
        said={KEY_WORKS}
        selected={works}
        onselect={(i) => (works = i)}
        onrun={(i) => (works = i)}
      />
    </div>
  </section>

  <section>
    <h2>A first run where another application already had the summon key</h2>
    <div class="window">
      <Welcome
        said={KEY_TAKEN}
        selected={taken}
        onselect={(i) => (taken = i)}
        onrun={(i) => (taken = i)}
      />
    </div>
  </section>

  <!--
    The two silences an empty root list can be.

    Side by side because the whole point is that they look identical on a real
    machine and mean opposite things: one is worth waiting a second for, the
    other is worth retyping. The second is what somebody got on their first run
    until `P5-08`, in the first minute of using the application.
  -->
  <section>
    <h2>Nothing found while the first scan is still running</h2>
    <div class="window short">
      <RootList
        commands={[]}
        selected={0}
        query="chrome"
        building={true}
        onselect={() => {}}
        onrun={() => {}}
      />
    </div>
  </section>

  <section>
    <h2>Nothing found once the index is built</h2>
    <div class="window short">
      <RootList
        commands={[]}
        selected={0}
        query="chrome"
        building={false}
        onselect={() => {}}
        onrun={() => {}}
      />
    </div>
  </section>

  <section>
    <h2>An empty conversation</h2>
    <div class="window">
      <AiChat
        conversation={[]}
        live={fresh()}
        {answersWith}
        ondecide={() => {}}
        onoffer={() => {}}
      />
    </div>
  </section>

  <section>
    <h2>Waiting for the first thing to arrive</h2>
    <div class="window">
      <AiChat
        conversation={[conversation[0]]}
        live={{ ...fresh(), asking: true }}
        {answersWith}
        ondecide={() => {}}
        onoffer={() => {}}
      />
    </div>
  </section>

  <section>
    <h2>An answer being written: thinking, then two tools, one still running</h2>
    <div class="window">
      <AiChat
        conversation={[conversation[0]]}
        live={writing}
        {answersWith}
        ondecide={() => {}}
        onoffer={() => {}}
      />
    </div>
  </section>

  <section>
    <h2>A conversation, with a card waiting on a decision</h2>
    <div class="window">
      <AiChat
        {conversation}
        live={{
          ...fresh(),
          asked: {
            id: "1",
            title: "Move a file",
            subject: "C:\\Users\\you\\Downloads\\notes.md",
            touches: "moves a file on this machine",
          },
        }}
        {answersWith}
        ondecide={() => {}}
        onoffer={() => {}}
      />
    </div>
  </section>

  <section>
    <h2>A script, while it is running and once it has failed</h2>
    <div class="window short">
      <ScriptOutput
        output={{
          job: "1",
          title: "Rebuild the index",
          running: true,
          stdout: "",
          stderr: "",
          code: null,
          ended: "finished",
        }}
      />
    </div>
    <div class="window short">
      <ScriptOutput
        output={{
          job: "2",
          title: "Rebuild the index",
          running: false,
          stdout: "read 1,204 entries\nwrote index.json",
          stderr: "warning: two entries share an id",
          code: 3,
          ended: "finished",
        }}
      />
    </div>
  </section>
</div>

<style>
  .stage {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    padding: var(--space-5);
    background: var(--surface-base);
    min-height: 100vh;
  }

  h2 {
    margin: 0 0 var(--space-3);
    color: var(--text-2);
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
  }

  .window {
    display: flex;
    flex-direction: column;
    width: 720px;
    height: 380px;
    border-radius: var(--radius-window);
    background: var(--core-secondary-background);
    box-shadow: var(--ring);
    overflow: hidden;
  }

  .short {
    height: 220px;
    margin-bottom: var(--space-3);
  }
</style>
