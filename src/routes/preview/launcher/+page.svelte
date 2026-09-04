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
  import AiChat, { type Shown } from "$lib/components/AiChat.svelte";
  import ScriptOutput from "$lib/components/ScriptOutput.svelte";
  import type { AiReady } from "$lib/exthost/commands";

  const answersWith = {
    ready: true,
    id: "anthropic",
    name: "Anthropic",
    model: "claude-sonnet-5",
    kind: "remote",
    whyNot: "",
  } as unknown as AiReady;

  const conversation: Shown[] = [
    { role: "user", text: "What windows do I have open?", steps: [] },
    {
      role: "assistant",
      text: "Four: **Zen**, a terminal, Obsidian and Sill's own settings window.\n\n- Zen is on the second display\n- The terminal is running `cargo test`",
      steps: [{ tool: "list_windows", subject: "" } as never],
    },
  ];
</script>

<div class="stage">
  <section>
    <h2>An empty conversation</h2>
    <div class="window">
      <AiChat
        conversation={[]}
        answering=""
        asking={false}
        asked={null}
        steps={[]}
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
        answering=""
        asking={false}
        asked={{
          id: "1",
          title: "Move a file",
          subject: "C:\\Users\\you\\Downloads\\notes.md",
          touches: "moves a file on this machine",
        } as never}
        steps={[]}
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
