/**
 * An extension written against `@sill/api`, so the API has a reader.
 *
 * A real extension directory rather than a pre-bundled fixture, because the
 * point of it is the whole chain: this `package.json` is what
 * `extension_install.rs` parses into a contributed action, and this source is
 * what `scripts/build-extension.mjs` bundles for the view gate to run. One
 * file on disk, read by both halves, so they cannot drift.
 *
 * It uses all three parts of the API deliberately:
 *
 * - `actionTarget()` for the object, and the branch for having been run
 *   without one, which is what happens off the root list;
 * - `holds()` for the capability, which is how an extension says "you have not
 *   allowed this" in its own words rather than throwing out of `Clipboard`;
 * - `apiVersion` in the message, so a run proves the constant is reachable.
 */

import { Clipboard, showHUD, showToast, Toast } from "@raycast/api";
import { actionTarget, apiVersion, holds } from "@sill/api";

export default async function copyWhatItIs(): Promise<void> {
  const on = actionTarget();

  if (!on) {
    await showToast({
      style: Toast.Style.Failure,
      title: "Nothing to copy",
      message: "Run this from the action panel of a file or a folder.",
    });
    return;
  }

  if (!holds("clipboardWrite")) {
    await showToast({
      style: Toast.Style.Failure,
      title: "Not allowed to write the clipboard",
      message: "Grant it in Settings, under Extensions.",
    });
    return;
  }

  await Clipboard.copy(`${on.kind}\t${on.target}`);
  await showHUD(`Copied the ${on.kind} ${on.title} (sill api ${apiVersion})`);
}
