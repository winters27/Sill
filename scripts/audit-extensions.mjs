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
 * One kind of finding, off the runner's own summary lines.
 *
 * The runner prints `audit: <key>=<value>,<value>` for each kind it found
 * something of, and prints nothing for a kind it found none of, so an absent
 * line means "none" rather than "did not look". Values may carry a count as
 * `name:count`; a value without one counts as one.
 *
 * **This replaced scraping the prose above it, and the reason is worth
 * keeping.** The pattern it used, `/unimplemented:?\s+([\w/.]+)/i`, matched the
 * runner's own headline, "Unimplemented API surface this extension needs",
 * and captured the word "API". Every extension reported one gap called API,
 * including the extensions with no gaps at all, because the summary line
 * "no unimplemented API was needed" matched too. The ranking this whole
 * script exists to produce was a single meaningless row, and nothing failed,
 * because prose that has changed shape still reads as prose.
 *
 * `verify:source` holds these keys to the ones the runner emits, in both
 * directions, so a finding added on one side cannot be dropped on the other.
 */
function found(output, key) {
  const line = new RegExp(`^audit: ${key}=(.*)$`, "gm");

  return [...output.matchAll(line)].flatMap((match) =>
    match[1]
      .split(",")
      .filter(Boolean)
      .map((value) => {
        const at = value.lastIndexOf(":");
        const counted = at > 0 && /^\d+$/.test(value.slice(at + 1));
        return {
          name: counted ? value.slice(0, at) : value,
          count: counted ? Number(value.slice(at + 1)) : 1,
        };
      }),
  );
}

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

    /*
     * Long enough for one network round trip.
     *
     * Most of these fetch, and a view that has drawn its empty list is not a
     * view that has finished. At the gate's own quiet window this report said
     * "0 rows, no icons" for every extension whose rows come over the wire,
     * which is most of the interesting ones.
     */
    args.push("--settle", "5000");

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

      const gaps = found(output, "unimplemented").map((one) => one.name);

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
        lettered: found(output, "lettered-icon"),
        unresolved: found(output, "unresolved-icon"),
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

/**
 * Icon names, by how many extensions want one and how often it is drawn.
 *
 * Two numbers because they answer different questions. **How many extensions**
 * decides what to draw next: one name in forty extensions is worth an
 * afternoon and one name in one is not. **How many rows** says how much of a
 * screen it is: an icon on every row of a thirty row list is the whole list
 * reading as letters, which is what this looks like to somebody using it.
 */
const byIcon = new Map();
const byUnresolved = new Map();

const tally = (into, findings, extension) => {
  for (const { name, count } of findings) {
    if (!into.has(name)) into.set(name, { who: new Set(), rows: 0 });
    into.get(name).who.add(extension);
    into.get(name).rows += count;
  }
};

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
  tally(byIcon, result.lettered ?? [], result.extension);
  tally(byUnresolved, result.unresolved ?? [], result.extension);
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

/*
 * The half no other report can see.
 *
 * An icon name the window has no drawing for is not a failed call and raises
 * nothing at all: it falls back to a letter tile and the extension runs
 * perfectly. So an extension can be reported as fully supported by everything
 * above while every row of it draws a letter, which is the state Hacker News
 * shipped in.
 */
const ranked = (counted) =>
  [...counted].sort((a, b) => b[1].who.size - a[1].who.size || b[1].rows - a[1].rows);

console.log("\nIcon names drawn as letters, by how many extensions want one");
if (byIcon.size === 0) {
  console.log("  none");
} else {
  for (const [name, { who, rows }] of ranked(byIcon)) {
    console.log(
      `  ${String(who.size).padStart(4)}  ${name.padEnd(24)} ${String(rows).padStart(5)} row(s)  (${[...who].join(", ")})`,
    );
  }
}

/*
 * A different problem with the same symptom. These are extensions pointing at
 * a file they shipped, which the window cannot resolve because it does not
 * know where an installed extension lives on disk. No amount of artwork fixes
 * one; a route to those bytes does.
 */
console.log("\nExtension assets the window cannot resolve, by how many want one");
if (byUnresolved.size === 0) {
  console.log("  none");
} else {
  for (const [name, { who, rows }] of ranked(byUnresolved)) {
    console.log(
      `  ${String(who.size).padStart(4)}  ${name.padEnd(40)} ${String(rows).padStart(5)} row(s)  (${[...who].join(", ")})`,
    );
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
