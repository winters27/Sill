/**
 * Fetches the Node runtime Sill ships for its extensions.
 *
 * Extensions run in a Node process Sill starts. That used to be whatever
 * `node` the machine had on its PATH, which is a different Node on every
 * machine and no Node at all on most of them: the store told those people to
 * go and install one. Raycast bundles its own runtime, and so does Sill now:
 * one pinned LTS build, in the install directory, used before anything the
 * machine has. The size is the cost, roughly 28 MB in the installer, and it
 * is what makes an extension the same program everywhere it runs.
 *
 * **npm comes with it, and the docs and man pages do not.** Installing an
 * extension runs npm to put its dependencies where esbuild can resolve them,
 * and npm is only ever found beside the Node it is run with, so a runtime
 * without it is a runtime that cannot install anything. Measured against this
 * exact archive: node.exe is 93.4 MB raw and npm's runtime is 9.8 MB, so
 * carrying it is 10.4% more raw and 8.6% more compressed. The docs and man
 * pages are another 2.7 MB that nothing reads and are left in the zip.
 *
 * The repository does not carry the binary; this fetches it per machine from
 * nodejs.org, the way `fetch-fonts.mjs` fetches the font. The version and the
 * SHA-256 of the archive are written down here, so a build reproduces the
 * exact bytes Node published and a mirror or a captive portal cannot hand it
 * something else. Changing the version means changing both lines, on purpose.
 *
 * Windows' own `tar.exe` (bsdtar) opens the zip: it is in every Windows 10
 * and 11, and pulling in an unzip package for one archive is a dependency
 * for the sake of a dependency.
 *
 * Run: node scripts/fetch-node.mjs [--required] [--force]
 */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

/** Node 24 is the active LTS line; this is its release as of 2026-09-05. */
const VERSION = "v24.20.0";
/** From https://nodejs.org/dist/v24.20.0/SHASUMS256.txt, for the win-x64 zip. */
const SHA256 = "6cac9ffbca8f6a47091e4b5c772e0606049c3871cb67d900c0cedde630e545ba";

const archive = `node-${VERSION}-win-x64`;
const url = `https://nodejs.org/dist/${VERSION}/${archive}.zip`;

const root = resolve(import.meta.dirname, "..");
const dir = join(root, "src-tauri", "resources", "node");
const exe = join(dir, "node.exe");
const licence = join(dir, "LICENSE");
/**
 * npm's entry point, where `store::install::npm_cli` looks for it.
 *
 * Named here as well as there because this script is what puts it on disk. A
 * runtime that has node.exe and not this is one where every extension install
 * fails on a message about npm, which is how it shipped before.
 */
const npmCli = join(dir, "node_modules", "npm", "bin", "npm-cli.js");
const stamp = join(dir, "VERSION");

const force = process.argv.includes("--force");
const required = process.argv.includes("--required");

/** Whether what is on disk is the pinned runtime, asked of the binary itself. */
function present() {
  if (!existsSync(exe) || !existsSync(licence) || !existsSync(npmCli)) return false;
  if (!existsSync(stamp) || readFileSync(stamp, "utf8").trim() !== VERSION) return false;
  try {
    return execFileSync(exe, ["--version"], { encoding: "utf8" }).trim() === VERSION;
  } catch {
    return false;
  }
}

if (present() && !force) {
  console.log(`ok   Node ${VERSION} present (${statSync(exe).size} bytes)`);
  process.exit(0);
}

const tar = join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe");
const zip = join(dir, `${archive}.zip`);

try {
  mkdirSync(dir, { recursive: true });

  console.log(`     fetching ${url}`);
  const bytes = new Uint8Array(
    await fetch(url).then((r) => {
      if (!r.ok) throw new Error(`nodejs.org answered ${r.status}`);
      return r.arrayBuffer();
    }),
  );

  const digest = createHash("sha256").update(bytes).digest("hex");
  if (digest !== SHA256) {
    throw new Error(`the archive's SHA-256 is ${digest}, not the ${SHA256} written down for ${VERSION}`);
  }

  writeFileSync(zip, bytes);
  // The runtime, its licence, and npm. Everything else in the archive is
  // documentation: `--exclude` drops npm's own docs and man pages, which are
  // 2.7 MB that nothing on this machine will ever open.
  execFileSync(tar, [
    "-xf",
    zip,
    "-C",
    dir,
    "--strip-components=1",
    "--exclude",
    `${archive}/node_modules/npm/docs/*`,
    "--exclude",
    `${archive}/node_modules/npm/man/*`,
    `${archive}/node.exe`,
    `${archive}/LICENSE`,
    `${archive}/node_modules/npm`,
  ]);
  rmSync(zip, { force: true });
  writeFileSync(stamp, `${VERSION}\n`);

  if (!existsSync(npmCli)) throw new Error("the archive did not yield npm, which installing needs");
  if (!present()) throw new Error("the extracted node.exe does not answer with the pinned version");
  console.log(`ok   Node ${VERSION} fetched with npm (${statSync(exe).size} bytes)`);
} catch (err) {
  rmSync(zip, { force: true });
  console.warn(`warn Node was not fetched: ${err.message}`);

  if (required) {
    console.error("     A build may not ship without its extension runtime. Fix the");
    console.error("     network, or run `npm run node:fetch` before building.");
    process.exit(1);
  }

  console.warn("     Extensions will use a Node on the PATH if there is one.");
  console.warn("     Run `npm run node:fetch` once there is a network to fix that.");
}
