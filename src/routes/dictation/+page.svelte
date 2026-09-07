<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getDictationPanelStatus } from "$lib/dictation";
  import { applyAppearance, getPreferences } from "$lib/settings";
  import "$lib/theme/theme.css";

  type PanelStatus = "listening" | "transcribing" | "copied" | "confirming";

  const COUNT = 29;

  /** Rise fast, fall slow. Symmetric smoothing reads as a twitching meter. */
  const RISE = 0.55;
  const FALL = 0.14;
  /** Fraction of each end over which the row fades, so it dissolves rather
   *  than stopping abruptly. */
  const EDGE_FADE = 0.22;
  const DOT_HEIGHT_RATIO = 0.92;
  /** How far ahead of the ends the centre dots wake during the entrance. */
  const REVEAL_LEAD = 0.55;
  const INTRO_MS = 420;

  /** Thin lines, not blocks. Drawn on whole device pixels so they stay sharp
   *  rather than smearing into grey. */
  const DOT_WIDTH = 1.5;
  const DOT_GAP = 3.5;
  const PILL_WIDTH = 142;
  /** Tall enough for the bars to read as a waveform. At 24 the loudest band
   *  reached 22px and the row stayed a dotted line however hard you spoke. */
  const PILL_HEIGHT = 38;

  /**
   * The row's height while the model runs, under the word.
   *
   * Slim because there it is the second thing in the box rather than the only
   * one. At the full 38 the word and the row together stand taller than the
   * pill and one of them has to leave.
   */
  const SCAN_HEIGHT = 13;
  /** How long the pulse takes to cross the row. */
  const SCAN_MS = 1100;
  /** How wide the pulse is, as the squared fraction of the row it covers. */
  const SCAN_SPREAD = 0.012;
  /** How tall the pulse stands at its centre, and where the rest of the row sits. */
  const SCAN_PEAK = 0.85;
  const SCAN_FLOOR = 0.08;
  /** How far past each end the pulse travels, so it arrives and leaves instead
   *  of appearing at the edge. */
  const SCAN_LEAD = 0.2;

  /**
   * The pill's box, one size in every state.
   *
   * The waveform is the tallest thing that goes in it, so the box is that plus
   * the vertical padding, which is `--space-2` at each end. Until this was
   * fixed the word states collapsed the pill from 54 to 31, and because it is
   * centred in its own window it shrank from the top and the bottom at once,
   * so the panel appeared to flinch the moment a recording ended.
   */
  const BOX_PAD_Y = 8;
  const BOX_HEIGHT = PILL_HEIGHT + BOX_PAD_Y * 2;

  /**
   * Canvas fills take a literal colour, not a CSS variable, so the token is
   * read out of the stylesheet each time the panel shows: `--accent-bright`,
   * the one token bright enough to read at 1.5px, in whichever theme is on.
   * The literal is only the value before the first read, and it is the
   * Frost one; five other themes define their own.
   */
  let ACCENT = "200, 224, 232";

  function readAccent(): void {
    const hex = getComputedStyle(document.documentElement)
      .getPropertyValue("--accent-bright")
      .trim();
    const m = /^#([0-9a-f]{6})$/i.exec(hex);
    if (!m) return;
    const n = Number.parseInt(m[1], 16);
    ACCENT = `${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}`;
  }

  let status = $state<PanelStatus>("listening");
  let visible = $state(false);
  let canvas = $state<HTMLCanvasElement>();

  /** Live values from Rust, and the smoothed values actually drawn. */
  let live = new Float32Array(COUNT);
  const shown = new Float32Array(COUNT);
  let introStart = 0;
  let frame = 0;
  let unlisten: UnlistenFn[] = [];

  const listening = $derived(status === "listening");
  const transcribing = $derived(status === "transcribing");

  /**
   * Whether this machine has asked for less movement.
   *
   * Read once, and read here rather than in the stylesheet: the pulse is drawn
   * in script, so a media query would leave the setting applying to nothing.
   */
  const REDUCED_MOTION =
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  function easeInOut(t: number): number {
    return t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
  }

  function draw(): void {
    const el = canvas;
    // Re-armed only while there is something to draw on. Scheduling the next
    // frame first, unconditionally, is what kept this loop running for the
    // entire life of the app: the panel is hidden almost always, the canvas
    // is unmounted with it, and the body below returned immediately while the
    // 60 Hz ping-pong carried on and kept the GPU process awake with it.
    if (!el) {
      frame = 0;
      return;
    }

    // Nothing moves for somebody who asked for less motion, so nothing is
    // redrawn either: one frame stands until the state changes, and a state
    // change swaps the canvas and restarts the loop through the effect below.
    if (!(REDUCED_MOTION && transcribing)) {
      frame = requestAnimationFrame(draw);
    }

    const ctx = el.getContext("2d");
    if (!ctx) return;

    // Everything below is in DEVICE pixels, not logical ones. Drawing 2px
    // dots on a 4.5px pitch through a scaled transform lands every dot on a
    // fraction, and each one antialiases into a smudge.
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(PILL_WIDTH * dpr);
    const h = Math.round((transcribing ? SCAN_HEIGHT : PILL_HEIGHT) * dpr);
    if (el.width !== w || el.height !== h) {
      el.width = w;
      el.height = h;
    }
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const intro = Math.min(1, (performance.now() - introStart) / INTRO_MS);
    const wakeAll = easeInOut(intro);

    /*
     * How far along the row the pulse has reached, while the model runs.
     *
     * Deliberately not a level. Nothing is being heard by then, so a row that
     * still answered to the microphone would be drawing input that does not
     * exist. This says work is happening and claims nothing about what it
     * hears. Parked mid-row when the machine has asked for less motion.
     */
    const head = REDUCED_MOTION
      ? 0.5
      : (1 + SCAN_LEAD * 2) * ((performance.now() / SCAN_MS) % 1) - SCAN_LEAD;

    const dot = Math.max(1, Math.round(DOT_WIDTH * dpr));
    const pitch = Math.round((DOT_WIDTH + DOT_GAP) * dpr);
    const span = (COUNT - 1) * pitch + dot;
    // Snap the row's origin too, so the whole run sits on whole pixels.
    let x = Math.round((w - span) / 2);
    const centerY = h / 2;
    const maxDot = h * DOT_HEIGHT_RATIO;

    for (let i = 0; i < COUNT; i++) {
      const u = i / (COUNT - 1);

      let v: number;
      if (transcribing) {
        // Written straight in rather than smoothed. The pulse is already
        // smooth and already where it belongs, and easing it again would only
        // make it lag behind itself.
        const from = u - head;
        shown[i] = Math.exp(-(from * from) / SCAN_SPREAD) * SCAN_PEAK + SCAN_FLOOR;
        v = shown[i];
      } else {
        const target = listening ? live[i] : 0;
        const k = target > shown[i] ? RISE : FALL;
        shown[i] += (target - shown[i]) * k;
        v = shown[i];
      }

      // Fade toward both ends so the row dissolves instead of stopping dead.
      const e = Math.min(1, Math.max(0, Math.min(u, 1 - u) / EDGE_FADE));
      const edge = e * e * (3 - 2 * e);

      // The reveal spreads from the middle out: centre dots are lit while the
      // ends are still arriving.
      const distance = Math.abs(u - 0.5) * 2;
      const wake = Math.min(
        1,
        Math.max(0, (wakeAll * (1 + REVEAL_LEAD) - distance * REVEAL_LEAD) / 0.35),
      );

      const dh = Math.round(Math.max(dot, v * maxDot) * wake);
      const y = Math.round(centerY - dh / 2);
      // Nearly opaque even at rest: at 22% the dots read as smudges rather
      // than as a sign that something is listening.
      ctx.fillStyle = `rgba(${ACCENT}, ${(0.5 + 0.5 * v) * edge * wake})`;
      ctx.beginPath();
      ctx.roundRect(x, y, dot, Math.max(dot, dh), dot / 2);
      ctx.fill();

      x += pitch;
    }
  }

  /**
   * The loop lives exactly as long as the canvas does.
   *
   * `canvas` is bound by `bind:this` inside the `{#if visible}` block, so it
   * is an element while the meter is on screen and undefined the rest of the
   * time. Keying the loop to it means the panel costs nothing at all when
   * nobody is dictating, which is most of the time the app is running.
   */
  $effect(() => {
    if (!canvas) return;

    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  });

  onMount(() => {
    introStart = performance.now();

    (async () => {
      // Its own webview, so it does not inherit the launcher's font choice
      // and has to ask for it.
      try {
        applyAppearance(await getPreferences());
        readAccent();
      } catch {
        // The bundled default is fine; a panel that refuses to draw because
        // it could not read a font setting would be worse.
      }

      // This window is declared hidden, so its webview can miss the very
      // first status event. Recover whatever Rust holds before listening.
      try {
        const current = await getDictationPanelStatus();
        if (current && current !== "") {
          status = current as PanelStatus;
          visible = true;
        }
      } catch (err) {
        console.error("[sill] could not read the dictation panel status:", err);
      }

      try {
        unlisten.push(
          await listen<PanelStatus>("dictation:status", (event) => {
            if (!visible) introStart = performance.now();
            status = event.payload;
            visible = true;
          }),
        );
        unlisten.push(
          await listen<number[]>("dictation:bands", (event) => {
            live = Float32Array.from(event.payload);
          }),
        );
        unlisten.push(
          await listen("dictation:hide", () => {
            visible = false;
            live = new Float32Array(COUNT);
            shown.fill(0);
          }),
        );
      } catch (err) {
        console.error("[sill] dictation panel listeners failed:", err);
      }
    })();
  });

  onDestroy(() => {
    cancelAnimationFrame(frame);
    for (const off of unlisten) off();
    unlisten = [];
  });
