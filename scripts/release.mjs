/**
 * Cuts a release: `npm run release 0.2.0`.
 *
 * Everything between "the work is committed" and "a tag is on GitHub" is
 * mechanical, and doing it by hand is how a release goes out with a benchmark
 * page still naming the previous version, or a tag that disagrees with
 * `package.json`, or no changelog section at all. Each of those has happened
 * here, each was caught by a check that runs an hour into a build or not at
 * all, and each cost a tag that had to be deleted and moved.
 *
 * What it does, in order, stopping at the first thing that is not true:
 *
 * 1. The tree is clean and this is `main`, up to date with the remote.
 * 2. The version is newer than the one in `package.json`, and has no tag.
 * 3. `CHANGELOG.md` has a section for it. This is the one part nobody can
 *    generate: the diff says what changed, and a person says what of that
 *    somebody would notice.
 * 4. `verify`, on the tree as it stands, so a failure costs minutes here
 *    rather than a tag that has to be deleted and moved.
 * 5. Every version in the tree is set and the cost page is regenerated, then
 *    the two checks a bump can break are run again. Both take seconds, and
 *    running them after the slow part is what leaves a failed run with the
 *    tree exactly as it found it.
 * 6. Commit, tag, and push both.
 *
 * The workflow does the rest: it builds, waits for `verify` on the same
 * commit, and publishes only if that passed.
 *
 * `--dry-run` does every check and changes nothing.
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const version = args.find((arg) => !arg.startsWith("-"));

if (!version) {
  console.error("usage: npm run release <version> [--dry-run]");
  process.exit(2);
}

/**
 * Runs an npm script, which on Windows is not as simple as running `npm`.
 *
 * Two failures in a row, and neither message says what is wrong. `npm` is
 * `npm.cmd`, a batch script rather than an executable, so `execFile` answers
 * `ENOENT` against the name, which reads like npm is not installed. Naming
 * `npm.cmd` gets past that and into `EINVAL`, which is Node refusing to spawn
 * a `.cmd` at all without a shell: it has done that since the argument-quoting
 * hole in the Windows batch parser was closed, and the refusal is the fix.
 *
 * So the shell is the answer rather than the workaround, and the whole line
 * goes to it as one string: passing an args array alongside `shell: true`
 * concatenates without escaping, which Node now warns about. `script` is a
 * literal at every call, never a path or anything somebody typed.
 */
function npmRun(script) {
  const npm = process.platform === "win32" ? "npm.cmd" : "npm";
  execFileSync(`${npm} run ${script}`, { stdio: "inherit", shell: true });
}

/** Runs something and lets its output through, or stops. */
function run(command, commandArgs) {
  execFileSync(command, commandArgs, { stdio: "inherit" });
}

/** Runs something and keeps what it said. */
function said(command, commandArgs) {
  return execFileSync(command, commandArgs, { encoding: "utf8" }).trim();
}

function stop(why, fix) {
  console.error(`\n${why}`);
  if (fix) console.error(`  ${fix}`);
  process.exit(1);
}

/** Three numbers, because the tag, the crate and the installer all parse it. */
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  stop(`"${version}" is not a version.`, "Three numbers: 0.2.0");
}

const tag = `v${version}`;
console.log(`cutting ${tag}\n`);

// ---- 1. Where this is being run from -------------------------------------

const branch = said("git", ["rev-parse", "--abbrev-ref", "HEAD"]);
if (branch !== "main") {
  stop(`on ${branch}, not main.`, "A release is cut from main.");
}

if (said("git", ["status", "--porcelain"])) {
  stop(
    "the tree has uncommitted changes.",
    "Commit them first: they would go out under this version either way.",
  );
}

run("git", ["fetch", "origin", "main", "--tags", "--quiet"]);
if (said("git", ["rev-parse", "HEAD"]) !== said("git", ["rev-parse", "origin/main"])) {
  stop("main and origin/main disagree.", "Push or pull first.");
}

// ---- 2. The number -------------------------------------------------------

const was = JSON.parse(readFileSync("package.json", "utf8")).version;
const asNumbers = (v) => v.split(".").map(Number);
const [wasMajor, wasMinor, wasPatch] = asNumbers(was);
const [isMajor, isMinor, isPatch] = asNumbers(version);
const newer =
  isMajor > wasMajor ||
  (isMajor === wasMajor && isMinor > wasMinor) ||
  (isMajor === wasMajor && isMinor === wasMinor && isPatch > wasPatch);

if (!newer) stop(`${version} is not newer than ${was}.`);

if (said("git", ["tag", "--list", tag])) {
  stop(`${tag} already exists.`, `Delete it first if it was never published.`);
}

// ---- 3. The one part that is not mechanical ------------------------------

try {
  execFileSync("node", ["scripts/changelog.mjs", "extract", version], { stdio: "pipe" });
} catch {
  stop(
    `CHANGELOG.md has no "## ${version}" section.`,
    "Write it first. `npm run changelog -- research` prints the diff to write from.",
  );
}

if (dryRun) {
  console.log(`\nevery check passes. ${was} -> ${version} would be cut.`);
  process.exit(0);
}

// ---- 4. The tests, before anything is written ---------------------------

// Before the bump rather than after it, so a failure here leaves the tree
// exactly as this found it and the whole command can be run again. What the
// bump changes afterwards is version strings and one generated page, and the
// two checks at the end of step 5 cover precisely those.
console.log("\nverifying before tagging. This is the slow part.\n");
npmRun("verify");

// ---- 5. The copies of the version, and the page that carries it ----------

run("node", ["scripts/set-version.mjs", version]);

// The cost page names the version in its header and the build refuses a copy
// that does not match the measurements, so a bump without this fails an hour
// in. It takes no readings; it reassembles the page from what is recorded.
run("node", ["scripts/benchmark-page.mjs"]);

// Every other version in the tree is a copy that cannot read package.json, and
// this is what refuses to let them drift. Seconds, and they are the first two
// things the release build checks.
run("node", ["scripts/verify-source.mjs"]);
run("node", ["scripts/benchmark-page.mjs", "--check"]);

// ---- 6. Out -------------------------------------------------------------

run("git", ["add", "-A"]);
run("git", ["commit", "-m", version]);
run("git", ["tag", "-a", tag, "-m", version]);
run("git", ["push", "origin", "main"]);
run("git", ["push", "origin", tag]);

console.log(`
${tag} is pushed.

The release workflow builds it, waits for verify on this commit, and publishes
if that passed. Nothing else to do.

  gh run watch $(gh run list --workflow release --limit 1 --json databaseId --jq '.[0].databaseId')
`);
