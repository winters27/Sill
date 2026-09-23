<script lang="ts">
  /**
   * Confetti, over everything, for a few seconds.
   *
   * A window of its own because it has to be drawn over whatever was in
   * front, and the launcher has just been put away. Transparent, on top,
   * and ignoring the mouse, so it is a picture rather than a thing. The
   * arithmetic is `$lib/confetti`; this only draws it and, when every piece
   * has fallen off the bottom, asks Rust to put the window away so its
   * renderer can sleep. Nothing here runs between bursts.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { burst, flip, settled, step, type Particle } from "$lib/confetti";
  import "$lib/theme/theme.css";

  let canvas = $state<HTMLCanvasElement | null>(null);
  let running = false;

  /** The two faces of one piece of paper. */
  interface Face {
    front: string;
    back: string;
  }

  /**
   * A colour written as `rgb(r, g, b)`, whatever it was declared as.
   *
   * A custom property hands back its computed token stream, which is a hex
   * in one theme and an `rgb()` in the next. Canvas takes either as a fill,
   * but neither can be darkened without numbers, so the engine is asked to
   * resolve it once rather than parsing colour syntax here.
   */
  function resolved(colour: string): string {
    const probe = document.createElement("span");
    probe.style.color = colour;
    document.body.append(probe);
    const value = getComputedStyle(probe).color;
    probe.remove();
    return value;
  }

  /** The same colour with the light taken out of it. */
  function darker(colour: string, by: number): string {
    const parts = colour.match(/[\d.]+/g);
    if (!parts || parts.length < 3) return colour;
    const [r, g, b] = parts.slice(0, 3).map((n) => Math.round(Number(n) * by));
    return `rgb(${r}, ${g}, ${b})`;
  }

  /**
   * The colours, read from the theme rather than chosen here: the accent
   * and the three states are the four colours Sill already has, so confetti
   * looks like it belongs to the same launcher.
   *
   * Each becomes a pair. The back is the same hue with the light taken out,
   * so a piece turning over reads as one object catching the light rather
   * than as two pieces swapping places.
   */
  function palette(): Face[] {
    const style = getComputedStyle(document.documentElement);
    const named = ["--accent", "--success", "--warning", "--info"]
      .map((name) => style.getPropertyValue(name).trim())
      .filter(Boolean);
    const source = named.length ? named : [style.color];

    return source.map((colour) => {
      const front = resolved(colour);
      return { front, back: darker(front, 0.6) };
    });
  }

  function draw(context: CanvasRenderingContext2D, pieces: Particle[], colours: Face[]) {
    context.clearRect(0, 0, innerWidth, innerHeight);

    for (const piece of pieces) {
      const face = colours[piece.colour % colours.length];
      context.save();
      context.translate(piece.x, piece.y);

      if (piece.paper) {
        /*
         * The tumble, drawn rather than simulated.
         *
         * Squashing the long side by the same cosine the physics swayed
         * with is what a rotation about the long axis looks like from the
         * front, and it costs one multiply instead of a transform. At zero
         * the piece is edge-on and draws nothing, which is the flicker that
         * makes a rectangle read as paper.
         */
        const turn = flip(piece);
        context.rotate(piece.angle);
        context.fillStyle = turn > 0 ? face.front : face.back;
        context.fillRect(
          -piece.width / 2,
          (-piece.size * turn) / 2,
          piece.width,
          piece.size * turn,
        );
      } else {
        // A sequin has no face to turn, so it takes the darker one and
        // stays that colour the whole way down.
        context.fillStyle = face.back;
        context.beginPath();
        context.arc(0, 0, piece.size / 2, 0, Math.PI * 2);
        context.fill();
      }

      context.restore();
    }
  }

  function play() {
    if (running || !canvas) return;

    // Asked for less motion: a screen full of falling paper is the most
    // motion Sill has, and the stylesheet's reduced-motion rule cannot reach
    // a canvas. Put straight away instead, so nothing is left over the screen.
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
      void invoke("finish_confetti");
      return;
    }
    const context = canvas.getContext("2d");
    if (!context) return;

    running = true;
    const scale = devicePixelRatio || 1;
    canvas.width = Math.round(innerWidth * scale);
    canvas.height = Math.round(innerHeight * scale);
    context.setTransform(scale, 0, 0, scale, 0, 0);

    const colours = palette();
    /*
     * Enough pieces to read as a burst at whatever size the screen is.
     *
     * A flat 180 was tuned against one display and is thin on a larger
     * one: the count that fills a laptop is scattered over a 27in at
     * 1440p, where the same two fountains have four times the area to
     * cover. Capped, because past a point more pieces is more work for a
     * picture nobody is counting.
     */
    const count = Math.round(Math.min(420, 140 + (innerWidth * innerHeight) / 9000));
    const pieces = burst(innerWidth, innerHeight, count);
    let last = performance.now();

    const frame = (now: number) => {
      // Capped, so a frame that arrives late after the window was shown does
      // not fling everything off screen in one step.
      const dt = Math.min((now - last) / 1000, 0.05);
      last = now;

      step(pieces, dt);
      draw(context, pieces, colours);

      if (settled(pieces, innerHeight)) {
        context.clearRect(0, 0, innerWidth, innerHeight);
        running = false;
        void invoke("finish_confetti");
        return;
      }

      requestAnimationFrame(frame);
    };

    requestAnimationFrame(frame);
  }

  onMount(() => {
    // Told, rather than starting on mount: the window is built once and
    // shown for every burst after the first.
    const off = listen("sill://confetti", () => play());

    /*
     * The first burst, which that event cannot carry.
     *
     * `throw_confetti` builds this window and emits in the next breath, so
     * on the throw that creates it the event is sent before this page
     * exists and nothing is listening yet. That throw drew nothing, and
     * because `play` never ran it never asked to be put away either, so the
     * window was left shown and blank until the next one.
     *
     * Being mounted at all IS the first burst: `ensure(app, "confetti")` is
     * reached from `throw_confetti` and from nowhere else, so this window
     * is never built except by a throw that is already under way.
     *
     * Safe in the other order too. If the page does win the race and the
     * event arrives as well, both paths call `play` and the `running` guard
     * makes the second one a no-op.
     */
    play();
    return () => {
      void off.then((stop) => stop());
    };
  });
</script>

<canvas bind:this={canvas} aria-hidden="true"></canvas>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
  }

  canvas {
    position: fixed;
    inset: 0;
    display: block;
    width: 100vw;
    height: 100vh;
  }
</style>
