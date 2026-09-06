/**
 * Keeping a transcript at the bottom while an answer arrives, and only then.
 *
 * Only while the reader is already at the bottom: yanking somebody back down
 * while they are reading what was said earlier is worse than letting the new
 * text arrive out of sight. Whether they are at the bottom is sampled when
 * they scroll, not when the text grows, because after the text has grown the
 * distance to the bottom is the height of the text that just arrived.
 *
 * A Svelte action on the scrolling box. It watches the box's first child for
 * growth, so the box's contents are wrapped in one element. Active only while
 * `live` is true: an idle window has no listener and no observer.
 */

/** How close to the bottom still counts as reading the newest thing. */
const NEAR = 80;

export function follow(box: HTMLElement, live: boolean) {
  let near = true;
  let observer: ResizeObserver | null = null;

  const sample = () => {
    near = box.scrollHeight - box.scrollTop - box.clientHeight < NEAR;
  };

  const pin = () => {
    if (near) box.scrollTop = box.scrollHeight;
  };

  const start = () => {
    if (observer) return;
    sample();
    box.addEventListener("scroll", sample, { passive: true });
    observer = new ResizeObserver(pin);
    const flow = box.firstElementChild;
    if (flow) observer.observe(flow);
  };

  const stop = () => {
    if (!observer) return;
    box.removeEventListener("scroll", sample);
    observer.disconnect();
    observer = null;
  };

  if (live) start();

  return {
    update(next: boolean) {
      if (next) start();
      else stop();
    },
    destroy: stop,
  };
}
