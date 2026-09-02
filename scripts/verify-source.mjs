/**
 * Checks the source tree for damage that compiles.
 *
 * Everything here is a mistake that a compiler, a type checker and a test suite
 * all accept, which is why it needs a pass of its own. Each one has happened.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, extname } from "node:path";
import { spawnSync } from "node:child_process";

const SKIP = new Set([
  "node_modules",
  "target",
  ".git",
  "build",
  "dist",
  ".svelte-kit",
  "package",
]);

const READ = new Set([
  ".rs",
  ".ts",
  ".tsx",
  ".js",
  ".mjs",
  ".svelte",
  ".json",
  ".toml",
  ".css",
  ".html",
  ".md",
]);

/**
 * The character left behind when text is decoded as the wrong encoding.
 *
 * Built from its code point rather than written out. Spelling it here would
 * put one in this file and make the check fail on itself, which it did.
 */
const REPLACEMENT = String.fromCodePoint(0xfffd);

/**
 * The settings window, where every picker and field looks the same.
 *
 * The launcher and the extension surfaces are deliberately not here: they draw
 * their own chrome and answer to a different design.
 */
const SETTINGS_SURFACE = /[\\/]settings[\\/]/;

/** The file that owns the look, which cannot use itself. */
const OWNS_A_CONTROL = /Select\.svelte$/;

let failures = 0;

function fail(file, line, what) {
  failures += 1;
  console.error(`  ${file}${line ? `:${line}` : ""}  ${what}`);
}

/** Where in the file a byte offset lands, for a message worth reading. */
function lineOf(text, at) {
  return text.slice(0, at).split("\n").length;
}

function* sources(dir) {
  for (const name of readdirSync(dir)) {
    if (SKIP.has(name)) continue;

    const path = join(dir, name);
    const stat = statSync(path);

    if (stat.isDirectory()) yield* sources(path);
    else if (READ.has(extname(name))) yield path;
  }
}

console.log("checking source for damage that compiles");

