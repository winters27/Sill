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

  /** Canvas fills take a literal colour, not a CSS variable. This is
   *  `--accent-bright`, the one token bright enough to read at 1.5px. */
  const ACCENT = "200, 224, 232";

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
    frame = requestAnimationFrame(draw);

    const ctx = el.getContext("2d");
    if (!ctx) return;

    // Everything below is in DEVICE pixels, not logical ones. Drawing 2px
    // dots on a 4.5px pitch through a scaled transform lands every dot on a
    // fraction, and each one antialiases into a smudge.
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(PILL_WIDTH * dpr);
    const h = Math.round(PILL_HEIGHT * dpr);
    if (el.width !== w || el.height !== h) {
      el.width = w;
      el.height = h;
    }
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const intro = Math.min(1, (performance.now() - introStart) / INTRO_MS);
    const wakeAll = easeInOut(intro);

    const dot = Math.max(1, Math.round(DOT_WIDTH * dpr));
    const pitch = Math.round((DOT_WIDTH + DOT_GAP) * dpr);
    const span = (COUNT - 1) * pitch + dot;
    // Snap the row's origin too, so the whole run sits on whole pixels.
    let x = Math.round((w - span) / 2);
    const centerY = h / 2;
    const maxDot = h * DOT_HEIGHT_RATIO;

    for (let i = 0; i < COUNT; i++) {
      const target = listening ? live[i] : 0;
      const k = target > shown[i] ? RISE : FALL;
      shown[i] += (target - shown[i]) * k;
      const v = shown[i];

      // Fade toward both ends so the row dissolves instead of stopping dead.
      const u = i / (COUNT - 1);
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
    <div class="pill">
      {#if status === "transcribing"}
        <span class="label">Transcribing</span>
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
    A flat dark capsule, not a blurred one. `backdrop-filter` blurs what is
    behind the element *within the page*, and there is nothing there; a real
    desktop blur needs a compositor backdrop, which with a transparent tint
    samples the desktop and turns murky grey. Carrying the darkness in the
    fill reads better than either.
  */
  .pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 174px;
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-pill);
    background: var(--shade-5);
    box-shadow: var(--ring-fill), var(--elevation-pill);
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
