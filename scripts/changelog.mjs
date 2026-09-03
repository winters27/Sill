/**
 * Two halves of a changelog that is written rather than generated.
 *
 * `research` reads the diff for a range and prints what somebody needs in
 * front of them to write the entry: which areas moved, what appeared and what
 * went away, which commands and settings are new, and the commit bodies in
 * full. It deliberately does not produce a changelog. A list of commit
 * subjects is not one: half of them are merges, most name the mechanism
 * rather than what changed for whoever uses the thing, and pasting that into
 * a release reads as nobody having looked.
 *
 * `extract` reads a version's section back out of `CHANGELOG.md` and fails if
 * it is not there. That is the half the release workflow runs, and it is what
 * makes the writing compulsory: a tag cannot become a release until the
 * section exists, so the notes are always somebody's sentences.
 *
 * Run:
 *   node scripts/changelog.mjs research [<range>]
 *   node scripts/changelog.mjs extract <version>
 */
import { readFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const CHANGELOG = join(root, "CHANGELOG.md");

function git(...args) {
  return execFileSync("git", args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
}

/**
 * Git's empty tree, which every repository has and none of them stores.
 *
 * Diffing against it is the whole history as one change, including the initial
 * commit. `<root>..HEAD` would leave the initial commit's contents out, and
 * passing no range at all is worse than either: `git diff` with no arguments
 * means the working tree, so a first release researched itself and reported
 * whatever happened to be uncommitted.
 */
const NOTHING = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/**
 * The range to research, when nobody named one.
 *
 * The newest tag to HEAD is what a second release wants. There are no tags
 * before the first one, so it falls back to everything, which is the honest
 * answer for a first release even though it is a lot to read.
 */
function defaultRange() {
  const tags = git("tag", "--list", "v*", "--sort=-v:refname").trim();
  if (!tags) return `${NOTHING}..HEAD`;
  return `${tags.split("\n")[0]}..HEAD`;
}

/**
 * Which part of Sill a changed path belongs to.
 *
 * Coarse on purpose. The point is to see at a glance that a release is mostly
 * the extension host, or mostly settings, because that decides what the entry
 * leads with. A per-file list answers a different question and is already one
 * `git diff --stat` away.
 */
function area(path) {
  const rust = path.match(/^src-tauri\/src\/([^/]+)/);
  if (rust) return `rust: ${rust[1].replace(/\.rs$/, "")}`;
  if (path.startsWith("src-tauri/tests/")) return "rust: tests";
  if (path.startsWith("src-tauri/")) return "rust: crate";

  const lib = path.match(/^src\/lib\/([^/]+)/);
  if (lib) return `window: ${lib[1].replace(/\.(ts|svelte|js)$/, "")}`;

  const route = path.match(/^src\/routes\/([^/]+)/);
  if (route) return `window: route ${route[1]}`;
  if (path.startsWith("src/")) return "window";

  if (path.startsWith("host/")) return "extension host";
  if (path.startsWith("scripts/")) return "scripts";
  if (path.startsWith(".github/")) return "ci";
  if (path.startsWith("docs/") || path.endsWith(".md")) return "docs";
  return "other";
}

/** Lines added by this diff that match a pattern, with the file they landed in. */
function added(diff, pattern) {
  const found = [];
  let file = null;

  for (const line of diff.split("\n")) {
    const header = line.match(/^\+\+\+ b\/(.+)$/);
    if (header) {
      file = header[1];
      continue;
    }
    if (!line.startsWith("+") || line.startsWith("+++")) continue;

    const hit = line.slice(1).match(pattern);
    if (hit) found.push({ file, what: hit[1] ?? hit[0] });
  }

  return found;
}

function research(range) {
  // Never empty. `git diff` with no range is the working tree, and `git log`
  // with no range is every commit, so an absent range would ask two different
  // questions of the same word.
  const rangeArgs = [range];

  const numstat = git("diff", "--numstat", ...rangeArgs)
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const [plus, minus, path] = line.split("\t");
      return { plus: Number(plus) || 0, minus: Number(minus) || 0, path };
    });

  const status = git("diff", "--name-status", ...rangeArgs)
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => line.split("\t"));

  const commits = git(
    "log",
    "--no-merges",
    "--format=%x00%H%x1f%an%x1f%ad%x1f%s%x1f%b",
    "--date=short",
    ...rangeArgs,
  )
    .split("\0")
    .filter((entry) => entry.trim())
    .map((entry) => {
      const [hash, author, date, subject, body] = entry.split("\x1f");
      return { hash, author, date, subject, body: (body ?? "").trim() };
    });

  const out = [];
  const say = (line = "") => out.push(line);

  say(`# Changelog research: ${range}`);
  say();
  say(
    `${commits.length} commit(s) excluding merges, ${numstat.length} file(s), ` +
      `+${numstat.reduce((n, f) => n + f.plus, 0)} ` +
      `-${numstat.reduce((n, f) => n + f.minus, 0)}.`,
  );
  say();
  say("Notes to write the entry from. This is not the entry.");
  say();

  say("## Where the work went");
  say();
  const areas = new Map();
  for (const file of numstat) {
    const key = area(file.path);
    const seen = areas.get(key) ?? { plus: 0, minus: 0, files: 0 };
    seen.plus += file.plus;
    seen.minus += file.minus;
    seen.files += 1;
    areas.set(key, seen);
  }
  const ranked = [...areas].sort((a, b) => b[1].plus + b[1].minus - a[1].plus - a[1].minus);
  say("| Area | Files | + | - |");
  say("| --- | --- | --- | --- |");
  for (const [name, n] of ranked) say(`| ${name} | ${n.files} | ${n.plus} | ${n.minus} |`);
  say();

  const appeared = status.filter(([code]) => code.startsWith("A")).map(([, path]) => path);
  const gone = status.filter(([code]) => code.startsWith("D")).map(([, path]) => path);
  const renamed = status.filter(([code]) => code.startsWith("R"));

  if (appeared.length || gone.length || renamed.length) {
    say("## What appeared and what went away");
    say();
    say("A new module or a deleted one is usually the headline, and a rename is");
    say("usually not news at all. Read this before the commit bodies.");
    say();
    for (const path of appeared) say(`- new  ${path}`);
    for (const path of gone) say(`- gone ${path}`);
    for (const [, from, to] of renamed) say(`- moved ${from} -> ${to}`);
    say();
  }

  // The diff, not the log, because a command added and then removed again in
  // the same range is not something to announce.
  const diff = git("diff", "--unified=0", ...rangeArgs);

  // Naming the function would need the line under the attribute, which a
  // unified diff does not reliably carry. The file is enough to go and read.
  const attributed = added(diff, /#\[tauri::command\]/);

  if (attributed.length) {
    say("## New IPC surface");
    say();
    say(`${attributed.length} new \`#[tauri::command]\` attribute(s).`);
    say("Anything the window can now ask for that it could not before is a");
    say("user-visible change, even when the commit that added it was about");
    say("something else. Read these files.");
    say();
    for (const hit of attributed) say(`- ${hit.file}`);
    say();
  }

  const deps = added(diff, /^\s*"?([a-z0-9@/_-]+)"?\s*[=:]\s*[{"]/i).filter((hit) =>
    /Cargo\.toml$|package\.json$/.test(hit.file ?? ""),
  );
  if (deps.length) {
    say("## Dependencies added");
    say();
    say("Worth a line in the entry only when somebody has to install something,");
    say("but always worth reading: a new dependency is a new licence and a new");
    say("thing that can break a build.");
    say();
    for (const hit of deps) say(`- ${hit.what}  (${hit.file})`);
    say();
  }

  const items = new Set();
  for (const commit of commits) {
    for (const hit of `${commit.subject}\n${commit.body}`.matchAll(/\bP\d-\d\d\b/g)) {
      items.add(hit[0]);
    }
  }
  if (items.size) {
    say("## Checklist items named in the range");
    say();
    say([...items].sort().join(", "));
    say();
  }

  say("## The commit bodies, in full");
  say();
  say("Sill's commit messages are prose about why, so this is the research and");
  say("not a summary of it. What goes in the entry is what somebody using Sill");
  say("would notice, which is a subset, said differently.");
  say();
  for (const commit of commits) {
    say(`### ${commit.subject}`);
    say();
    say(`\`${commit.hash.slice(0, 10)}\`  ${commit.date}  ${commit.author}`);
    say();
    if (commit.body) say(commit.body);
    else say("_No body._");
    say();
  }

  return out.join("\n");
}

/**
 * A version's section of `CHANGELOG.md`, as the release body.
 *
 * Headings are `## <version>`, so `## 0.2.0`, and the section runs to the next
 * `## `. Missing or empty is a failure rather than an empty release body: an
 * unwritten changelog is the one thing a release should stop for, and it is
 * cheap to fix and impossible to fix after the fact.
 */
function extract(version) {
  if (!existsSync(CHANGELOG)) {
    console.error(`no CHANGELOG.md at ${CHANGELOG}`);
    process.exit(1);
  }

  const text = readFileSync(CHANGELOG, "utf8");
  const heading = new RegExp(`^## ${version.replace(/\./g, "\\.")}\\b.*$`, "m");
  const at = text.match(heading);

  if (!at) {
    console.error(
      `CHANGELOG.md has no \`## ${version}\` section.\n` +
        "Write it before tagging: run `npm run changelog -- research` for the\n" +
        "diff, then say what somebody using Sill would notice.",
    );
    process.exit(1);
  }

  const after = text.slice(at.index + at[0].length);
  const next = after.search(/^## /m);
  const body = (next === -1 ? after : after.slice(0, next)).trim();

  if (!body) {
    console.error(`the \`## ${version}\` section of CHANGELOG.md is empty`);
    process.exit(1);
  }

  return body;
}

const [mode, argument] = process.argv.slice(2);

if (mode === "research") {
  process.stdout.write(`${research(argument ?? defaultRange())}\n`);
} else if (mode === "extract") {
  if (!argument) {
    console.error("extract needs a version, as in `extract 0.2.0`");
    process.exit(1);
  }
  process.stdout.write(`${extract(argument)}\n`);
} else {
  console.error(
    "usage:\n" +
      "  node scripts/changelog.mjs research [<range>]\n" +
      "  node scripts/changelog.mjs extract <version>",
  );
  process.exit(1);
}
