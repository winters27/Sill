/**
 * Runs every command a store audit installed, and collects what the host could
 * not answer.
 *
 * The other half of `src-tauri/tests/store_audit.rs`. That test installs the
 * most-installed extensions and stops at the bundle, because whether an
 * extension *installs* is a fact about the store and whether it *runs* is a
 * fact about the host. This is the second question.
 *
 * `run-extension.mjs` already reports every API an extension asked for that the
 * host does not implement. Running it once tells you about one extension;
 * running it over a hundred commands tells you what the API surface is actually
 * missing, ranked by how many real extensions want it.
 *
 * Run the Rust side first, then:
 *
 * ```text
 * node scripts/audit-extensions.mjs
 * ```
 *
 * Nothing here is part of `npm run verify`. It needs a network, it runs a
 * hundred Node processes, and it takes minutes.
 */
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";

const root = resolve(import.meta.dirname, "..");
const manifestPath =
  process.argv[2] ?? join(tmpdir(), "sill-store-audit", "audit.json");

if (!existsSync(manifestPath)) {
  console.error(
    `No manifest at ${manifestPath}.\n` +
      "Run the installer first:\n" +
      "  cargo test --manifest-path src-tauri/Cargo.toml --test store_audit -- --ignored --nocapture",
  );
  process.exit(1);
}

const commands = JSON.parse(readFileSync(manifestPath, "utf8"));

/** How long one command gets before it is called hung. */
const TIMEOUT_MS = 30_000;

/**
 * Runs one command and returns what it needed.
 *
 * The runner prints a line per unimplemented API and a summary count, so the
 * parsing is deliberately shallow: what is wanted is the names, and inventing
 * a machine-readable channel for it would be a second format to keep in step.
 */
function run(entry) {
  return new Promise((done) => {
    const args = [
      join(root, "scripts", "run-extension.mjs"),
      entry.entrypoint,
      entry.extension,
    ];
    if (entry.mode === "no-view") args.push("--no-view");

    // The permissions installing granted. Without these the run measures the
    // default-deny path rather than what somebody who accepted an install
    // actually gets, which are very different numbers.
    if (entry.granted?.length) args.push("--grant", entry.granted.join(","));

    const child = spawn(process.execPath, args, {
      cwd: root,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let output = "";
    child.stdout.on("data", (chunk) => (output += chunk));
    child.stderr.on("data", (chunk) => (output += chunk));

    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      done({ ...entry, outcome: "timed out", gaps: [] });
    }, TIMEOUT_MS);

    child.on("close", () => {
      clearTimeout(timer);

      // The runner names each one it could not answer. Its own words, so a
      // new gap needs no change here to be counted.
      const gaps = [...output.matchAll(/unimplemented:?\s+([\w/.]+)/gi)].map((m) => m[1]);

      /*
       * A refused permission is its own outcome, and it has to be, because it
       * does not look like a crash from the outside: the extension dies at
       * `require` before it renders, and the runner then reports "produced no
       * view" exactly as a no-view command does. Counting those together said
       * 104 of 104 ran when most of them had not started.
       */
      const denied = [
        ...output.matchAll(/not allowed to ([^,]+), so "([^"]+)" is unavailable/g),
      ].map((m) => `${m[2]} (${m[1]})`);

      const crashed = output.includes("CRASH:") || /\bError:/.test(output);
      const rendered = output.includes("extension rendered");
      const noView = output.includes("extension produced no view");

      done({
        ...entry,
        outcome: denied.length
          ? "refused a permission"
          : crashed
            ? "crashed"
            : rendered
              ? "rendered"
              : noView
                ? "ran"
                : "nothing",
        denied: [...new Set(denied)],
        gaps: [...new Set(gaps)],
        output,
      });
    });
  });
}

/** A few at a time. A hundred Node processes at once helps nobody. */
const AT_ONCE = 4;

const results = [];
for (let i = 0; i < commands.length; i += AT_ONCE) {
  const batch = commands.slice(i, i + AT_ONCE);
  results.push(...(await Promise.all(batch.map(run))));
  process.stdout.write(`\r  ran ${results.length}/${commands.length}`);
}
process.stdout.write("\n\n");

const byOutcome = new Map();
const byGap = new Map();
const byDenial = new Map();

for (const result of results) {
  byOutcome.set(result.outcome, (byOutcome.get(result.outcome) ?? 0) + 1);
  for (const gap of result.gaps) {
    if (!byGap.has(gap)) byGap.set(gap, new Set());
    byGap.get(gap).add(result.extension);
  }
  for (const denial of result.denied ?? []) {
    if (!byDenial.has(denial)) byDenial.set(denial, new Set());
    byDenial.get(denial).add(result.extension);
  }
}

console.log("outcomes");
for (const [outcome, count] of [...byOutcome].sort((a, b) => b[1] - a[1])) {
  console.log(`  ${String(count).padStart(4)}  ${outcome}`);
}

console.log("\nPermissions refused at load, by how many extensions they stopped");
if (byDenial.size === 0) {
  console.log("  none");
} else {
  for (const [denial, who] of [...byDenial].sort((a, b) => b[1].size - a[1].size)) {
    console.log(`  ${String(who.size).padStart(4)}  ${denial}  (${[...who].join(", ")})`);
  }
}

console.log("\nAPIs the host does not answer, by how many extensions want one");
if (byGap.size === 0) {
  console.log("  none");
} else {
  for (const [gap, who] of [...byGap].sort((a, b) => b[1].size - a[1].size)) {
    console.log(`  ${String(who.size).padStart(4)}  ${gap}  (${[...who].join(", ")})`);
  }
}

const broken = results.filter((r) => r.outcome === "crashed" || r.outcome === "timed out");
if (broken.length) {
  console.log("\ncommands that did not run");
  for (const one of broken) {
    const why =
      one.output
        ?.split("\n")
        .find((line) => /Error:|not a function|undefined is not/.test(line))
        ?.trim() ?? one.outcome;
    console.log(`  ${one.id.padEnd(40)} ${why.slice(0, 100)}`);
  }
}
