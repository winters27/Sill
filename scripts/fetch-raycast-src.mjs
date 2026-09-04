/**
 * Fetches the real Raycast extensions `gate:views` draws against.
 *
 * `npm run gate:views` renders somebody else's extensions rather than only
 * fixtures, because a fixture agrees with the host by construction and cannot
 * say whether a command written by a stranger still works. Those extensions
 * live in `raycast/extensions`, which is far too big to vendor for three
 * directories and is gitignored here, so a fresh clone has nothing to draw and
 * the gate fails at its first line.
 *
 * This is the one command that fixes that:
 *
 * ```text
 * npm run extensions:fetch
 * npm run gate:views
 * ```
 *
 * ## Which extensions, and why the list is not written here
 *
 * The gate names them, so the gate is read for them. A list typed into this
 * file would be a second list with nothing making it agree with the first, and
 * the failure would be quiet in the worst way: the gate skips an extension it
 * cannot find, so a name that fell out of step here would turn a check into a
 * silent pass rather than into an error.
 *
 * Only the paths the gate spells out are taken. The two it reaches through a
 * shell variable are optional by its own design, and it says so when they are
 * absent, so they are fetched only when they are asked for by name:
 *
 * ```text
 * node scripts/fetch-raycast-src.mjs kill-process hacker-news
 * ```
 *
 * ## Why the clone is not pinned to a commit
 *
 * `verify.yml` argues this and the argument holds here. The gate exists to
 * answer whether the host still renders real extensions, and a frozen copy
 * stops being able to answer that: it would report on the ecosystem as it was
 * on the day somebody typed a hash. The cost is that an upstream change can
 * turn the gate red without anything here having moved, and that is the right
 * way round, because it is true.
 *
 * ## Why the dependencies are installed
 *
 * Not optional. The gate bundles five commands with esbuild and one of them,
 * uuid-generator's `generateV7`, imports `uuid`, `typeid-js` and `ulidx`.
 * esbuild bundles what it is pointed at, so an unresolved import is a build
 * failure rather than a warning.
 *
 * `--ignore-scripts` because `@raycast/api` runs `bin/ray npm-post-install`
 * after install, which is a shell line that does not survive cmd.exe. Nothing
 * here needs whatever it does: these are bundled with esbuild and run against
 * Sill's own API layer, never against Raycast's toolchain.
 *
 * `install` rather than `ci`, because `ci` refuses a tree whose lockfile is
 * missing or out of step and these are somebody else's trees at whatever
 * commit the unpinned clone landed on. Determinism there was never on offer.
 */
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const checkout = join(root, "extensions", "raycast-src");
const GATE = join(root, "scripts", "gate-views.sh");
const UPSTREAM = "https://github.com/raycast/extensions.git";

/**
 * Running npm, which on Windows is more awkward than it looks.
 *
 * `npm` there is `npm.cmd`, and Node has refused to start a `.cmd` without a
 * shell since the argument-injection fix, so a bare `spawnSync("npm")` is
 * ENOENT and `spawnSync("npm.cmd")` is EINVAL. A shell is required rather than
 * chosen.
 *
 * Written as one line rather than as a command plus an array, because Node
 * warns about the second form under a shell for a good reason: the array is
 * concatenated and not escaped, so the shell parses it after all. Nothing here
 * comes from a caller, and a line has no second reading.
 *
 * The extension is reached through `cwd` rather than through `--prefix`. That
 * path can contain a space, which survives being a working directory and does
 * not survive being part of a command line.
 */
const NPM_INSTALL = "npm install --ignore-scripts --no-audit --no-fund";

/**
 * Every extension the gate names outright.
 *
 * A literal path only. The gate also builds paths from a loop variable, and
 * those are the cases it explicitly skips with a reason when they are missing,
 * so pulling them in here would change what the gate means.
 */
function wantedByTheGate() {
  const text = readFileSync(GATE, "utf8");
  const found = [...text.matchAll(/extensions\/raycast-src\/extensions\/([A-Za-z0-9._-]+)/g)];
  return [...new Set(found.map((one) => one[1]))];
}

/** Runs a command, and stops the script if it fails. */
function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: "inherit", shell: false, ...options });

  if (result.error) {
    console.error(`\n${command} could not be started: ${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) {
    console.error(`\n${[command, ...args].join(" ")} failed with ${result.status}`);
    process.exit(result.status ?? 1);
  }
}

const asked = process.argv.slice(2).filter((one) => !one.startsWith("--"));
const skipInstall = process.argv.includes("--no-install");

const fromGate = wantedByTheGate();

if (fromGate.length === 0) {
  console.error(
    "scripts/gate-views.sh names no extension under extensions/raycast-src, so this is " +
      "parsing rather than reading. Check the paths in the gate before trusting this script.",
  );
  process.exit(1);
}

const wanted = [...new Set([...fromGate, ...asked])];

console.log(`fetching ${wanted.length} extension(s) from raycast/extensions`);
console.log(`  ${wanted.join(", ")}\n`);

if (existsSync(join(checkout, ".git"))) {
  console.log("the checkout is already here; widening it to cover everything asked for");
} else {
  /*
   * Blobless and sparse, which is what makes this seconds rather than an
   * afternoon. The whole repository is many gigabytes; this is about 25 MB,
   * because no file content is fetched until a path is actually checked out.
   */
  run("git", [
    "clone",
    "--filter=blob:none",
    "--sparse",
    "--depth",
    "1",
    UPSTREAM,
    checkout,
  ]);
}

run("git", [
  "-C",
  checkout,
  "sparse-checkout",
  "set",
  ...wanted.map((name) => `extensions/${name}`),
]);

for (const name of wanted) {
  const where = join(checkout, "extensions", name);

  if (!existsSync(join(where, "package.json"))) {
    console.error(
      `\n${name} has no package.json after the sparse checkout, so upstream has moved or ` +
        "renamed it. The gate reads these paths too, so fix them together.",
    );
    process.exit(1);
  }

  if (skipInstall) continue;

  console.log(`\ninstalling ${name}'s dependencies`);
  run(NPM_INSTALL, [], { cwd: where, shell: true });
}

console.log("\nready. `npm run gate:views` can draw these now.");