</script>

<div class="root">
  {#if visible}
    <div class="pill" style:height="{BOX_HEIGHT}px">
      {#if status === "transcribing"}
        <!-- The one state that is work in progress, drawn as such. The word
             says which state it is; the row under it says the work is still
             running, which puts the movement at the size of the panel rather
             than at the size of a twelve-pixel word. -->
        <div class="stack">
          <span class="label">Transcribing</span>
          <canvas
            bind:this={canvas}
            class="dots"
            style:width="{PILL_WIDTH}px"
            style:height="{SCAN_HEIGHT}px"
          ></canvas>
        </div>
      {:else if status === "copied"}
        <span class="label">Copied</span>
      {:else if status === "confirming"}
        <span class="label warn">Press again to discard</span>
      {:else}
        <canvas
          bind:this={canvas}
          class="dots"
          style:width="{PILL_WIDTH}px"
          style:height="{PILL_HEIGHT}px"
        ></canvas>
      {/if}
    </div>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    background: transparent;
    overflow: hidden;
  }

  .root {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100vw;
    height: 100vh;
    background: transparent;
  }

  /*
    Glass painted into the fill, not blurred through it. `backdrop-filter`
    blurs what is behind the element *within the page*, and there is nothing
    there; a real desktop blur needs a compositor backdrop, which with a
    transparent tint samples the desktop and turns murky grey. So the sheet is
    drawn instead: a dark fill, `--sheen` for the light falling down it, and
    `--bevel-tile` for the edge, that being the pair of opposing insets the
    palette note calls the thing that gives an edge the thickness of glass. A
    single ring is the same weight on all four sides and reads as an outline.
  */
  .pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 174px;
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-pill);
    background-image: var(--sheen);
    background-color: var(--shade-5);
    box-shadow: var(--bevel-tile), var(--elevation-pill);
  }

  /* The word, and the row running underneath it. */
  .stack {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
  }

  .dots {
    display: block;
    flex-shrink: 0;
  }

  .label {
    font-family: var(--font);
    font-size: var(--text-meta);
    font-weight: var(--weight-medium);
    letter-spacing: 0.02em;
    color: var(--accent-bright);
    white-space: nowrap;
  }

  /* The one destructive state gets the one colour that says so. */
  .warn {
    color: var(--danger);
  }

</style>
