/**
 * Writes the public page that says what Sill costs.
 *
 * The efficiency claim is the pitch, so the page's job is not to be
 * impressive. It is to be checkable: every number says what was measured, on
 * what machine, in which build, on which day, and which command a reader runs
 * to take the same reading themselves.
 *
 * Nothing here invents a number and nothing here is allowed to be typed. The
 * page is `scripts/benchmarks.json` (what Sill claims and what it is allowed
 * to cost, which are decisions) crossed with `docs/measurements/*.json` (what
 * a measuring script concluded, which are facts). A claim with no measurement
 * is printed as a claim with no measurement rather than left out, because a
 * page that quietly omits the row that got worse is worse than no page.
 *
 * `--check` regenerates it and fails if the file on disk differs. That is what
 * stops somebody editing a number into the page: `npm run verify` runs it, and
 * so does the release workflow before it spends an hour compiling.
 *
 *   node scripts/benchmark-page.mjs
 *   node scripts/benchmark-page.mjs --check
 */
import { readFileSync, writeFileSync, readdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const CATALOGUE = join(ROOT, "scripts", "benchmarks.json");
const RECORDS = join(ROOT, "docs", "measurements");
const PAGE = join(ROOT, "docs", "benchmark.md");

const check = process.argv.includes("--check");

/** The measurements, one file per row, each written by a measuring script. */
function records() {
  if (!existsSync(RECORDS)) return new Map();

  const found = new Map();
  for (const name of readdirSync(RECORDS)) {
    if (!name.endsWith(".json")) continue;
    const one = JSON.parse(readFileSync(join(RECORDS, name), "utf8"));
    found.set(one.id, one);
  }
  return found;
}

/**
 * A machine is named once and referred to by a letter after that.
 *
 * Provenance per row is the point, and a row that carries the whole
 * description of the machine is a row nobody reads to the end of. The letters
 * are assigned in the order the machines appear so the legend under each table
 * reads top to bottom.
 */
function machines(rows) {
  const seen = new Map();
  for (const row of rows) {
    if (!row.taken) continue;
    if (!seen.has(row.taken.machine)) {
      seen.set(row.taken.machine, String.fromCharCode(65 + seen.size));
    }
  }
  return seen;
}

/** Nothing typed into a table cell may split the row it is in. */
function cell(text) {
  return String(text).replace(/\|/g, "\\|");
}

function wrap(text, width = 78) {
  const out = [];
  let line = "";
  for (const word of text.split(/\s+/)) {
    if (line && line.length + 1 + word.length > width) {
      out.push(line);
      line = word;
    } else {
      line = line ? `${line} ${word}` : word;
    }
  }
  if (line) out.push(line);
  return out.join("\n");
}

/**
 * Whether a reading can be compared to the budget beside it at all.
 *
 * A release build and a development build are two orders of magnitude apart in
 * the part of this that draws pixels, so a millisecond or a megabyte taken on
 * the wrong one is not a figure anybody can use. It says so in the reading
 * itself rather than in a note at the bottom of the page.
 *
 * The exceptions are counts and structural facts, which the build does not
 * change, and they are named in the catalogue one by one.
 */
function provisional(row) {
  return row.taken && row.taken.build !== "release" && row.buildChanges;
}

/**
 * "One reading is" or "Three readings are", which is the whole of it.
 *
 * Written out because the alternative is a sentence built with a plural `s`
 * glued on, and this page is the most public thing in the repository.
 */
function many(n, one, more) {
  const words = ["no", "One", "Two", "Three", "Four", "Five", "Six", "Seven"];
  const said = n < words.length ? words[n] : String(n);
  return `${said} ${n === 1 ? one : more}`;
}

function table(rows, letters, version) {
  const out = [
    "| What it means | Reading | Allowed | Taken |",
    "| --- | --- | --- | --- |",
  ];

  for (const row of rows) {
    let reading = "**not measured yet**";
    let taken = "never";

    if (row.taken) {
      reading = cell(row.taken.reading);
      if (provisional(row)) reading += " (provisional)";
      if (row.taken.within === false) reading = `**over budget:** ${reading}`;

      taken = `${row.taken.on}, ${row.taken.build} build, machine ${
        letters.get(row.taken.machine)
      }`;

      // A reading taken against an earlier version is still a reading, and it
      // is a different claim from one taken against this one. Said in the row
      // rather than left for somebody to work out from the date.
      if (row.taken.version !== version) {
        taken += `, **version ${row.taken.version}**`;
      }
    }

    out.push(
      `| ${cell(row.means)} | ${reading} | ${
        row.budget ? cell(row.budget) : "no budget, reported so a change shows"
      } | ${taken} |`,
    );
  }

  return out.join("\n");
}

/**
 * The commands, one per script rather than one per row.
 *
 * A row with no command is a cost nothing takes a reading of. It is left out of
 * this block and named in the table at the bottom instead, because a command
 * printed here is a promise that running it produces the row above.
 */
function commands(rows) {
  const out = [];
  for (const row of rows) {
    if (row.how && !out.includes(row.how)) out.push(row.how);
  }
  return ["```bash", ...out, "```"].join("\n");
}

function legend(rows, letters) {
  const used = new Set(
    rows.filter((row) => row.taken).map((row) => row.taken.machine),
  );
  if (used.size === 0) return "";

  const lines = [];
  for (const [description, letter] of letters) {
    if (!used.has(description)) continue;
    lines.push(`- **Machine ${letter}:** ${description}`);
  }
  return `${lines.join("\n")}\n`;
}

function page(catalogue, taken, version, today) {
  const same = new Set(catalogue.buildDoesNotChange.ids);
  const rows = catalogue.rows.map((row) => ({
    ...row,
    taken: taken.get(row.id),
    buildChanges: !same.has(row.id),
  }));
  const letters = machines(rows);

  const anywhere = rows.filter((row) => row.runs === "anywhere");
  const dedicated = rows.filter((row) => row.runs === "dedicated");

  const missing = rows.filter((row) => !row.taken);
  const over = rows.filter((row) => row.taken && row.taken.within === false);
  const unproven = rows.filter(provisional);

  const parts = [];

  parts.push("# What Sill costs");
  parts.push(
    wrap(
      "Sill is meant to be almost free to leave running, and that is a claim " +
        "about numbers rather than a feeling. This page is those numbers. " +
        "Every row says what was measured, on which machine, in which build " +
        "and on what day, and under each table is the command that takes " +
        "those readings on your own machine. Where a cost has no reading, " +
        "or no command that would take one, the row says that instead.",
    ),
  );
  parts.push(
    wrap(
      `Generated for version ${version} on ${today}. Nothing on this page is ` +
        "written by hand: it is assembled from what the measuring scripts " +
        "concluded, and the build refuses a copy that has been edited.",
    ),
  );

  if (unproven.length > 0) {
    parts.push(
      wrap(
        `**${many(unproven.length, "reading is", "readings are")} provisional ` +
          `and cannot be compared to the ${
            unproven.length === 1 ? "budget beside it" : "budgets beside them"
          }.** A development build and a release build are two orders of ` +
          "magnitude apart in the part of this that draws pixels, so a " +
          "reading taken on anything other than a release build proves the " +
          "measurement arrives and nothing more. Each one is marked in its " +
          "own row.",
      ),
    );
  }

  if (over.length > 0) {
    parts.push(
      wrap(
        `**${many(over.length, "measurement is", "measurements are")} over ` +
          "what it is allowed to cost:** " +
          over.map((row) => row.means.toLowerCase()).join("; ") +
          ". The row says so rather than the page leaving it out.",
      ),
    );
  }

  if (missing.length > 0) {
    parts.push(
      wrap(
        `**${many(missing.length, "cost has", "costs have")} no measurement ` +
          `yet, of the ${rows.length} here.** They are listed at the bottom ` +
          "with what would take one. A page that showed only the rows with " +
          "flattering numbers on them would not be worth reading.",
      ),
    );
  }

  parts.push("## Costs that mean the same on any machine");
  parts.push(
    wrap(
      "Counts, ratios and sizes. None of them depends on how fast the " +
        "machine is or what else it was doing, so these are checked on every " +
        "build, on whatever hardware happens to run it, and a reader gets the " +
        "same answer.",
    ),
  );
  parts.push(table(anywhere, letters, version));
  parts.push("Take these yourself:");
  parts.push(commands(anywhere));
  const anywhereLegend = legend(anywhere, letters);
  if (anywhereLegend) parts.push(anywhereLegend.trimEnd());

  parts.push("## Costs that need a machine nobody is using");
  parts.push(
    wrap(
      "Milliseconds and megabytes of a running launcher. These need a real " +
        "window, a real display and a machine that is not doing anything " +
        "else, so they are taken on one machine set aside for them rather " +
        "than wherever a build happens to run. Reading them off a busy " +
        "machine measures the machine.",
    ),
  );
  parts.push(table(dedicated, letters, version));
  parts.push("Take these yourself:");
  parts.push(commands(dedicated));
  const dedicatedLegend = legend(dedicated, letters);
  if (dedicatedLegend) parts.push(dedicatedLegend.trimEnd());

  parts.push("## Costs with no measurement yet");

  if (missing.length === 0) {
    parts.push("Every cost on this page has a reading behind it.");
  } else {
    parts.push(
      wrap(
        "Named rather than left out. Having no reading is a different thing " +
          "from costing nothing, and a row that says which is which is worth " +
          "more than a page with only the flattering rows on it.",
      ),
    );
    parts.push(
      [
        "| What it means | What would measure it |",
        "| --- | --- |",
        ...missing.map(
          (row) =>
            `| ${cell(row.means)} | ${
              row.recordedBy
                ? `\`${cell(row.recordedBy)}\`, which has not written one down`
                : `**nothing yet:** ${cell(row.noReading)}`
            } |`,
        ),
      ].join("\n"),
    );
  }

  parts.push("## How a reading gets onto this page");
  parts.push(
    wrap(
      "A measuring script decides its own verdict and writes it to " +
        "`docs/measurements/`, carrying the machine, the build, the day and " +
        "the version it was taken against. This page is assembled from those " +
        "files and from `scripts/benchmarks.json`, which holds what Sill " +
        "claims and what each cost is allowed to be. Budgets are decided and " +
        "so they are written down; readings are taken and so they are never " +
        "written down.",
    ),
  );
  parts.push(
    wrap(
      "That split is what makes the page checkable. No number here can be " +
        "improved by editing this file: `npm run verify` regenerates it and " +
        "fails if what it produces is not what is committed.",
    ),
  );
  parts.push("```bash\nnode scripts/benchmark-page.mjs\n```");

  return `${parts.join("\n\n")}\n`;
}

const catalogue = JSON.parse(readFileSync(CATALOGUE, "utf8"));
const taken = records();

/*
 * A measurement whose row was renamed or removed.
 *
 * Dropped silently, this is the exact failure the page is designed against:
 * the reading still exists, the script still takes it, and the page stops
 * mentioning it with nothing saying why.
 */
const known = new Set(catalogue.rows.map((row) => row.id));
const orphans = [...taken.keys()].filter((id) => !known.has(id));
if (orphans.length > 0) {
  console.error(
    `measurements with no row in scripts/benchmarks.json: ${orphans.join(", ")}`,
  );
  process.exit(1);
}

/*
 * The same, for the list of costs the build does not change.
 *
 * A typo there would quietly put a row back among the provisional ones, which
 * is the safe direction, and a renamed row would quietly leave one out of
 * them, which is not. Neither should be silent.
 */
const strangers = catalogue.buildDoesNotChange.ids.filter((id) => !known.has(id));
if (strangers.length > 0) {
  console.error(
    `scripts/benchmarks.json says the build does not change costs it does not have: ${strangers.join(", ")}`,
  );
  process.exit(1);
}

const version = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8")).version;

/*
 * The day the newest reading was taken, not the day the page was generated.
 *
 * Regenerating changes nothing about what was measured, and a date that moved
 * every time somebody ran a script would make a stale page look fresh. It also
 * keeps `--check` from failing at midnight.
 */
const days = [...taken.values()].map((one) => one.on).sort();
const today = days.length > 0 ? days[days.length - 1] : "no readings yet";

const rendered = page(catalogue, taken, version, today);

if (check) {
  /*
   * Compared with line endings set aside.
   *
   * `.gitattributes` keeps the page at LF so this normally never matters, and
   * if it ever does, a checkout that wrote CRLF is not somebody editing a
   * number in. Failing on that would be red for a reason nobody could act on,
   * which is the thing that teaches people to ignore red.
   */
  const flat = (text) => text.replace(/\r\n/g, "\n");

  const onDisk = existsSync(PAGE) ? readFileSync(PAGE, "utf8") : "";
  if (flat(onDisk) !== flat(rendered)) {
    console.error(
      "docs/benchmark.md is not what the measurements say. Run: node scripts/benchmark-page.mjs",
    );
    process.exit(1);
  }
  console.log("the published cost page matches the measurements");
} else {
  writeFileSync(PAGE, rendered, "utf8");
  console.log(
    `docs/benchmark.md written: ${taken.size} reading(s) across ${catalogue.rows.length} costs`,
  );
}
