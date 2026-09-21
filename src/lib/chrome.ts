/**
 * Whether this window is the one in front, published to CSS.
 *
 * A stylesheet cannot ask the operating system which window is focused, and
 * the glaze wants to know: lit chrome on a window nobody is using is the
 * tell that a surface is painted on rather than real. So the window's focus
 * is written onto `<html>` as `data-window-blurred="true"` while it is not
 * in front, beside `data-theme` and `data-font`, and removed when it is.
 *
 * One Tauri listener per window that asks for it, and an attribute flip per
 * focus change. Nothing runs between changes.
 *
 * Mounted by the three windows that draw their own title bar, and by nothing
 * else on purpose. The dictation panel is never focused (it must leave focus
 * where it found it, or the transcript pastes into the panel), and pin and
 * capture are built unfocused, so a window-wide watcher would mark them
 * blurred at mount and never clear it. The launcher hides on blur.
 */
import { getCurrentWindow } from "@tauri-apps/api/window";

const BLURRED = "data-window-blurred";

/** Starts watching. Returns the function that stops. */
export function watchFocus(): () => void {
  // A preview route in an ordinary browser tab has no window to ask.
  if (!("__TAURI_INTERNALS__" in window)) {
    return () => {};
  }

  const root = document.documentElement;
  const mark = (focused: boolean) => {
    if (focused) {
      root.removeAttribute(BLURRED);
    } else {
      root.setAttribute(BLURRED, "true");
    }
  };

  mark(document.hasFocus());

  let cancelled = false;
  let unlisten: (() => void) | undefined;

  getCurrentWindow()
    .onFocusChanged(({ payload }) => mark(payload))
    .then((stop) => {
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    })
    .catch(() => {
      // Denied or unavailable: the chrome stays lit, which is the state it
      // was in before this existed.
    });

  return () => {
    cancelled = true;
    unlisten?.();
    root.removeAttribute(BLURRED);
  };
}
