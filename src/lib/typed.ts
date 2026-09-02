/**
 * Recognising that what was typed is an address or a path.
 *
 * ## Why this exists
 *
 * Typing `https://example.com` and pressing Enter searched the web for the
 * words "https://example.com", which is nobody's intent: an address is not a
 * question, it is a destination. And typing `C:\Users\` did nothing at all.
 *
 * ## Why it is decided here
 *
 * The same reason the web search row is built in the window: there is nothing
 * to decide until the row is chosen, and asking Rust to compose one row per
 * keystroke is exactly the chatter the constitution's rule 18 is about. What
 * is decided is a pure question about a string, so it is a function with
 * tests rather than a round trip.
 */

/**
 * Whether this is an address a browser would open.
 *
 * Two shapes, and both are things people actually type. One with a scheme,
 * which is unambiguous. One without, which has to be judged: `example.com` is
 * an address and `notepad.exe` is not, so a bare host needs a dot, no spaces,
 * and a last part that reads like a domain ending rather than a file
 * extension.
 */
export function isUrl(typed: string): boolean {
  const text = typed.trim();
  if (!text || /\s/.test(text)) return false;

  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(text)) {
    // A scheme and something after it. `https://` alone is somebody halfway
    // through typing, not an address.
    return text.split("//")[1]?.length > 0;
  }

  // A bare host. `www.` counts on its own, because nobody types that by
  // accident.
  if (/^www\./i.test(text)) return text.length > 4;

  const host = text.split(/[/?#]/)[0];
  if (!host.includes(".")) return false;

  const last = host.slice(host.lastIndexOf(".") + 1);

  // Two letters or more, all letters. That is what a domain ending looks
  // like, and it is what keeps `readme.md`, `main.rs` and `notepad.exe` out:
  // those endings are the same shape, so the file-extension list below is
  // what actually separates them.
  if (!/^[a-z]{2,}$/i.test(last)) return false;

  return !FILE_ENDINGS.has(last.toLowerCase());
}

/**
 * Endings that look like a domain and are not.
 *
 * Short, and only the ones that collide: an ending nobody types into a
 * launcher does not need to be here, and a long list would be a second thing
 * to maintain. Every one of these is also a real top level domain, which is
 * why the shape alone cannot decide.
 */
const FILE_ENDINGS = new Set([
  "exe",
  "md",
  "rs",
  "ts",
  "js",
  "json",
  "html",
  "css",
  "py",
  "sh",
  "txt",
  "log",
  "zip",
  "pdf",
  "png",
  "jpg",
  "gif",
  "mp4",
  "app",
  "dev",
  "sql",
  "yml",
  "yaml",
  "toml",
  "lock",
  "bat",
  "ps1",
  "dll",
  "ini",
  "cfg",
]);

/** The address to open, with a scheme put on the front if it needs one. */
export function asUrl(typed: string): string {
  const text = typed.trim();

  return /^[a-z][a-z0-9+.-]*:\/\//i.test(text) ? text : `https://${text}`;
}

/**
 * Whether this is a path to something on this machine.
 *
 * A drive letter, a UNC share, or one of the shell's own folder variables.
 * Deliberately not "anything with a backslash": a query is not a path because
 * somebody typed a slash in it, and offering to open one that cannot exist is
 * worse than not offering.
 */
export function isPath(typed: string): boolean {
  const text = typed.trim();
  if (!text) return false;

  // C:\ or C:/ and anything after it, including nothing.
  if (/^[a-z]:[\\/]/i.test(text)) return true;

  // A share.
  if (/^\\\\[^\\]/.test(text)) return true;

  // %APPDATA%\... and the rest, which Windows expands everywhere else and
  // people type into address bars for the same reason.
  return /^%[a-z_]+%[\\/]/i.test(text);
}
