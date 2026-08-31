/**
 * Checks the source tree for damage that compiles.
 *
 * Everything here is a mistake that a compiler, a type checker and a test suite
 * all accept, which is why it needs a pass of its own. Each one has happened.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, extname } from "node:path";

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

console.log(
  failures === 0 ? "source verification passed" : `\n${failures} problem(s) found`,
);
process.exit(failures === 0 ? 0 : 1);