for (const file of sources(".")) {
  const raw = readFileSync(file);

  /*
   * A stray NUL byte.
   *
   * Written twice by scripts whose escaping was mangled on the way to disk.
   * Rust accepts one inside a char literal and JavaScript accepts one inside a
   * string, so both compiled, both passed their tests, and the only outward
   * sign was `grep` quietly deciding the file was binary and refusing to
   * search it.
   */
  const nul = raw.indexOf(0);
  if (nul !== -1) {
    fail(file, lineOf(raw.toString("utf8"), nul), "contains a NUL byte");
    continue;
  }

  const text = raw.toString("utf8");

  /*
   * A replacement character.
   *
   * What is left when text is decoded as the wrong encoding and written back.
   * It renders as a black diamond and reads as a typo rather than as damage.
   */
  const bad = text.indexOf(REPLACEMENT);
  if (bad !== -1) {
    fail(file, lineOf(text, bad), "contains U+FFFD, so something was decoded wrongly");
  }

  /*
   * A conflict marker left in place.
   *
   * In a comment or a string these compile perfectly well.
   */
  const conflict = text.match(/^(<{7}|={7}|>{7})[ \t]/m);
  if (conflict) {
    fail(file, lineOf(text, conflict.index), `merge conflict marker ${conflict[1]}`);
  }

  /*
   * Design tokens, bypassed.
   *
   * `src/lib/theme/theme.css` is the only place a size or a colour is chosen.
   * That is not a preference, it is what the frontend decayed away from: 16
   * distinct font sizes across 101 sites and 74 inline accent alphas at 21
   * different opacities, each one added by somebody being reasonable about
   * one component.
   *
   * There is no escape hatch on purpose. Every size that existed has a token,
   * including `--text-hero`, `--text-micro` and the three `--glyph-*` steps
   * for glyphs that are sized like type but are not type. A rule with an
   * opt-out marker becomes a rule nobody keeps.
   */
  if (extname(file) === ".svelte") {
    for (const m of text.matchAll(/font-size:\s*[\d.]+px/g)) {
      fail(file, lineOf(text, m.index), `${m[0]} is a literal; use a --text-* or --glyph-* token`);
    }
    for (const m of text.matchAll(/rgba\(var\(--accent-rgb\)/g)) {
      fail(
        file,
        lineOf(text, m.index),
        "inline accent alpha; the accent means selection, match, focus or an " +
          "affirmative state, and each has a named token",
      );
    }

    /*
     * A surface that scrolls without saying how its scrollbar looks.
     *
     * Windows paints one itself, in its own colours, and it is the only thing
     * in the window that does not follow the theme. Six components carry the
     * two lines that fix it and the seventh forgot, which is what a rule
     * copied by hand always eventually does.
     *
     * File level rather than rule level, because a component that scrolls
     * anything needs the answer somewhere in it, and where is its business.
     */
    if (/overflow(-[xy])?:\s*(auto|scroll)/.test(text)) {
      const answered =
        text.includes("scrollbar-color") || text.includes("sill-scrolls");

      if (!answered) {
        fail(
          file,
          lineOf(text, text.search(/overflow(-[xy])?:\s*(auto|scroll)/)),
          "this scrolls but never says how; add the sill-scrolls class",
        );
      }
    }

    /*
     * A hand-rolled picker in the settings window.
     *
     * Windows draws an open `<select>` list itself, in a window of its own,
     * and starts it white whatever the page looks like. A panel that writes
     * its own picker therefore has to remember a rule for the options as
     * well, and nothing in its own styling hints that the rule is missing.
     * Three panels remembered and two did not; the two rendered white text
     * on a white list, readable only on the highlighted row.
     *
     * Only `<select>` is checked. A panel that forgets to style an
     * `<input>` is wrong too, but it is wrong visibly, in the panel, the
     * first time anybody looks at it. This is for the failure that hides.
     */
    if (SETTINGS_SURFACE.test(file) && !OWNS_A_CONTROL.test(file)) {
      for (const m of text.matchAll(/<select\b/g)) {
        fail(file, lineOf(text, m.index), "a bare <select> in settings; use Select.svelte");
      }
    }
  }

  /*
   * An `@import` that a rule has already silenced.
   *
   * CSS requires every `@import` to come before every other rule, so an import
   * written below one is not reported as a mistake: it is dropped, quietly,
   * and the stylesheet still parses and still applies. That happened here.
   * Satoshi's `@font-face` was declared above the Inter import, which took
   * Inter out of the build entirely, and the only outward sign was the
   * interface font setting falling through to a system face for anybody who
   * picked it.
   *
   * A `{` before the last `@import` is the whole test, because an `@import` is
   * a statement and never opens a block. Comments are blanked first so a brace
   * in prose does not count, and blanked rather than cut so the offsets still
   * land on the right lines.
   */
  if (extname(file) === ".css") {
    const bare = text.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, " "));
    const lastImport = bare.lastIndexOf("@import");
    const opens = lastImport === -1 ? -1 : bare.slice(0, lastImport).indexOf("{");

    if (opens !== -1) {
      fail(
        file,
        lineOf(text, opens),
        `a rule opens here, above the @import on line ${lineOf(text, lastImport)}, ` +
          "so that import is dropped and nothing says so",
      );
    }
  }
}

/*
 * A scratch directory nobody removes.
 *
 * `mkdtemp` hands back a new directory every call and never takes it away
 * again. One script used two of them per run, the view gate runs that script
 * once per extension, and nothing tidied up: fifteen hundred and ninety-nine
 * folders had accumulated on the machine this was found on, going back to the
 * first day the gate existed.
 *
 * A file that makes one has to say what removes it. `rmSync` is coarse as a
 * test and that is deliberate: the point is that somebody writing the next
 * one has to think about the end of its life, not that this can prove they
 * got it right.
 */
/*
 * Nothing runs an action except the registry.
 *
 * `ActionRegistry::perform` runs it and records it in the activity log, which
 * is what makes undo work after the launcher has closed. Calling `run` on an
 * action directly does the thing and remembers nothing.
 *
 * This exists because the log was first hooked into the one command the window
 * calls, under a comment claiming that was the only way an action could run.
 * There were three: that command, a bound key, and Enter on a row. Each was
 * found by the feature failing rather than by reading, so the rule is written
 * down instead.
 */
