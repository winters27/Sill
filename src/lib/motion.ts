/**
 * Sill's motion, in one place.
 *
 * Everything that appears or disappears used to do so abruptly: forty-two CSS
 * transitions existed and every one of them was a hover or a selection change,
 * so nothing had an entrance. A popover simply was not there and then was.
 *
 * Three rules hold everywhere here:
 *
 * 1. **Transform and opacity only.** Both are composited, so an animation
 *    costs no layout and no paint. Animating width, height, top or filter
 *    would put work on the main thread on every frame of it.
 * 2. **`backdrop-filter` is never animated.** Transitioning it is the WebView2
 *    glass-flicker bug that `theme.css` documents; it is also inert in this
 *    window, so there would be nothing to gain.
 * 3. **Leaving is faster than arriving.** An exit that takes as long as an
 *    entrance feels like the interface is reluctant to get out of the way.
 *
 * Motion here is a settle, not a flight: a few pixels and a few percent. The
 * launcher is summoned to be typed into within a couple of hundred
 * milliseconds, so anything that delays reading it is a cost, not polish.
 */

import { cubicOut } from "svelte/easing";

/** Where a popover grows from. Match it to whatever anchors the popover. */
export type Origin = "bottom left" | "bottom right" | "top left" | "top right" | "center";

interface PopoverOptions {
  origin?: Origin;
  /** Use the exit duration rather than the entrance one. */
  out?: boolean;
  /** Travel in px. Deliberately small: this settles, it does not fly. */
  lift?: number;
}

/**
 * Durations, read from the stylesheet so CSS and JS cannot disagree.
 *
 * Read once and cached. `getComputedStyle` forces a style recalculation, and
 * while once per popover would be survivable, once per session is free.
 */
let cached: { enter: number; exit: number } | null = null;

function durations() {
  if (cached) return cached;

  const read = (name: string, fallback: number) => {
    if (typeof document === "undefined") return fallback;
    const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    const value = Number.parseFloat(raw);
    return Number.isFinite(value) && value > 0 ? value : fallback;
  };

  cached = { enter: read("--motion-enter", 150), exit: read("--motion-exit", 100) };
  return cached;
}

/**
 * Whether the machine has asked for less movement.
 *
 * Checked per transition rather than cached: it is one cheap media-query read,
 * and somebody who turns the setting on should not have to restart the app to
 * be taken seriously.
 */
function reduced(): boolean {
  return (
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/**
 * A popover arriving or leaving.
 *
 * Returns a `css` function rather than a `tick` one, so Svelte compiles it to
 * a real CSS animation and the compositor runs it. A `tick` transition would
 * be JavaScript on every frame for something the GPU does for nothing.
 *
 * ```svelte
 * <div in:popover={{ origin: "bottom left" }}
 *      out:popover={{ origin: "bottom left", out: true }}>
 * ```
 */
export function popover(_node: Element, options: PopoverOptions = {}) {
  const { origin = "bottom left", out = false, lift = 6 } = options;
  const ms = durations();

  return {
    // Zero rather than "skipped": the element still mounts and unmounts
    // normally, it simply does so instantly.
    duration: reduced() ? 0 : out ? ms.exit : ms.enter,
    easing: cubicOut,
    css: (t: number, u: number) =>
      `opacity: ${t};` +
      `transform-origin: ${origin};` +
      `transform: translateY(${u * lift}px) scale(${0.97 + 0.03 * t});`,
  };
}

/**
 * A drawer opening or closing inside the thing that owns it.
 *
 * **This one animates height, and is the deliberate exception to rule 1.**
 * A drawer whose contents fade in while the box around them snaps to full
 * size is not an opening, it is a jump with a fade over it: the movement the
 * eye actually follows is the edge, and that edge was not moving. Nothing
 * composited can move an edge, because moving an edge is a layout.
 *
 * The cost is bounded and it is worth naming rather than hiding: one element
 * relaying out for the length of one user-initiated click, in a settings
 * window, at most once per click. That is not the launcher, nothing here is
 * on a hot path, and at rest it costs nothing at all.
 *
 * `scrollHeight` is read once when the transition starts rather than per
 * frame, so this measures the layout once and then only writes to it.
 */
export function drawer(node: Element, options: { out?: boolean } = {}) {
  const ms = durations();
  const el = node as HTMLElement;

  // Read before the first frame writes anything, so this is one measurement
  // and not a read/write cycle per frame.
  const style = getComputedStyle(el);
  const height = el.scrollHeight;
  const padTop = Number.parseFloat(style.paddingTop) || 0;
  const padBottom = Number.parseFloat(style.paddingBottom) || 0;

  return {
    duration: reduced() ? 0 : options.out ? ms.exit : ms.enter,
    easing: cubicOut,
    css: (t: number) =>
      `overflow: hidden;` +
      `height: ${t * height}px;` +
      `padding-top: ${t * padTop}px;` +
      `padding-bottom: ${t * padBottom}px;` +
      // Fades slightly ahead of the collapse, so the last frame is empty
      // rather than a sliver of clipped text.
      `opacity: ${Math.min(1, t * 1.4)};`,
  };
}

/**
 * A panel's content being replaced.
 *
 * Smaller and quicker than a popover, and it does not scale: the panel is not
 * arriving from anywhere, its contents are being swapped underneath a heading
 * that stays put. A scale would make the whole window look like it moved.
 */
export function swap(_node: Element, options: { out?: boolean } = {}) {
  const ms = durations();

  return {
    duration: reduced() ? 0 : options.out ? ms.exit : ms.enter,
    easing: cubicOut,
    css: (t: number, u: number) => `opacity: ${t}; transform: translateY(${u * 4}px);`,
  };
}
