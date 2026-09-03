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
 * A static mutable is a decision, not a convenience.
 *
 * Rule 2 of the constitution forbids "static mutable caches" and
 * "module-global state that effectively behaves as a singleton", even when
 * thread-safe. The reason is not thread safety: it is that a global cache is
 * unreachable from a test, invisible in a signature, and shared by everything
 * whether or not that was intended.
 *
 * Five were added in one afternoon of performance work, each one obviously
 * right on its own, which is exactly how this rule gets eroded. They are
 * managed state now. What is left below is named, with why, so a new one has
 * to be argued for here rather than simply appearing.
 */
{
  const ALLOWED = {
    // The `say!` macro is called from everywhere, including code that has no
    // handle to anything and no business acquiring one. A logger threaded
    // through every call site would be a worse design than this.
    "log.rs": ["FILE", "PATH", "WRITTEN"],

    /*
     * A `WH_KEYBOARD_LL` callback is a bare `extern "system" fn`. Windows
     * gives it no context pointer, so there is nowhere to put a handle and no
     * way to reach one. Every one of these is read or written from inside
     * that callback, which is an operating system constraint rather than a
     * design choice.
     */
    "dictation/hotkey.rs": [
      "CHORD",
      "CHORD_KEY_SEEN",
      "GENERATION",
      "HOOK_INSTALLED",
      "INJECTED_SEEN",
      "KEYS_SEEN",
      "LAST_MODS",
      "LISTENING",
      "OWN_SEEN",
      "SENDER",
      "TRIGGERS_SEEN",
      "TRIGGER_HELD",
    ],
    "snippets/expander.rs": ["APP", "EXPANDER", "KEYS_SEEN", "START"],

    // Read from `restore_foreground`, which runs while a window is being put
    // away and has only the window. Worth moving; not worth moving badly.
    "summon.rs": ["PREVIOUS_FOREGROUND"],

    /*
     * Known violations, inherited rather than introduced. Each is a
     * short-lived cache that belongs on a service: the audit lists them under
     * P2-07 and they are not fixed yet. Named here so the count cannot grow
     * quietly while they wait.
     */
    "commands/store.rs": ["WATCHING"],
    "everything_ipc.rs": ["QUERIES"],
    "sleep.rs": ["GENERATIONS"],
  };

  const held = /^\s*static\s+([A-Z][A-Z0-9_]*)\s*:/gm;

  /*
   * Where `thread_local!` blocks are, so their contents can be skipped.
   *
   * A thread-local is not a singleton. It is per-thread state, which is the
   * opposite of what rule 2 is about: nothing is shared and nothing is visible
   * from another thread. The two in this codebase exist because a window
   * procedure and a COM apartment are both per-thread by definition.
   */
  const perThread = (text) => {
    const spans = [];

    for (const start of text.matchAll(/thread_local!\s*\{/g)) {
      let depth = 0;
      let at = start.index + start[0].length - 1;

      for (; at < text.length; at += 1) {
        if (text[at] === "{") depth += 1;
        else if (text[at] === "}") {
          depth -= 1;
          if (depth === 0) break;
        }
      }

      spans.push([start.index, at]);
    }

    return spans;
  };

  for (const file of sources("src-tauri/src")) {
    const text = readFileSync(file, "utf8");
    const relative = file.replace(/\\/g, "/").replace("src-tauri/src/", "");
    const allowed = ALLOWED[relative] ?? [];
    const locals = perThread(text);

    for (const found of text.matchAll(held)) {
      const name = found[1];

      // A plain constant is not state. Only something with interior
      // mutability is what the rule is about.
      const line = text.slice(found.index, text.indexOf("\n", found.index + 1));
      if (!/Mutex|OnceLock|Atomic|ArcSwap|RwLock|Cell/.test(line)) continue;

      if (allowed.includes(name)) continue;
      if (locals.some(([from, to]) => found.index > from && found.index < to)) {
        continue;
      }

      fail(
        file,
        lineOf(text, found.index),
        `\`static ${name}\` is module-global mutable state, which rule 2 ` +
          "refuses even when thread-safe. Put it on a managed service, or " +
          "name it in the allowlist in this check with the reason it cannot be",
      );
    }
  }
}

/*
 * Nothing is read from disk before Sill answers its own hotkey.
 *
 * The number somebody feels on a cold start is "how long until the key
 * works", and setup used to do seven synchronous reads before stamping that:
 * the ranking history, the index cache, last run's file index, the snippets
 * and the quicklinks. None of them is needed for the key. They are what the
 * first search reads, and the first search happens after somebody has already
 * pressed it.
 *
 * Preferences are the exception and always will be: the hotkey to register is
 * written in them.
 */
{
  const LIB = "src-tauri/src/lib.rs";
  const text = readFileSync(LIB, "utf8");

  const stamp = text.indexOf("timings.ready(since_start)");
  const setup = text.lastIndexOf(".setup(", stamp);

  if (stamp < 0 || setup < 0) {
    fail(LIB, 1, "the startup path no longer has a `setup` and a `ready` stamp to check between");
  } else {
    const before = text.slice(setup, stamp);

    // Named one at a time rather than matched by shape, because the point is
    // the list: adding a read here should be a decision somebody makes and
    // writes down, not something that happens.
    const reads = [
      "Frecency::load",
      "registry::load_cache",
      "catalog.warm",
      "reload_snippets",
      "reload_quicklinks",
      "snippets::store::load",
      "quicklinks::store::load",
    ];

    for (const read of reads) {
      const at = before.indexOf(read);
      if (at < 0) continue;

      fail(
        LIB,
        lineOf(text, setup + at),
        `${read} runs before the hotkey is answered, so a cold start waits ` +
          "for a file nothing needs yet",
      );
    }
  }
}

/*
 * Every kind of row Rust can produce has a heading of its own.
 *
 * `groupOf` was a switch whose default returned "Applications", so a mode
 * nobody had thought about read as an application. Four did: a saved window
 * arrangement, a running process, a captured piece of text and a clipboard
 * entry. None of them is an application and every one of them said it was.
 *
 * This is the shape that has cost this project a session more than once. A
 * match over modes with a default makes forgetting silent, and the thing
 * forgotten looks like something else rather than looking wrong. So the
 * default is gone, and a mode with no heading fails here instead.
 */
{
  const LIST = "src/lib/list.ts";
  const headings = readFileSync(LIST, "utf8");
  const table = headings.slice(
    headings.indexOf("const HEADINGS"),
    headings.indexOf("export function groupOf"),
  );

  const named = new Set(
    Array.from(table.matchAll(/^\s*"?([a-z][a-z-]*)"?\s*:/gm), (m) => m[1]),
  );

  /*
   * Modes whose heading is deliberately the row's own extension title.
   *
   * A snippet is filed under the collection it is in, and Rust puts that in
   * the same field an extension command puts its extension: both answer the
   * question "which heading does this row go under". Naming it here says the
   * fallback was chosen rather than forgotten, which is the whole difference
   * this check exists to make.
   */
  const byExtension = new Set(["snippet"]);

  for (const file of sources("src-tauri/src")) {
    const rust = readFileSync(file, "utf8");

    for (const found of rust.matchAll(/\bmode:\s*"([a-z][a-z-]*)"/g)) {
      if (named.has(found[1]) || byExtension.has(found[1])) continue;

      fail(
        file,
        lineOf(rust, found.index),
        `mode "${found[1]}" has no heading in ${LIST}, so its rows are filed ` +
          "under whichever extension produced them rather than under a name",
      );
    }
  }
}

/*
 * The launcher window does not decide what Ctrl+K means.
 *
 * `navigation.rs` resolves every movement chord, presets and overrides
 * included, and it binds Ctrl+K to Previous under vim on purpose: the comment
 * on `Move::Actions` says so and offers Alt+Enter as the alternative. The
 * window tested for Ctrl+K itself, before consulting the map, so a vim user
 * got the action panel and the preset was a lie in the one place it was
 * documented not to be.
 *
 * Narrow on purpose: this is one key that was hardcoded, not a rule about
 * every chord. Adding it back is the mistake worth catching.
 */
{
  const PAGE = "src/routes/+page.svelte";
  const text = readFileSync(PAGE, "utf8");
  const hardcoded = /["']k["']\s*&&\s*\(?\s*event\.(ctrl|meta)Key/;
  const found = text.match(hardcoded);

  if (found) {
    fail(
      PAGE,
      lineOf(text, found.index),
      "Ctrl+K is decided here rather than by the chord map, so a preset that " +
        "binds it to something else is ignored and the settings screen lies",
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