for (const file of sources("src-tauri/src")) {
  if (file.endsWith("action.rs")) continue;

  const text = readFileSync(file, "utf8");

  for (const m of text.matchAll(/\.run\(&ActionCtx/g)) {
    fail(
      file,
      lineOf(text, m.index),
      "an action run outside the registry; use ActionRegistry::perform so it " +
        "reaches the activity log and can be undone",
    );
  }
}

for (const file of sources("scripts")) {
  const text = readFileSync(file, "utf8");

  if (text.includes("mkdtempSync") && !text.includes("rmSync")) {
    fail(
      file,
      lineOf(text, text.indexOf("mkdtempSync")),
      "makes a scratch directory and never removes one; say what tidies it up",
    );
  }
}

/*
 * The row height, which lives in two places because it has to.
 *
 * Rust sizes the launcher window and cannot read CSS, so `window_height`
 * carries a copy of `--row-height`. A window sized from a stale copy clips its
 * last row, and that reads as a list refusing to scroll rather than as a
 * number being wrong. Nothing else can catch this: a Rust test asserting
 * `CHROME + rows * ROW` only restates the formula.
 */
const THEME = "src/lib/theme/theme.css";
const PREFS = "src-tauri/src/preferences.rs";
const css = readFileSync(THEME, "utf8").match(/--row-height:\s*([\d.]+)px/);
const rust = readFileSync(PREFS, "utf8").match(/const ROW: f64 = ([\d.]+)/);

if (!css) fail(THEME, null, "no --row-height, which preferences.rs mirrors");
else if (!rust) fail(PREFS, null, "no `const ROW`, which mirrors --row-height");
else if (Number(css[1]) !== Number(rust[1])) {
  fail(
    PREFS,
    null,
    `ROW is ${rust[1]} but --row-height is ${css[1]}px, so the window will clip its last row`,
  );
}

/*
 * Every built-in the extension host performs is one that names a permission.
 *
 * `perform_builtin` does for an extension what the extension cannot do for
 * itself, which means it reaches the same capabilities the API layer gates.
 * The dispatch and the capability table are two lists that have to agree, and
 * nothing but this makes them: adding an arm and forgetting the table gives
 * the new action an empty permission set, which is a way round the gate that
 * compiles and passes every test.
 */
{
  const LAUNCH = "src-tauri/src/commands/launch.rs";
  const text = readFileSync(LAUNCH, "utf8");
  const table = text.slice(text.indexOf("fn builtin_needs"));
  const gate = table.slice(0, table.indexOf("\n}"));

  const dispatch = text.slice(text.indexOf("async fn perform_builtin"));
  const performed = new Set(
    Array.from(dispatch.matchAll(/"(Action\.[A-Za-z]+)"\s*(?:\||=>)/g), (m) => m[1]),
  );

  for (const tag of performed) {
    if (gate.includes(`"${tag}"`)) continue;
    fail(
      LAUNCH,
      lineOf(text, text.indexOf(`"${tag}"`, text.indexOf("async fn perform_builtin"))),
      `${tag} is performed for an extension but is not in \`builtin_needs\`, ` +
        "so it asks for no permission and is a way round the gate",
    );
  }
}

/*
 * Every place a hotkey is registered records whether it took.
 *
 * Windows refuses a combination another application already owns, and there is
 * no second sign: the accelerator string is still in the preferences, the row
 * still shows it, and the key does nothing. The settings window can only mark
 * the row if the failure reached `HotkeyConflicts`, and for per-command
 * shortcuts it never did, so those rows were incapable of being right. On this
 * machine the summon key itself had been refused for weeks with nothing but a
 * log line to say so.
 *
 * Registration lives in two files, so this checks both rather than the one
 * that happens to be correct today.
 */
{
  for (const file of ["src-tauri/src/lib.rs", "src-tauri/src/bindings.rs"]) {
    const text = readFileSync(file, "utf8");
    const registrations = Array.from(text.matchAll(/\.on_shortcut\(/g));

    for (const at of registrations) {
      // The result is recorded near the call, before or just after whatever
      // logs it. A whole file's worth of slack would let an unrelated
      // registration borrow another's `note`.
      const nearby = text.slice(at.index, at.index + 1600);
      if (nearby.includes("conflicts.note(")) continue;

      fail(
        file,
        lineOf(text, at.index),
        "a hotkey is registered here and the result never reaches `HotkeyConflicts`, " +
          "so a key another application owns will look bound and do nothing",
      );
    }
  }
}

/*
 * Every custom property a component reads is one the theme defines.
 *
 * A `var(--nope)` with no fallback is not an error anywhere: the declaration
 * is dropped and the element keeps whatever it inherited, so a missing border
 * is invisible and a missing background looks like a deliberate transparent
 * one. Eight of these had accumulated, including `--line`, which drew every
 * border in the extension store, and `--text`, which coloured ten rows.
 *
 * Fallbacks are read as intent rather than as a definition: `var(--danger,
 * #d24b4b)` still names a token nobody defined, and it hides the fact by
 * looking correct. Both halves are refused.
 */
{
  const theme = readFileSync(THEME, "utf8");
  const defined = new Set(
    Array.from(theme.matchAll(/^\s*(--[\w-]+)\s*:/gm), (m) => m[1]),
  );

  const files = [
    ...sources("src/lib"),
    ...sources("src/routes"),
  ].filter((f) => /\.(svelte|css)$/.test(f) && !f.includes("theme.css"));

  for (const file of files) {
    const raw = readFileSync(file, "utf8");

    /*
     * Comments are prose about the code, not code.
     *
     * A note explaining that `var(--column, 872px)` was removed contains the
     * thing this refuses, and reporting it makes the check fire on its own
     * explanation. Blanked rather than cut, so every line number still points
     * at the right line.
     */
    const text = raw.replace(/\/\*[\s\S]*?\*\/|<!--[\s\S]*?-->/g, (c) =>
      c.replace(/[^\n]/g, " "),
    );

    /*
     * A component may define its own, scoped to itself, and the interesting
     * case is where it does it: `style="--columns: {n}"` on the element, so
     * the value can come from the script. Matched anywhere in the file rather
     * than at the start of a line for exactly that reason.
     */
    for (const m of text.matchAll(/(--[\w-]+)\s*:/g)) defined.add(m[1]);

    for (const m of text.matchAll(/var\(\s*(--[\w-]+)/g)) {
      if (defined.has(m[1])) continue;
      fail(
        file,
        lineOf(text, m.index),
        `reads ${m[1]}, which ${THEME} does not define, so the declaration is ` +
          "dropped and nothing looks wrong",
      );
    }
  }
}

/*
 * A panel that hands `commit` a new object must be given a `commit` that takes
 * one.
 *
 * The Shortcuts panel declares `commit: (next: Preferences) => void` and calls
 * `commit({ ...prefs, taps })`. The page passed a zero-argument `commit` that
 * snapshotted what it already held, so every write from that panel was
 * dropped: the hyper key, double-tap, the navigation preset, per-command
 * hotkeys. Nothing failed and nothing said so, because **a zero-argument
 * function satisfies a one-argument type**, and the panel is the one screen
 * whose settings are all verified by pressing keys somewhere else.
 *
 * It shipped that way for three days. Types cannot catch it and no test
 * rendered the panel, so this does.
 */
const SETTINGS_PAGE = "src/routes/settings/+page.svelte";
const page = readFileSync(SETTINGS_PAGE, "utf8");

for (const file of sources("src/lib/components/settings")) {
  const text = readFileSync(file, "utf8");
  const call = text.indexOf("commit({");
  if (call === -1) continue;

  const name = file.split(/[\\/]/).pop().replace(".svelte", "");
  const used = new RegExp(`<${name}\\b[^>]*commit=\\{commitWith\\}`).test(page);

  if (!used) {
    fail(
      file,
      lineOf(text, call),
      `hands \`commit\` a new settings object, so ${SETTINGS_PAGE} has to ` +
        `pass it \`commit={commitWith}\`; with the zero-argument \`commit\` ` +
        "the object is silently dropped",
    );
  }
}

/*
 * No font file is tracked by git.
 *
 * Satoshi is under the ITF Free Font License, which permits embedding it in a
 * desktop application but forbids making the file available through a
 * "repository" or "publicly accessible servers". It was tracked from the
 * initial commit until this check existed, but the remote it reached is
 * private and has never been forked, so the file was not distributed to
 * anybody. Keeping it out from here is a precaution against the day that
 * changes, not a remedy for something already done.
 *
 * `scripts/fetch-fonts.mjs` puts it on disk per machine and `.gitignore` keeps
 * it out. Neither stops `git add -f`, and neither stops a second font being
 * added later by somebody who never heard why the first one could not be.
 * This does.
 */
const FONTS = /\.(woff2?|ttf|otf|eot)$/i;
const tracked = spawnSync("git", ["ls-files"], { encoding: "utf8" });

if (tracked.status !== 0) {
  console.log("skip no git here, so the tracked-font check did not run");
} else {
  for (const file of tracked.stdout.split("\n").filter((f) => FONTS.test(f))) {
    fail(
      file,
      null,
      "a font is tracked. Fonts are fetched by `npm run fonts`, never committed",
    );
  }
}


console.log(
  failures === 0 ? "source verification passed" : `\n${failures} problem(s) found`,
);
process.exit(failures === 0 ? 0 : 1);
