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
  import { burst, settled, step, type Particle } from "$lib/confetti";
  import "$lib/theme/theme.css";

  let canvas = $state<HTMLCanvasElement | null>(null);
  let running = false;

  /**
   * The colours, read from the theme rather than chosen here: the accent
   * and the three states are the four colours Sill already has, so confetti
   * looks like it belongs to the same launcher.
   */
  function palette(): string[] {
    const style = getComputedStyle(document.documentElement);
    const named = ["--accent", "--success", "--warning", "--info"]
      .map((name) => style.getPropertyValue(name).trim())
      .filter(Boolean);
    return named.length ? named : [style.color];
  }

  function draw(context: CanvasRenderingContext2D, pieces: Particle[], colours: string[]) {
    context.clearRect(0, 0, innerWidth, innerHeight);
    for (const piece of pieces) {
      context.save();
      context.translate(piece.x, piece.y);
      context.rotate(piece.angle);
      context.fillStyle = colours[piece.colour % colours.length];
      context.fillRect(-piece.size / 2, -piece.size / 4, piece.size, piece.size / 2);
      context.restore();
    }
  }

  function play() {
    if (running || !canvas) return;
    const context = canvas.getContext("2d");
    if (!context) return;

    running = true;
    const scale = devicePixelRatio || 1;
    canvas.width = Math.round(innerWidth * scale);
    canvas.height = Math.round(innerHeight * scale);
    context.setTransform(scale, 0, 0, scale, 0, 0);

    const colours = palette();
    const pieces = burst(innerWidth, innerHeight, 180);
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
