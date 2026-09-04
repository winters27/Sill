/**
 * How a machine describes itself on the published cost page.
 *
 * One implementation, called from both sides. The PowerShell measuring scripts
 * shell out to this and the JavaScript ones import it, because two descriptions
 * of the same desk would appear on the page as two machines, and a reader
 * comparing rows would be told two readings came from different hardware when
 * they came from the same one.
 *
 * What it says is what a reader needs to judge a number and nothing more: the
 * edition of Windows, how many logical processors, how much memory. No machine
 * name, no user name, no paths. This ends up in a public file.
 *
 *   node scripts/machine.mjs
 */
import { cpus, totalmem, version } from "node:os";

export function describe() {
  const gb = Math.round(totalmem() / 1024 ** 3);
  return `${version()}, ${cpus().length} cores, ${gb} GB`;
}

if (process.argv[1] && process.argv[1].endsWith("machine.mjs")) {
  console.log(describe());
}
