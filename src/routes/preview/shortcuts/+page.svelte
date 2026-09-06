<script lang="ts">
  /**
   * A development route. Reachable with `npm run dev`, never linked from the
   * app, and of no use to anyone running Sill rather than working on it.
   *
   * The shortcuts panel's parts, with nothing running behind them: the
   * keyboard map over a fixture reference, a recorder in each of its states,
   * and the chord component. The settings route itself cannot be opened in a
   * plain browser because every `invoke` is undefined outside the Tauri
   * window; these three take everything they draw as props, so they can be
   * looked at here. The recorders record for real: press keys on one.
   *
   * The chords are fixture data, the same way the wallpapers in the gallery
   * are, and this route is exempt from the guards that refuse literal chords
   * in the real surfaces.
   */
  import "$lib/theme/theme.css";
  import KeyMap from "$lib/components/settings/KeyMap.svelte";
  import KeyRecorder from "$lib/components/settings/KeyRecorder.svelte";
  import Chord from "$lib/components/Chord.svelte";
  import Section from "$lib/components/settings/Section.svelte";
  import Row from "$lib/components/settings/Row.svelte";
  import type { KeySection } from "$lib/exthost/commands";

  const reference: KeySection[] = [
    {
      title: "Opening Sill",
      keys: [{ chord: "Alt+Space", does: "Summon the launcher", changed: false, contested: false, refused: false }],
    },
    {
      title: "From anywhere",
      keys: [
        { chord: "Ctrl+Alt+W", does: "Open the window switcher", changed: false, contested: false, refused: false },
        { chord: "Ctrl+Shift+S", does: "Take a screenshot", changed: false, contested: false, refused: true },
        { chord: "Ctrl+Alt+U", does: "Uppercase", changed: false, contested: false, refused: false },
        { chord: "Ctrl+Alt+Left", does: "Left Half: Left half", changed: false, contested: false, refused: false },
      ],
    },
    {
      title: "Moving around",
      keys: [
        { chord: "Down", does: "Next", changed: false, contested: false, refused: false },
        { chord: "Up", does: "Previous", changed: false, contested: false, refused: false },
        { chord: "Ctrl+J", does: "Next", changed: true, contested: false, refused: false },
        { chord: "Ctrl+K", does: "Previous", changed: true, contested: false, refused: false },
        { chord: "PageDown", does: "Page down", changed: false, contested: false, refused: false },
        { chord: "Enter", does: "Open", changed: false, contested: false, refused: false },
        { chord: "Escape", does: "Back", changed: false, contested: false, refused: false },
      ],
    },
    {
      title: "Acting on a row",
      keys: [
        { chord: "Ctrl+Shift+C", does: "Copy Path", changed: false, contested: false, refused: false },
        { chord: "Ctrl+Shift+C", does: "Copy Name", changed: true, contested: true, refused: false },
        { chord: "Ctrl+Enter", does: "Reveal in Folder", changed: false, contested: false, refused: false },
        { chord: "Ctrl+Shift+Enter", does: "Paste as Plain Text", changed: false, contested: false, refused: false },
        { chord: "Ctrl+D", does: "Move to Recycle Bin", changed: false, contested: false, refused: false },
      ],
    },
  ];

  let picked = $state("");
  let summon = $state("Alt+Space");
  let switcher = $state("Ctrl+Alt+W");
  let capture = $state("Ctrl+Shift+S");
  let next = $state("Ctrl+J");
  let copy = $state("Ctrl+Shift+C");
  let unset = $state("");

  const later = async () => {};
</script>

<main>
  <h1>Shortcuts</h1>
  <p class="lede">
    The parts of the shortcuts panel, with fixture keys and nothing running behind them. The
    recorders record for real: click one and press keys.
  </p>

  <Section
    label="Keyboard"
    description="Choose a modifier above the board, or hold one over it, to see that layer. Hover a lit key. Click one."
    bare
  >
    <KeyMap sections={reference} onpick={(chord) => (picked = chord)} />
    <p class="picked">Picked: {picked || "nothing yet"}</p>
  </Section>

  <Section label="Recorders" description="One control in every state it has.">
    <Row title="Summon" description="A key that cannot be off.">
      {#snippet control()}
        <KeyRecorder chord={summon} scope="hotkey" section="Opening Sill" onsave={async (c) => { summon = c; }} />
      {/snippet}
    </Row>
    <Row title="Window switcher" description="A key that can be taken away.">
      {#snippet control()}
        <KeyRecorder chord={switcher} scope="hotkey" section="From anywhere" onsave={async (c) => { switcher = c; }} onclear={async () => { switcher = ""; }} placeholder="Off" />
      {/snippet}
    </Row>
    <Row title="Screenshot" description="A key Windows refused.">
      {#snippet control()}
        <KeyRecorder chord={capture} scope="hotkey" section="From anywhere" taken onsave={async (c) => { capture = c; }} onclear={async () => { capture = ""; }} placeholder="Off" />
      {/snippet}
    </Row>
    <Row title="Next" description="A movement, set by hand. Backspace gives it back to the preset.">
      {#snippet control()}
        <KeyRecorder chord={next} scope="navigation" section="Moving around" onsave={async (c) => { next = c; }} onreset={async () => { next = "Down"; }} />
      {/snippet}
    </Row>
    <Row title="Copy Name" description="An action key another action takes first.">
      {#snippet control()}
        <KeyRecorder chord={copy} scope="action" section="Acting on a row" contested="Copy Path" onsave={async (c) => { copy = c; }} onreset={later} onclear={async () => { copy = ""; }} />
      {/snippet}
    </Row>
    <Row title="Move to Folder" description="An action with no key.">
      {#snippet control()}
        <KeyRecorder chord={unset} scope="action" section="Acting on a row" onsave={async (c) => { unset = c; }} onreset={later} onclear={async () => { unset = ""; }} placeholder="No key" />
      {/snippet}
    </Row>
  </Section>

  <Section label="Chords" description="One keycap per key, wherever a chord is drawn." bare>
    <div class="chords">
      <Chord chord="Alt+Space" />
      <Chord chord="Ctrl+Shift+Up" />
      <Chord chord="Super+K" />
      <Chord chord="Ctrl+Alt+Delete" />
      <Chord chord="Enter" />
      <Chord chord="Ctrl+Alt" dim />
    </div>
  </Section>
</main>

<style>
  :global(body) {
    margin: 0;
    background: var(--core-secondary-background);
    color: var(--text-1);
    font-family: var(--font);
  }

  main {
    max-width: 880px;
    margin: 0 auto;
    padding: var(--space-8);
  }

  h1 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-title);
    font-weight: var(--weight-strong);
  }

  .lede,
  .picked {
    margin: 0 0 var(--space-6);
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  .picked {
    margin-top: var(--space-2);
  }

  .chords {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
  }
</style>
