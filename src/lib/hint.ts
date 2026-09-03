/**
 * The tooltip, which is Sill's rather than Windows'.
 *
 * ## Why not `title=""`
 *
 * A native tooltip is drawn by the operating system, so it arrives in the
 * system font at the system size on a white slab, in the middle of a window
 * that is deliberately none of those things. It also takes about a second to
 * appear, cannot be reached from the keyboard at all, and is announced by a
 * screen reader in whichever of four different ways that reader happens to
 * have been configured for, sometimes as the element's name and sometimes not
 * at all. There were ten of them left in the tree.
 *
 * This draws the same sentence in the window's own type on the window's own
 * surface, appears on focus as well as on hover, and hangs itself off the
 * element with `aria-describedby`, which is one behaviour rather than four.
 *
 * ## Why an action and not a component
 *
 * The ten places that want one are a truncated path, a swatch, a count and a
 * round icon button. Wrapping each in a component would put an element between
 * them and the flex or grid that positions them, which is how a tooltip ends
 * up changing a layout it was only meant to describe. An action adds nothing
 * to the tree until somebody hovers.
 *
 * ## What it does not do
 *
 * It does not own a shared node. Each attachment builds its own bubble when it
 * is shown and takes it away again when it is not, so at rest there is no
 * tooltip in the document and nothing listening for one. That is the same rule
 * the rest of the launcher follows: if nothing is happening, nothing is there.
 */

/**
 * How long the pointer has to rest before the bubble appears.
 *
 * Under about a fifth of a second and a tooltip fires while the pointer is
 * merely crossing the element on its way somewhere else, so a row of controls
 * flashes bubbles as the mouse passes over it. This is short enough to feel
 * like an answer and long enough not to be an accident.
 *
 * A number here rather than a token because it is a delay in a timer, not a
 * duration in a stylesheet: nothing animates for this long.
 */
const REST_MS = 220;

/** Clearance between the bubble and the thing it describes. */
const GAP_PX = 6;

/** Clearance between the bubble and the edge of the window. */
const EDGE_PX = 8;

/**
 * Shows `text` when the pointer rests on this element, or when it takes focus.
 *
 * Passing an empty string or nothing turns it off, which is what lets a caller
 * write `use:hint={row.truncated ? row.full : ""}` without a branch in the
 * markup.
 */
export function hint(node: HTMLElement, text: string | undefined) {
  /*
   * This attachment's own id.
   *
   * Random rather than counted, because a counter would have to live at module
   * scope and be shared by every attachment in every window. Two bubbles are
   * only ever in the document at once by accident, but an id collision makes
   * `aria-describedby` resolve to the wrong sentence and nothing looks wrong.
   */
  const id = `sill-hint-${Math.random().toString(36).slice(2, 10)}`;

  let saying = text ?? "";
  let bubble: HTMLElement | null = null;
  let waiting: ReturnType<typeof setTimeout> | null = null;

  function place() {
    if (!bubble) return;

    const around = node.getBoundingClientRect();
    const own = bubble.getBoundingClientRect();

    // Above by preference, below when there is no room above. A bubble that
    // covers the thing it describes is worse than one on the wrong side.
    const above = around.top - own.height - GAP_PX;
    const top = above >= EDGE_PX ? above : around.bottom + GAP_PX;

    const wanted = around.left + around.width / 2 - own.width / 2;
    const left = Math.min(
      Math.max(wanted, EDGE_PX),
      Math.max(EDGE_PX, window.innerWidth - own.width - EDGE_PX),
    );

    bubble.style.top = `${Math.round(top)}px`;
    bubble.style.left = `${Math.round(left)}px`;
  }

  function show() {
    if (bubble || !saying) return;

    bubble = document.createElement("div");
    bubble.id = id;
    bubble.className = "sill-hint";
    bubble.setAttribute("role", "tooltip");
    bubble.textContent = saying;
    document.body.appendChild(bubble);

    /*
     * Described by, not labelled by.
     *
     * Several of these sit on a control that already says what it is, and a
     * label would replace that name with the explanation. Where the element
     * has no name of its own the reader reads the description anyway.
     *
     * Skipped when the element's own name is already this sentence, which is
     * what a round icon button carrying both looks like: the reader would say
     * it twice.
     */
    if (node.getAttribute("aria-label") !== saying) {
      node.setAttribute("aria-describedby", id);
    }

    place();
  }

  function hide() {
    if (waiting !== null) {
      clearTimeout(waiting);
      waiting = null;
    }

    node.removeAttribute("aria-describedby");
    bubble?.remove();
    bubble = null;
  }

  function rest() {
    if (waiting !== null || bubble || !saying) return;
    waiting = setTimeout(() => {
      waiting = null;
      show();
    }, REST_MS);
  }

  /* Focus is not a rest: somebody who arrived here with the keyboard has
     already stopped, and waiting a fifth of a second to answer them would be
     a delay with nothing behind it. */
  const onFocus = () => show();

  /* Escape dismisses it without moving anything, which is what WCAG 1.4.13
     asks of any content that appears on hover. */
  const onKey = (event: KeyboardEvent) => {
    if (event.key === "Escape") hide();
  };

  node.addEventListener("pointerenter", rest);
  node.addEventListener("pointerleave", hide);
  node.addEventListener("pointerdown", hide);
  node.addEventListener("focus", onFocus);
  node.addEventListener("blur", hide);
  node.addEventListener("keydown", onKey);

  return {
    update(next: string | undefined) {
      saying = next ?? "";

      // The sentence changed under an open bubble: a filter narrowed the row,
      // or a path was renamed. Redraw rather than leave the old words up.
      if (bubble) {
        if (!saying) hide();
        else {
          bubble.textContent = saying;
          place();
        }
      }
    },
    destroy: hide,
  };
}
