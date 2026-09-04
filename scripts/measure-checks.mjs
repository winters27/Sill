/**
 * Takes the readings that mean the same on any machine.
 *
 * Counts, ratios and structural facts. None of them depends on how fast the
 * machine is or what else it was doing, so unlike everything in
 * `scripts/nightly.ps1` these can be taken on a borrowed build agent and mean
 * exactly what they mean on a desk.
 *
 * Each one is run the way a reader would run it, and the reading that gets
 * written down is what that command said. Nothing here reimplements a check to
 * produce a number: the ranking figure comes from the probe that prints it,
 * the verdicts come from the tests that own the thresholds, and the timer count
 * comes from the rule in `verify-source.mjs` rather than from a second count
 * taken here.
 *
 *   node scripts/measure-checks.mjs
 *   node scripts/measure-checks.mjs --release
 *
 * A debug run is worth taking and is marked as one. Every budget in
 * `docs/budgets.md` is a release figure, so a debug reading on the page proves
 * the measurement arrives and nothing more, which is why the page prints it
 * with the word provisional on it.
 */
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import { describe } from "./machine.mjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const RECORDS = join(ROOT, "docs", "measurements");
const MANIFEST = ["--manifest-path", join("src-tauri", "Cargo.toml")];

const release = process.argv.includes("--release");
const build = release ? "release" : "debug";
const profile = release ? ["--release"] : [];

const version = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8")).version;
const machine = describe();
/*
 * The local day, not the UTC one.
 *
 * `toISOString` is UTC, and on a machine a few hours behind it that is
 * tomorrow's date for part of every evening. The PowerShell scripts write the
 * local day, so a page built from both would carry two dates for one session
 * of measuring.
 */
const now = new Date();
const on = [
  now.getFullYear(),
  String(now.getMonth() + 1).padStart(2, "0"),
  String(now.getDate()).padStart(2, "0"),
].join("-");

/*
 * Run without a shell, deliberately.
 *
 * Node warns about passing arguments through one, and it is right to: they are
 * concatenated rather than escaped, and one of these arguments is a path that
 * on somebody's machine has a space in it. `cargo` and `node` are executables
 * on the path, so nothing here needs a shell to find them.
 */
function run(command, args) {
  const said = spawnSync(command, args, { cwd: ROOT, encoding: "utf8" });

  if (said.error) {
    console.error(`  could not run ${command}: ${said.error.message}`);
    process.exit(1);
  }

  const text = `${said.stdout ?? ""}${said.stderr ?? ""}`;
  return { ok: said.status === 0, text };
}

function record(id, reading, within, by) {
  mkdirSync(RECORDS, { recursive: true });
  const one = { id, reading, within, build, machine, version, on, by };
  writeFileSync(
    join(RECORDS, `${id}.json`),
    `${JSON.stringify(one, null, 2)}\n`,
    "utf8",
  );
  console.log(`  recorded ${id}: ${reading}`);
}

/*
 * The number and the verdict come from two different commands, deliberately.
 *
 * The probe is `#[ignore]`d and asserts nothing, so it is where a figure can be
 * printed without a threshold beside it. The threshold lives in the test that
 * enforces it, and that test is what says whether this build is within it.
 * Reading the number out of one and the verdict out of the other is what keeps
 * the budget in exactly one place.
 */
{
  console.log("ranking");

  const probe = run("cargo", [
    "test",
    ...profile,
    ...MANIFEST,
    "--test",
    "budgets",
    "measured",
    "--",
    "--ignored",
    "--nocapture",
  ]);

  // `   1500 entries,  "visual" ->  3621 us`
  const worst = [...probe.text.matchAll(/^\s*1500 entries,\s*("[^"]*"|"")\s*->\s*(\d+) us/gm)]
    .map((one) => Number(one[2]))
    .sort((a, b) => b - a)[0];

  const held = run("cargo", [
    "test",
    ...profile,
    ...MANIFEST,
    "--test",
    "budgets",
    "ranking_a_whole_index_stays_within_one_keystroke",
  ]);

  if (worst === undefined) {
    console.error("  the probe printed no 1,500 entry reading, so nothing was recorded");
    console.error(probe.text.trim().split("\n").slice(-12).join("\n"));
    process.exit(1);
  }

  record(
    "ranking-one-keystroke",
    `${(worst / 1000).toFixed(1)} ms for the worst of four queries over 1,500 entries`,
    held.ok,
    "scripts/measure-checks.mjs",
  );

  const linear = run("cargo", [
    "test",
    ...profile,
    ...MANIFEST,
    "--test",
    "budgets",
    "ranking_grows_with_the_corpus_and_not_faster",
  ]);

  record(
    "ranking-growth",
    linear.ok ? "still linear" : "no longer linear",
    linear.ok,
    "scripts/measure-checks.mjs",
  );
}

/*
 * A check whose answer is whether it holds.
 *
 * There is no number to print. What a summon leaves behind is asked of the
 * structures directly rather than by opening a window five hundred times, so a
 * reading of "the check holds" with the command beside it is the whole of what
 * a reader can verify, and it is verifiable.
 *
 * The clipboard's write-ahead file is the other row of this shape and it is
 * deliberately not taken here. `docs/budgets.md` names `tests/clipboard_merge
 * .rs` as what holds it and there is no such file: those tests moved into the
 * library and none of the ones that moved measures the file's size. Running
 * something adjacent and writing down "the check holds" would publish a check
 * nothing performs, so the catalogue says nothing takes that reading and the
 * page prints it as a budget with no instrument.
 */
{
  console.log("structure");

  const summons = run("cargo", [
    "test",
    ...profile,
    ...MANIFEST,
    "--lib",
    "five_hundred_summons_leave_nothing_behind",
  ]);

  record(
    "summon-leaves-nothing",
    summons.ok ? "the check holds" : "the check does not hold",
    summons.ok,
    "scripts/measure-checks.mjs",
  );
}

/*
 * The timer count, read off the rule that owns it.
 *
 * `verify-source.mjs` prints what it counted. Counting again here would be a
 * second implementation of a rule whose whole value is that there is one, and
 * the two would drift the first time an exemption was added.
 */
{
  console.log("timers");

  const said = run("node", [join("scripts", "verify-source.mjs")]);
  const line = said.text.match(
    /^measurement no-unaccounted-timer :: (.+) :: (true|false)$/m,
  );

  if (!line) {
    console.error(
      "  verify-source.mjs printed no timer count, so this row would be a number typed here",
    );
    process.exit(1);
  }

  record(
    "no-unaccounted-timer",
    line[1],
    line[2] === "true",
    "scripts/measure-checks.mjs",
  );
}

console.log("");
console.log(`taken on a ${build} build. Now: node scripts/benchmark-page.mjs`);
