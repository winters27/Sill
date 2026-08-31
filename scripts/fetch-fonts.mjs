/**
 * Fetches the bundled interface font, which the repository is not allowed to
 * carry.
 *
 * Satoshi is under the ITF Free Font License, not the OFL. That licence lets
 * anyone embed it in a desktop application at no cost, which is what Sill
 * does, but section 02 forbids making the font file itself available through
 * "another font website, font library, marketplace, repository, download
 * service, application or platform [...] publicly accessible servers". A
 * public git repository is exactly that: anyone can clone it and take the
 * file out on its own. So the file is fetched per machine instead, which is
 * the arrangement the licence describes, where each user holds their own copy
 * direct from Fontshare and is bound by the licence themselves.
 *
 * The URL is read out of Fontshare's own stylesheet rather than written down
 * here. The path carries opaque hashes that change when they republish, and a
 * hardcoded one would rot into a 404 that looks like a network failure.
 *
 * Never fatal. `--font` in `theme.css` names Segoe UI Variable behind Satoshi,
 * so a machine with no network draws the interface in the Windows face rather
 * than failing to build.
 *
 * Run: node scripts/fetch-fonts.mjs [--force]
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const target = join(root, "src", "lib", "theme", "fonts", "Satoshi-Variable.woff2");
const stylesheet = "https://api.fontshare.com/v2/css?f%5B%5D=satoshi@1,2";

/**
 * The upright variable face, which is the first `woff2` in their stylesheet.
 *
 * The italic block follows it and Sill does not use it: the design synthesises
 * the rare italic rather than carrying a second file for it.
 */
function firstWoff2(css) {
  const found = css.match(/\/\/cdn\.fontshare\.com\/[^'"]+\.woff2/);
  if (!found) throw new Error("no woff2 URL in Fontshare's stylesheet");
  return `https:${found[0]}`;
}

/**
 * That the bytes are a font and not a captive portal's login page.
 *
 * A hotel network answering every request with HTML would otherwise leave a
 * file named `.woff2` that the browser refuses silently, and the interface
 * would fall back to Segoe with nothing saying why.
 */
function isWoff2(bytes) {
  return bytes.length > 1024 && new TextDecoder().decode(bytes.subarray(0, 4)) === "wOF2";
}

const force = process.argv.includes("--force");

/**
 * Whether a missing font is fatal.
 *
 * `postinstall` runs without it, because somebody installing on a train should
 * still get a working checkout, and `npm run dev` draws perfectly well in
 * Segoe meanwhile. A build is the opposite case. `beforeBuildCommand` routes
 * every packaged binary through `npm run build`, and nobody who downloads one
 * of those is ever going to run this script, so the face has to be in the
 * bundle by then or it is never coming.
 *
 * Nothing downstream would say so either. Vite answers a missing `url()` by
 * leaving it as written, printing one line about it and exiting 0, so a failed
 * fetch here reaches a shipped installer that reads in the wrong face with a
 * green build behind it the whole way.
 */
const required = process.argv.includes("--required");

if (existsSync(target) && !force) {
  /*
   * Present is not the same as usable. An interrupted write leaves a file that
   * `existsSync` is satisfied by and a font stack is not, and on the path that
   * may not ship without it, that has to be caught here rather than by whoever
   * downloads the result.
   */
  if (required && !isWoff2(readFileSync(target))) {
    console.error(`fail ${target} is not a woff2`);
    console.error("     Run `npm run fonts -- --force` to fetch it again.");
    process.exit(1);
  }

  console.log(`ok   Satoshi present (${statSync(target).size} bytes)`);
  process.exit(0);
}

try {
  const css = await fetch(stylesheet).then((r) => {
    if (!r.ok) throw new Error(`Fontshare answered ${r.status}`);
    return r.text();
  });

  const url = firstWoff2(css);
  const bytes = new Uint8Array(
    await fetch(url).then((r) => {
      if (!r.ok) throw new Error(`the font answered ${r.status}`);
      return r.arrayBuffer();
    }),
  );

  if (!isWoff2(bytes)) throw new Error("what came back was not a woff2");

  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, bytes);
  console.log(`ok   Satoshi fetched from Fontshare (${bytes.length} bytes)`);
} catch (err) {
  console.warn(`warn Satoshi was not fetched: ${err.message}`);

  /*
   * A build stops here. Everything else carries on, because the face is a
   * setting and Segoe UI Variable stands behind it in `--font`: a fair answer
   * for a checkout, and no answer at all for something being handed to
   * somebody else.
   */
  if (required) {
    console.error("     A build may not ship without it. Fix the network, or");
    console.error("     run `npm run fonts` before building.");
    process.exit(1);
  }

  console.warn("     The interface will draw in Segoe UI Variable instead.");
  console.warn("     Run `npm run fonts` once there is a network to fix that.");
}
