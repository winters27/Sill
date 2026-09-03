/**
 * That everything the installer is told to carry is actually on disk.
 *
 * `bundle.resources` in `tauri.conf.json` is read at the very end of a release
 * build, after the whole tree has been compiled optimised. A missing file
 * there costs the entire build, and the two that go missing are both produced
 * by steps that are easy to leave out: `host/dist/host.js` comes from
 * `npm run host:build`, and `esbuild.exe` comes from an `npm ci` that resolved
 * the right platform package.
 *
 * So this asks the question first, from the config rather than from a list
 * written here, in the time it takes to stat a few files.
 *
 * The interface font is checked too. It is not a bundle resource, it is
 * compiled into the frontend, and `npm run build` fetches it with `--required`
 * and stops if it cannot. That check happens inside `tauri build`, which is
 * late; asking here means a machine with no network learns before it compiles.
 *
 * Run: node scripts/verify-bundle.mjs
 */
import { readFileSync, existsSync, readdirSync } from "node:fs";
import { join, resolve, dirname, basename } from "node:path";

const root = resolve(import.meta.dirname, "..");
const tauriDir = join(root, "src-tauri");

let missing = 0;

function gone(what, why) {
  missing += 1;
  console.error(`  ${what}\n    ${why}`);
}

/**
 * Whether a `bundle.resources` key names anything.
 *
 * Tauri resolves these relative to `src-tauri` and accepts a trailing glob on
 * the file name, which is the only glob shape the config actually uses. A
 * pattern in the middle of a path would need real globbing and would also be
 * a config nobody here has written.
 */
function resolves(pattern) {
  const path = join(tauriDir, pattern);

  if (!pattern.includes("*")) return existsSync(path);

  const dir = dirname(path);
  if (!existsSync(dir)) return false;

  const match = basename(pattern).replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*");
  return readdirSync(dir).some((name) => new RegExp(`^${match}$`).test(name));
}

const config = JSON.parse(readFileSync(join(tauriDir, "tauri.conf.json"), "utf8"));
const resources = config.bundle?.resources ?? {};

console.log(`checking ${Object.keys(resources).length} bundle resource(s)`);

for (const [pattern, destination] of Object.entries(resources)) {
  if (resolves(pattern)) continue;

  gone(
    pattern,
    `named by tauri.conf.json as ${destination} and not on disk. ` +
      (pattern.includes("host/dist")
        ? "Run `npm run host:build`."
        : pattern.includes("node_modules")
          ? "Run `npm --prefix host ci`."
          : "Nothing produces this; check the path."),
  );
}

const font = join(root, "src", "lib", "theme", "fonts", "Satoshi-Variable.woff2");
if (!existsSync(font)) {
  gone(
    "src/lib/theme/fonts/Satoshi-Variable.woff2",
    "the interface font is not here, and `npm run build` stops rather than " +
      "package a copy without it. Run `npm run fonts`.",
  );
}

if (missing) {
  console.error(`\n${missing} thing(s) the bundle needs are not there`);
  process.exit(1);
}

console.log("everything the bundle carries is on disk");
