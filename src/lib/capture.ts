import { invoke } from "@tauri-apps/api/core";
import { orElse, silently } from "$lib/status";

/**
 * Picking a piece of the screen, and copying it.
 *
 * The overlay is a window of its own placed over every screen, so everything
 * here is a request to Rust rather than anything the launcher draws.
 */

/** Puts the picking overlay up. The launcher gets out of the way first. */
export function beginCapture(): Promise<void> {
  return invoke("begin_capture");
}

/** Takes the overlay away without capturing. */
export function cancelCapture(): Promise<void> {
  return invoke("cancel_capture");
}

/**
 * Copies a rectangle of the screen.
 *
 * Coordinates are the screen's own physical pixels, which is not what the
 * pointer reports: the caller converts, because only it knows the scaling of
 * the display the drag happened on.
 *
 * Returns what happened, to be shown as it is.
 */
export function captureArea(
  left: number,
  top: number,
  width: number,
  height: number,
): Promise<string> {
  return invoke<string>("capture_area", { left, top, width, height });
}

/** Copies the whole of every screen at once. */
export function captureScreen(): Promise<string> {
  return invoke<string>("capture_screen");
}

/**
 * What the overlay is up for.
 *
 * Copying a picture is the ordinary case. The other three hand the rectangle
 * back to whoever put the overlay up: a region for the model to read, a pixel
 * to name the colour of, a code to decode.
 */
export type Purpose = "copy" | "choose" | "colour" | "qr";

/**
 * Asks Rust what the overlay is up for, once it is shown.
 *
 * Validated rather than trusted: a command Tauri denies resolves with nothing,
 * and an overlay that then handed a rectangle nowhere would look like a
 * capture that did nothing. Copying is the answer when the question fails,
 * silently, because copying is what the overlay did before the question
 * existed and nothing about it is worse for the question going unanswered.
 */
export async function capturePurpose(): Promise<Purpose> {
  const said = await invoke<unknown>("capture_purpose").catch(silently(null));
  return said === "choose" || said === "colour" || said === "qr" ? said : "copy";
}

/**
 * Hands a rectangle back to whoever put the overlay up for one.
 *
 * Physical pixels, like `captureArea`, and for the same reason.
 */
export function choseArea(
  left: number,
  top: number,
  width: number,
  height: number,
): Promise<void> {
  return invoke("chose_area", { left, top, width, height });
}

/**
 * Opens the markup window on a picture from the clipboard history.
 *
 * The picture is handed over by Rust rather than passed here: a window cannot
 * be given an argument when it is shown, so it asks once it is up.
 */
export function openMarkup(entry: number): Promise<void> {
  return invoke("open_markup", { entry });
}

/** The picture the markup window should be showing, as a data URI. */
export function markupImage(): Promise<string | null> {
  return invoke<string | null>("markup_image");
}

/** Puts the marked-up picture on the clipboard and closes the window. */
export function finishMarkup(png: string): Promise<string> {
  return invoke<string>("finish_markup", { png });
}

/** Closes the markup window, keeping nothing. */
export function cancelMarkup(): Promise<void> {
  return invoke("cancel_markup");
}

/**
 * The row number of the last picture copied, or nothing if there is none.
 *
 * Asked of Rust rather than worked out here, so the row that marks up a
 * picture and the key bound to it cannot disagree about which one they mean.
 */
export function lastImage(): Promise<number | null> {
  return invoke<number | null>("last_image_entry");
}

/** A window the picker can capture, in the screen's own physical pixels. */
export interface CaptureTarget {
  id: number;
  title: string;
  app: string;
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * The windows the picker can offer, topmost first.
 *
 * Asked for once when the overlay opens, not per pointer move: the desk does
 * not rearrange itself while somebody is choosing, and enumerating windows on
 * every mouse move would be a Win32 walk per frame.
 *
 * Reported when it fails, because an empty list is indistinguishable from a
 * desk with nothing on it and the overlay draws it the same way. Click a
 * window is a setting, it reads as on, and clicking one would simply do
 * nothing with no sign anywhere that the enumeration is what broke.
 */
export function captureTargets(): Promise<CaptureTarget[]> {
  return invoke<CaptureTarget[]>("capture_targets").catch(
    orElse("capture", "which windows are on screen to capture", [], "screenshot"),
  );
}

/** Copies one window, whole, even where something is sitting on top of it. */
export function captureWindow(id: number): Promise<string> {
  return invoke<string>("capture_window", { id });
}

/** Copies one whole display. */
export function captureDisplay(index: number): Promise<string> {
  return invoke<string>("capture_display", { index });
}
