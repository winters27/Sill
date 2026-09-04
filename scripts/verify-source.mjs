/**
 * Checks the source tree for damage that compiles.
 *
 * Everything here is a mistake that a compiler, a type checker and a test suite
 * all accept, which is why it needs a pass of its own. Each one has happened.
 */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, extname, sep } from "node:path";
import { spawnSync } from "node:child_process";

import { SOURCE, TAURI_POINTER, read, source, tauriVersion } from "./versions.mjs";

const SKIP = new Set([
  "node_modules",
  "target",
  ".git",
  "build",
  "dist",
  ".svelte-kit",
  "package",
  // A worktree is a whole second checkout living inside this one, usually at
  // some other commit. Walking into it means judging code that is not the code
  // being checked: one stale worktree reported 612 problems here, all of them
  // in files this tree does not have.
  ".claude",
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

/*
 * The gallery and the mock windows under `src/routes/preview/`.
 *
 * A development harness rather than an interface. It paints simulated desktop
 * wallpapers behind the real components so contrast can be measured against a
 * pale desktop as well as a dark one, which means its colours are test
 * fixtures: `#e8e4dc` there is somebody's wallpaper, not a surface Sill draws.
 * Theming a fixture would defeat the fixture.
 *
 * It also ships as a lazy chunk and is meant to be deleted before any release,
 * so a rule enforced here is churn on code with a known end date.
 */
const HARNESS = /[\\/]routes[\\/]preview[\\/]/;

/**
 * The parts of a component that style it: `<style>` blocks and `style=""`.
 *
 * Everything below reads only these, and that boundary is the rule rather than
 * an implementation detail. A canvas is not a stylesheet: the markup tool sets
 * `pen.strokeStyle` to pick an ink and computes black-or-white for contrast
 * against whatever colour it lands on, and an SVG brand mark carries the
 * vendor's own hex because that hex IS the mark. Those are program data. Only
 * what the browser parses as CSS answers to the design system.
 *
 * Offsets are kept so a failure still points at the right line of the file.
 */
function styled(text) {
  const parts = [];

  for (const m of text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) {
    parts.push([m.index + m[0].indexOf(">") + 1, m[1]]);
  }
  for (const m of text.matchAll(/\bstyle="([^"]*)"/g)) {
    parts.push([m.index + 7, m[1]]);
  }

  // Comments are prose. Blanked rather than cut so offsets still land.
  return parts.map(([at, css]) => [
    at,
    css.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, " ")),
  ]);
}

/**
 * A component with everything that is not markup blanked out.
 *
 * Comments, `<script>` and `<style>` are replaced by spaces of the same
 * length, so offsets still land on the right line and nothing inside them is
 * read as an element. That matters more than it sounds: half the comments in
 * this tree explain an attribute by quoting it, and a rule that says
 * `role="application"` is banned would otherwise be tripped by the comment
 * saying why it was removed.
 */
function markup(text) {
  const blank = (c) => c.replace(/[^\n]/g, " ");

  return text
    .replace(/<!--[\s\S]*?-->/g, blank)
    .replace(/<script[\s\S]*?<\/script>/g, blank)
    .replace(/<style[\s\S]*?<\/style>/g, blank);
}

/**
 * Every opening tag in a template, with its attributes as one string.
 *
 * Written by hand rather than with a regex because a Svelte attribute holds
 * arbitrary JavaScript: `onclick={() => go("<b>")}` contains both a quote and
 * a `>` and neither of them ends the tag. Quotes and braces are tracked so the
 * end of the tag is the first `>` that is in neither.
 */
function* tags(text) {
  for (const found of text.matchAll(/<([A-Za-z][\w:.-]*)/g)) {
    let at = found.index + found[0].length;
    let quote = "";
    let depth = 0;

    while (at < text.length) {
      const ch = text[at];

      if (quote) {
        if (ch === quote) quote = "";
      } else if (ch === '"' || ch === "'") {
        quote = ch;
      } else if (ch === "{") {
        depth += 1;
      } else if (ch === "}") {
        depth -= 1;
      } else if (ch === ">" && depth === 0) {
        break;
      }

      at += 1;
    }

    yield {
      name: found[1],
      attrs: text.slice(found.index + found[0].length, at),
      at: found.index,
    };
  }
}

/*
 * The literals a component may still write, and why each one is allowed.
 *
 * "Zero literals" is not literally the target and pretending otherwise would
 * make the rule dishonest. The failure being guarded against is DRIFT: two
 * components that agree on a value today and quietly stop agreeing tomorrow. A
 * value used once cannot drift, so a measurement of one thing stays where it
 * is measured.
 *
 *   A BORDER OR OUTLINE WIDTH. No rule below looks at one. A hairline is one
 *   device pixel by definition and cannot be anything else; naming it would
 *   add a lookup to the one number in the system that can never move. 2px is
 *   the same quantity doubled, which is how a control says it is focused.
 *
 *   Any length in `width`, `height`, `top`, `inset`, `transform`,
 *   `grid-template-columns` and their relatives, EXCEPT where the number
 *   collides with a named size (below). A settings panel that is 520px wide is
 *   a measurement of that panel; there is no scale it belongs to, and
 *   inventing `--panel-wide` would name one thing twice. Where two rules DO
 *   have to agree on such a number, a component declares its own custom
 *   property and both read it; `AiPanel.svelte` does that with the label
 *   column its field grid sets and its action row has to clear.
 *
 *   `0`, percentages, `em`, `ch`, `rem`, `fr`, `vh`. Not px and not colours.
 *
 *   Opacity. `0` and `1` are the two ends of an animation rather than design
 *   values, and they are the two most common, so the exception list would be
 *   longer than the rule and a rule like that teaches people to write
 *   exceptions. The scale is defined and adopted where a component was saying
 *   disabled, muted or decorative, which is where naming it buys anything.
 *
 *   Anything outside `<style>` and `style=""`. See `styled` above.
 */

/**
 * Sizes with one meaning, which is what makes them safe to check by value.
 *
 * A control that is 30px tall is the control, every time. A 26px square is the
 * icon tile, in either dimension. Nothing else on the size scale is checked:
 * 16px is an icon in one file and a switch knob in the next, and a guard that
 * cannot tell them apart would be renaming the knob for the sake of a count.
 */
const NAMED_SIZE = [
  [/^(min-|max-)?height$/, "40px", "--row-height"],
  [/^(min-|max-)?height$/, "60px", "--search-height"],
  [/^(min-|max-)?height$/, "30px", "--control-height"],
  [/^(min-|max-)?(width|height)$/, "26px", "--icon-tile"],
];

/**
 * The things that really are decoration, so may wear `--text-4`.
 *
 * Placeholders are allowed by shape rather than by name, because their
 * selector says what they are. These two do not, so each is named with why.
 * Adding a third should take an argument.
 */
const DECORATIVE = {
  /*
   * The Escape hint under the launcher's footer, which the token's own comment
   * in `theme.css` names as the second thing it is for. Escape is the key
   * nobody needs reminding of, so the reminder is deliberately at the edge of
   * being noticed.
   */
  "src/lib/components/Footer.svelte": [".escape"],

  /*
   * The pin on a widget tile, which is a glyph rather than words. It is
   * invisible until the tile is hovered and goes to `--text-1` the moment the
   * pointer is on it, so at no point is somebody reading a 0.26 alpha.
   */
  "src/lib/widgets/Board.svelte": [".pin"],
};

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
     * Everything else the theme owns: colour, motion, layering, spacing,
     * radius and elevation.
     *
     * The font-size rule above was the first of these and it worked, so this
     * is the same rule widened to the categories that decayed the same way
     * while nothing was watching them. The counts when it was written: 64
     * colour literals, 101 durations across six speeds for what turned out to
     * be three ideas, 62 spacing values, 74 hand-copied shadows and 8 raw
     * z-indexes.
     *
     * A shadow is in here because it is the sneakiest of them. Nobody chooses
     * a `0 16px 40px -12px` twice; they copy it, and then one copy gets tuned
     * and the interface has two heights that were meant to be one.
     */
    if (!HARNESS.test(file)) {
      for (const [at, css] of styled(text)) {
        const line = (index) => lineOf(text, at + index);

        for (const m of css.matchAll(/#[0-9a-fA-F]{3,8}\b|\brgba?\([^)]*\)|\bhsla?\([^)]*\)/g)) {
          // `rgba(var(--accent-rgb), a)` has its own message above.
          if (m[0].includes("var(--")) continue;
          fail(file, line(m.index), `${m[0]} is a literal colour; every colour has a token`);
        }

        for (const m of css.matchAll(/\b(transition|animation)(-duration|-delay)?\s*:\s*([^;}]*)/g)) {
          for (const d of m[3].matchAll(/(?<![\w-])[\d.]+m?s\b/g)) {
            fail(
              file,
              line(m.index),
              `${m[1]} runs for ${d[0]}; use a --motion-* token so the whole ` +
                "interface changes state at one speed",
            );
          }
        }

        for (const m of css.matchAll(/\bz-index\s*:\s*(-?\d+)/g)) {
          fail(
            file,
            line(m.index),
            `z-index ${m[1]} is a bare number, so what it stacks above is only ` +
              "knowable by reading every other component; use a --z-* token",
          );
        }

        for (const m of css.matchAll(
          /\b(padding|margin|gap|row-gap|column-gap)(-top|-right|-bottom|-left)?\s*:\s*([^;}]*)/g,
        )) {
          if (!/[\d.]px/.test(m[3])) continue;
          fail(file, line(m.index), `${m[1]}${m[2] ?? ""} is in px; use a --space-* token`);
        }

        for (const m of css.matchAll(/\bborder-radius\s*:\s*([^;}]*)/g)) {
          if (!/[\d.]px/.test(m[1])) continue;
          fail(file, line(m.index), "a literal border-radius; use a --radius-* token");
        }

        for (const m of css.matchAll(/\bbox-shadow\s*:\s*([^;}]*)/g)) {
          if (!/[\d.]px/.test(m[1])) continue;
          fail(
            file,
            line(m.index),
            "a hand-written shadow; use a --ring-*, --focus-ring-*, --well or " +
              "--elevation-* token so two things at one height stay at one height",
          );
        }

        for (const m of css.matchAll(/(^|[;{\s])([a-z-]+)\s*:\s*([^;}]*)/g)) {
          const property = m[2];

          for (const [where, size, token] of NAMED_SIZE) {
            if (!where.test(property)) continue;
            if (!new RegExp(`(?<![\\d.])${size}`).test(m[3])) continue;
            fail(file, line(m.index), `${property}: ${size} is ${token}`);
          }
        }

        /*
         * The decorative text step, colouring something that has to be read.
         *
         * `--text-4` is declared in `theme.css` as decorative only: placeholder
         * text and the Escape hint, never information. It is white at alpha
         * 0.26, which against the page tint over a pale desktop is nowhere near
         * the contrast small text needs, and the token's own comment says the
         * small-text threshold is not the bar it is measured against **because
         * nothing readable is supposed to be wearing it**.
         *
         * Twelve places were. A store row saying why it cannot be installed, an
         * extension command saying Sill has nowhere to run it, the keys the
         * store answers to, the amount of memory behind a percentage, today's
         * high and low, the key a tray item is bound to. Every one of them is
         * the fact its element exists to carry, drawn in the step reserved for
         * things nobody needs to read.
         *
         * The declaration is checked rather than the token, because the
         * declaration is where the choice was made and the selector above it
         * says what the choice was made about.
         *
         * A view rolling its own empty or loading state is caught in the same
         * pass, for the same reason: the selector is where it shows. Eleven
         * views wrote one, in three designs, and `ActionPanel` drew two of its
         * own at once for months because `actions.length === 0` is true in both
         * of the branches that tested it and neither branch was wrong.
         * `components/Instead.svelte` is the one recipe now.
         */
        for (const at of css.matchAll(/\{/g)) {
          const from = Math.max(
            css.lastIndexOf("}", at.index),
            css.lastIndexOf("{", at.index - 1),
          );
          const selector = css.slice(from + 1, at.index).trim();
          const body = css.slice(at.index, css.indexOf("}", at.index));

          // Not a selector: an at-rule's prelude, or a nested block's parent.
          if (selector.startsWith("@") || selector === "") continue;

          const named = /(^|[\s,>+~])\.(empty|loading)\b/.exec(selector);
          if (named) {
            fail(
              file,
              line(at.index),
              `a \`.${named[2]}\` rule of its own; empty, loading and failed ` +
                "states are `components/Instead.svelte`, so the three stay one " +
                "design and a view cannot draw two of them at once",
            );
          }

          if (!/color\s*:\s*var\(\s*--text-4\s*\)/.test(body)) continue;

          // A placeholder is what the token is for, and it is the one use that
          // says so in its own selector.
          if (/::(-\w+-input-)?placeholder/.test(selector)) continue;
          if ((DECORATIVE[file.replace(/\\/g, "/")] ?? []).includes(selector)) continue;

          fail(
            file,
            line(at.index),
            `\`${selector}\` is coloured --text-4, which theme.css reserves for ` +
              "decoration. If a reader has to read it, --text-3 is the quietest " +
              "step that is still meant to be read; if they do not, name it in " +
              "DECORATIVE in this check with the reason",
          );
        }
      }
    }

    /*
     * A popover with nothing behind it.
     *
     * `.sill-menu` is the surface: a deep ground, a blur and one hairline. A
     * popover without it is transparent, and what shows through is whatever it
     * was opened over, so the labels sit directly on the rows underneath and
     * wash out. The clipboard view had two popovers four lines apart, opened by
     * two buttons in the same bar; the kind one had the class and the
     * collections one did not, and nothing said so because a transparent menu
     * still lays out, still highlights and still works.
     *
     * `role="menu"` is what is checked because that is what a popover claims to
     * be. The exceptions are menus that are a whole window, which paint the
     * window's own surface instead and would be wearing two.
     */
    const OWN_WINDOW = /[\\/]traymenu[\\/]/;

    if (!OWN_WINDOW.test(file)) {
      for (const at of text.matchAll(/\srole="menu"/g)) {
        const opens = text.lastIndexOf("<", at.index);

        // The end of the opening tag, skipping any `>` inside an attribute
        // expression: `onclick={() => ...}` is full of them.
        let depth = 0;
        let closes = opens;
        for (; closes < text.length; closes += 1) {
          if (text[closes] === "{") depth += 1;
          else if (text[closes] === "}") depth -= 1;
          else if (text[closes] === ">" && depth === 0) break;
        }

        if (text.slice(opens, closes).includes("sill-menu")) continue;

        fail(
          file,
          lineOf(text, at.index),
          "a popover with no surface; add the sill-menu class, or it is " +
            "transparent and the rows underneath show through its labels",
        );
      }
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

    /*
     * What a screen reader is told, which is the half of the interface nobody
     * looks at.
     *
     * Every rule below is a mistake that renders perfectly. The launcher's
     * search field keeps focus while the arrow keys walk the list under it, so
     * the ONLY thing that says which row is highlighted is
     * `aria-activedescendant` naming that row's id. A row with no id is a row
     * that is never announced, and it looks exactly like a row that is.
     *
     * That was the state of four of the five lists in this window. It is the
     * kind of thing that comes back, because nothing about a missing id shows
     * up on screen, in the type checker, or in a test that renders markup and
     * looks at the text in it.
     */
    const view = markup(text);

    for (const tag of tags(view)) {
      const attrs = tag.attrs;
      const where = () => lineOf(text, tag.at);

      /*
       * One list on screen, one name for it.
       *
       * `LISTBOX` is the id the search field points `aria-controls` at. A
       * listbox that spells its own id is a listbox the field cannot name.
       */
      if (/\brole="listbox"/.test(attrs) && !/\bid=\{LISTBOX\}/.test(attrs)) {
        fail(
          file,
          where(),
          'a role="listbox" with no `id={LISTBOX}`, so the search field above ' +
            "it has nothing to point `aria-controls` at",
        );
      }

      /*
       * A row that can be named, and that says whether it is the named one.
       *
       * Both halves, because either alone is silence: an id nothing points at
       * announces nothing, and a highlighted row with no `aria-selected` is
       * announced as an ordinary one.
       */
      if (/\brole="option"/.test(attrs)) {
        if (!/\bid=\{optionId\(/.test(attrs)) {
          fail(
            file,
            where(),
            'a role="option" with no `id={optionId(...)}`, so ' +
              "`aria-activedescendant` can never name it and the highlight is " +
              "never announced",
          );
        }

        if (!/\baria-selected=/.test(attrs)) {
          fail(file, where(), 'a role="option" with no `aria-selected`');
        }
      }

      /*
       * The same for a menu, except that a `<button>` is already focusable and
       * announces itself when focus lands on it. A `<div>` is not and does not.
       */
      if (/\brole="menuitem(?:radio|checkbox)?"/.test(attrs) && tag.name !== "button") {
        if (!/\bid=\{itemId\(/.test(attrs)) {
          fail(
            file,
            where(),
            `a role="menuitem" on a <${tag.name}> with no \`id={itemId(...)}\`. ` +
              "It takes no focus of its own, so the only way it is ever " +
              "announced is the menu naming it",
          );
        }
      }

      /*
       * `role="application"` turns the screen reader off.
       *
       * It tells the reader to stop intercepting keys and hand every one to
       * the page: no browse mode, no arrow keys for reading, no headings. It
       * is a contract for something genuinely unreadable any other way, and it
       * was on the Ask window because a `div` there listens for a drag.
       */
      if (/\brole="application"/.test(attrs)) {
        fail(
          file,
          where(),
          'role="application" turns off the screen reader\'s own navigation ' +
            "for everything inside it",
        );
      }

      /*
       * A native tooltip.
       *
       * Drawn by Windows in the system font on a white slab, a second late,
       * unreachable from the keyboard, and announced differently by every
       * reader. `use:hint` from `$lib/hint` is the same sentence in the
       * window's own type, on focus as well as hover.
       *
       * Only lowercase names, which are HTML elements. `<Row title="...">`
       * and `<Section title="...">` are components taking a prop called
       * `title`, and that prop is a heading somebody reads.
       */
      if (/^[a-z][a-z0-9]*$/.test(tag.name) && /(^|\s)title=/.test(attrs)) {
        fail(
          file,
          where(),
          `a native title="" on <${tag.name}>; use \`use:hint\` from $lib/hint`,
        );
      }
    }

    /*
     * Menu item ids that nothing names.
     *
     * `itemId` exists for one purpose, and a menu that gives its items ids
     * without pointing `aria-activedescendant` at one of them has done the
     * half of the work that has no effect.
     */
    if (view.includes("id={itemId(") && !view.includes("aria-activedescendant=")) {
      fail(
        file,
        null,
        "menu items here carry ids and nothing points `aria-activedescendant` " +
          "at any of them, so the ids are decoration",
      );
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
 * Every allowance the diagnostic bundle quotes is still in the budget table.
 *
 * `docs/budgets.md` is the contract: it says what Sill is allowed to cost and
 * which test holds it. The export bundle prints a couple of those figures
 * beside what this run measured, so somebody reading a bundle can see at a
 * glance whether the machine that produced it is inside them.
 *
 * That is one number written in two places with nothing making them agree,
 * which is the shape this file exists for. A budget loosened in the document
 * and not in `bundle.rs` means every bundle quotes a figure that stopped being
 * true, and it is exactly the kind of drift nobody notices, because both
 * halves keep working.
 */
{
  const BUNDLE = "src-tauri/src/bundle.rs";
  const BUDGETS = "docs/budgets.md";

  const table = readFileSync(BUDGETS, "utf8");
  const source = readFileSync(BUNDLE, "utf8");

  /*
   * Matched against the row rather than against the whole document.
   *
   * Looking the figure up anywhere in the file is not a check: the document
   * lists two dozen numbers, and "41 MB" is in it as what the Rust core costs
   * with a whole drive indexed. Quoting that as the idle allowance would have
   * passed. What has to hold is that the row naming this budget carries this
   * figure, so both halves of the pair are matched on one line.
   */
  const quoted = [
    ...source.matchAll(/what:\s*"([^"]+)",\s*allowed:\s*Some\("([^"]+)"\)/g),
  ];

  if (quoted.length === 0) {
    fail(BUNDLE, null, "no budget is quoted, so this check verifies nothing");
  }

  const rows = table.split("\n");

  for (const [, what, allowed] of quoted) {
    const said = rows.some((row) => row.includes(what) && row.includes(allowed));

    if (!said) {
      const at = source.indexOf(`what: "${what}"`);
      fail(
        BUNDLE,
        lineOf(source, at),
        `the bundle quotes "${allowed}" for "${what}" and no row of ${BUDGETS} ` +
          "says both, so every exported bundle reports a figure that is not " +
          "the one anything is held to",
      );
    }
  }
}

/*
 * Every cost the public page names is still a row of the budget table.
 *
 * `docs/benchmark.md` is the most public thing in this repository and the
 * efficiency claim is the pitch, so it has to be checkable. Its readings come
 * from `docs/measurements/`, which only a measuring script ever writes, and its
 * budgets come from `scripts/benchmarks.json`, which is where the decisions
 * are. That second half is the pair with nothing making it agree: a budget
 * loosened in `docs/budgets.md` and not in the catalogue leaves the public page
 * holding Sill to a figure nothing enforces, and the version where the page is
 * the loose one is worse.
 *
 * Matched on one line, for the reason the check above it gives: the budget
 * document lists two dozen numbers and finding one anywhere in the file is not
 * a check. The row naming this cost has to carry this budget.
 */
{
  const CATALOGUE = "scripts/benchmarks.json";
  const BUDGETS = "docs/budgets.md";

  const rows = JSON.parse(readFileSync(CATALOGUE, "utf8")).rows;
  const table = readFileSync(BUDGETS, "utf8").split("\n");

  if (rows.length === 0) {
    fail(CATALOGUE, null, "no cost is published, so the page checks nothing");
  }

  for (const row of rows) {
    const said = table.some(
      (line) => line.includes(row.what) && (!row.budget || line.includes(row.budget)),
    );

    if (!said) {
      fail(
        CATALOGUE,
        null,
        `"${row.what}"${row.budget ? ` at "${row.budget}"` : ""} is published ` +
          `and no row of ${BUDGETS} says that, so the public page and the ` +
          "budget it is held to have stopped being the same claim",
      );
    }

    /*
     * A row either names what measures it or says why nothing does.
     *
     * Both are real states and the page prints both. The clipboard's
     * write-ahead budget is held by a test file that is not in the tree any
     * more, and extension activation has an instrument whose readings are too
     * small to be what the budget is about. What is refused is a row that says
     * neither, because the page would then have a cost on it that nobody can
     * chase, and a row naming a measurer that is not there, because the page
     * would tell a reader to run something that cannot be run.
     */
    if (!row.recordedBy && !row.noReading) {
      fail(
        CATALOGUE,
        null,
        `"${row.what}" names neither what measures it nor why nothing does, ` +
          "so the published page carries a cost with no way to chase it",
      );
    }

    if (row.recordedBy && !existsSync(row.recordedBy)) {
      fail(
        CATALOGUE,
        null,
        `"${row.what}" names ${row.recordedBy} as what would measure it and ` +
          "that file does not exist, so the page sends a reader to a script " +
          "nobody has",
      );
    }
  }
}

/*
 * The Raycast API level, in the two files that both claim it.
 *
 * The worker tells every extension `environment.raycastVersion`, and the
 * installer refuses nothing but warns when a manifest asks for something
 * newer than that. Those are the same claim written twice, in a Rust constant
 * and a TypeScript literal, with nothing making them agree.
 *
 * Drift here is silent and one-directional. Raising the number in the worker
 * and not in Rust leaves the install screen warning about versions Sill now
 * implements; raising it in Rust and not in the worker tells every extension
 * it is running somewhere older than it is, and extensions branch on that.
 */
{
  const WORKER = "host/src/worker/worker.ts";
  const INSTALL = "src-tauri/src/extension_install.rs";

  const said = readFileSync(WORKER, "utf8").match(/raycastVersion:\s*"([\d.]+)"/);
  const declared = readFileSync(INSTALL, "utf8").match(
    /RAYCAST_API_LEVEL: &str = "([\d.]+)"/,
  );

  if (!said) fail(WORKER, null, "no `raycastVersion`, which RAYCAST_API_LEVEL mirrors");
  else if (!declared) {
    fail(INSTALL, null, "no `RAYCAST_API_LEVEL`, which mirrors the worker's raycastVersion");
  } else if (said[1] !== declared[1]) {
    fail(
      INSTALL,
      null,
      `RAYCAST_API_LEVEL is ${declared[1]} and the worker tells extensions ` +
        `${said[1]}. One of them is lying to somebody about what Sill implements`,
    );
  }
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
    // `LEVEL` is here for the same reason as the other three: `say!` and
    // `detail!` expand at call sites that have no handle to anything, and
    // `detail!` reads it before formatting, which is what keeps it free on a
    // path that runs per keystroke.
    "log.rs": ["FILE", "LEVEL", "PATH", "WRITTEN"],

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

    // A window procedure is the same constraint as a hook callback: Windows
    // calls a bare `extern "system" fn` and the only context it offers is the
    // `usize` handed to `SetWindowSubclass`, which cannot hold an `AppHandle`.
    "session.rs": ["APP"],

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

  /*
   * The window builds rows too, and this check could not see them.
   *
   * It read Rust only, on the assumption that a row's mode is Rust's to
   * decide. Six row builders in `commands.ts` and `results.ts` set one
   * themselves, and every one of those modes happened also to appear in Rust,
   * so the hole never showed. A row whose mode exists **only** in the window
   * went straight past: browser tabs are read by Rust and shaped into a row
   * here, and this file passed with the heading deleted.
   *
   * The preview gallery is not in the list. Its `mode` fields are extension
   * manifest data on fake listings, not row kinds, and `menu-bar` is a real
   * value there and not a heading anything is missing.
   */
  const BUILDERS = [
    "src/lib/exthost/commands.ts",
    "src/lib/results.ts",
    "src/lib/conversations.ts",
    "src/routes/+page.svelte",
  ];

  const looked = [
    ...sources("src-tauri/src"),
    ...BUILDERS.filter((one) => existsSync(one)),
  ];

  for (const file of looked) {
    const text = readFileSync(file, "utf8");

    for (const found of text.matchAll(/\bmode:\s*"([a-z][a-z-]*)"/g)) {
      if (named.has(found[1]) || byExtension.has(found[1])) continue;

      fail(
        file,
        lineOf(text, found.index),
        `mode "${found[1]}" has no heading in ${LIST}, so its rows are filed ` +
          "under whichever extension produced them rather than under a name",
      );
    }
  }
}

/*
 * Private mode is not a check anybody has to remember.
 *
 * Two things it stops are enforced by the compiler already: a screen capture
 * takes a `privacy::Allowed`, which only `privacy::allow` can make, so a new
 * way of photographing the screen does not build until it has asked. The third
 * is not, and cannot be: the clipboard watcher's `Rules` are a plain struct,
 * and anybody filling one in by hand would produce a watcher that records
 * while private mode is on. That is exactly how it was before, in two places
 * that each spelled the same seven fields out.
 *
 * So both halves are checked here rather than trusted.
 *
 * - `monitor::Rules { .. }` is built in `privacy.rs` and nowhere else, which
 *   is what makes the startup path and the settings path give the same answer.
 * - `privacy::allowed_regardless()` is the one way past the capture gate, and
 *   it belongs only where there is nobody whose screen it could be: a test, or
 *   an `#[ignore]` probe somebody runs by hand.
 */
{
  const OWNS_RULES = "src-tauri/src/privacy.rs";
  const DECLARES_RULES = "src-tauri/src/clipboard/monitor.rs";

  for (const file of sources("src-tauri")) {
    if (extname(file) !== ".rs") continue;

    const text = readFileSync(file, "utf8");
    const normal = file.split(sep).join("/");

    if (normal !== OWNS_RULES && normal !== DECLARES_RULES) {
      for (const found of text.matchAll(/\bmonitor::Rules\s*\{/g)) {
        fail(
          file,
          lineOf(text, found.index),
          "fills in the clipboard watcher's rules by hand, so private mode " +
            `is whatever this line says it is. Call \`privacy::clipboard_rules\` ` +
            `(${OWNS_RULES}) instead`,
        );
      }
    }

    if (!text.includes("allowed_regardless")) continue;

    // Where the test code starts, if there is any. A `#[cfg(test)]` module is
    // the last thing in a Rust file by convention here, and a file under
    // `tests/` is all test code.
    const tests = normal.includes("src-tauri/tests/")
      ? 0
      : (text.indexOf("#[cfg(test)]") + 1 || text.length + 1) - 1;

    for (const found of text.matchAll(/\ballowed_regardless\s*\(/g)) {
      if (normal === OWNS_RULES) continue;
      if (found.index >= tests) continue;

      fail(
        file,
        lineOf(text, found.index),
        "takes a screen capture past private mode. `allowed_regardless` is " +
          "for tests and hand-run probes; a capture somebody could be looking " +
          "at asks `privacy::allow` instead",
      );
    }
  }
}

/*
 * Every action the window names by hand is an action that exists.
 *
 * Thirteen places in the window reach an action by its id rather than through
 * the panel: Enter on a window, on a process, on a program's volume, on a
 * calculator answer, on what is playing. Each of those is a string written in
 * TypeScript that has to match a string written in Rust, with nothing making
 * them match. Rename one in `actions/mod.rs` and the window still compiles,
 * still passes `svelte-check`, and answers "no such action" the first time
 * somebody presses the key.
 *
 * Two lists that must agree with nothing making them agree is a shape this
 * project has paid for before. This is the thing that makes them agree.
 */
{
  const PAGE = "src/routes/+page.svelte";
  const ACTIONS = "src-tauri/src/actions/mod.rs";
  const text = readFileSync(PAGE, "utf8");
  const rust = readFileSync(ACTIONS, "utf8");

  // Every id an action declares. They are literals in `fn id`, so the whole
  // file is scanned for the shape rather than the function parsed.
  const declared = new Set(
    Array.from(rust.matchAll(/"(sill\.[A-Za-z0-9.]+)"/g), (m) => m[1]),
  );

  for (const found of text.matchAll(/runObjectAction\("(sill\.[A-Za-z0-9.]+)"/g)) {
    if (declared.has(found[1])) continue;

    fail(
      PAGE,
      lineOf(text, found.index),
      `the window runs "${found[1]}" and no action in ${ACTIONS} declares ` +
        "that id, so pressing the key it is behind answers \"no such action\"",
    );
  }
}

/*
 * Somebody's own writing is read entry by entry, never all or nothing.
 *
 * `json_store::load` refuses the whole document when one field cannot be read
 * and moves the file aside; `load_list` keeps every entry it can. For seven of
 * the eight stores either would be survivable, because a snippet or a
 * quicklink can be typed again. A paragraph somebody wrote cannot.
 *
 * The reason this is a rule and not a test: `notes.rs` extracts its reading
 * into a `read` function so a test can call it without a running application,
 * and a test that calls `read` would go on passing if `read` were changed to
 * the whole-document version and the store still called it. That is the same
 * hole three tests of clipboard pruning had, where taking the call out of the
 * recording path failed nothing. So the file is held to the choice directly.
 */
{
  const STORE = "src-tauri/src/notes.rs";
  const text = readFileSync(STORE, "utf8");

  if (!text.includes("json_store::load_list(")) {
    fail(STORE, null, "nothing here reads the notes file, so this rule is asleep");
  }

  for (const m of text.matchAll(/json_store::load(?:_with)?\(/g)) {
    fail(
      STORE,
      lineOf(text, m.index),
      "this reads the whole notes file at once, so one entry serde cannot read " +
        "costs every note in it. Use `json_store::load_list`",
    );
  }
}

/*
 * A row Rust builds out of the query has something bound to Enter.
 *
 * These are the rows that are not in the index: a calculator answer, what is
 * playing, a terminal profile, a note, a reminder. `launch_command` looks a
 * row's id up in the index, so pressing Enter on one of them answers "no such
 * command" unless the window intercepts the mode first. That is not
 * hypothetical: it is what happened to every sum anybody pressed Enter on
 * before the answer row got its branch, and the audit records that deleting
 * the media branch is caught by nothing.
 *
 * So the modes are read out of `registry.rs`, which is where the rows are
 * built, and each one has to be named somewhere in the launcher. Naming is a
 * low bar on purpose: a branch, a list, a `case`, whatever the window
 * eventually uses. What it catches is a row arriving with no Enter at all.
 */
{
  const RECORDS = "src-tauri/src/registry.rs";
  const PAGE = "src/routes/+page.svelte";
  const rust = readFileSync(RECORDS, "utf8");
  const page = readFileSync(PAGE, "utf8");

  /*
   * Every `mode` a `RankedCommand`-building function sets.
   *
   * Scoped to the functions that return a `RankedCommand`, because those are
   * exactly the rows spliced into a search rather than found in it. A
   * `CommandRecord` on its own is an index entry, and an index entry is
   * something `launch_command` can look up.
   */
  const built = new Map();

  for (const found of rust.matchAll(
    /pub fn (\w+)\([^)]*\)\s*->\s*RankedCommand \{([\s\S]*?)\n\}/g,
  )) {
    const mode = found[2].match(/\bmode: "([^"]*)"\.to_string\(\)/);
    if (mode) built.set(mode[1], { fn: found[1], line: lineOf(rust, found.index) });
  }

  if (built.size < 4) {
    fail(
      RECORDS,
      null,
      `only ${built.size} row builders were found, so this rule is not ` +
        "checking anything. The shape it scans for has changed",
    );
  }

  for (const [mode, where] of built) {
    if (page.includes(`"${mode}"`)) continue;

    fail(
      PAGE,
      null,
      `${RECORDS}:${where.line} builds a row with mode ${JSON.stringify(mode)} ` +
        `in ${where.fn}, and nothing in the launcher names it. That row is not ` +
        "in the index, so pressing Enter on it answers \"no such command\"",
    );
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
 * A registry action reaches the panel with the key it declares.
 *
 * The window used to write the chords beside the clipboard's four rows by
 * hand and pass `shortcut: undefined` for everything else, so an action that
 * arrived through any other list had no key at all and the panel drew nothing
 * beside it. `Action::shortcut` in Rust is the answer now, and the only way
 * to lose it again is to map an `ActionInfo` into a panel entry and drop the
 * field on the way, which type-checks: `shortcut` is optional.
 *
 * So every place the window turns a registry action into a panel row has to
 * pass the action's own shortcut along.
 *
 * Both places are in `$lib/panel` now rather than in the window's `$derived`.
 * The files are listed rather than the one that happens to hold them today,
 * because a check that names a single file goes quiet the moment the code
 * moves, which is the failure mode this whole script exists to avoid.
 */
{
  const BUILDERS = ["src/lib/panel.ts", "src/routes/+page.svelte"];
  let anywhere = 0;

  for (const file of BUILDERS) {
    const text = readFileSync(file, "utf8");

    // Each entry built from a registry action, found by the tag it is given.
    const built = Array.from(text.matchAll(/tag: `Sill\.Action:\$\{action\.id\}`/g));
    anywhere += built.length;

    for (const at of built) {
      // The entry is a short object literal. A whole file's worth of slack
      // would let one entry borrow the shortcut another passed.
      const nearby = text.slice(at.index, at.index + 900);

      if (!nearby.includes("action.shortcut")) {
        fail(
          file,
          lineOf(text, at.index),
          "a registry action is drawn without the shortcut Rust resolved for it, " +
            "so the key somebody set in Settings is neither drawn nor read",
        );
      }
    }
  }

  if (anywhere === 0) {
    fail(BUILDERS[0], 1, "no registry action reaches the action panel at all");
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
 * Nothing Sill reports as broken can stay broken after it is fixed.
 *
 * A trouble is held until something withdraws it, so a `report` with no
 * matching `resolved` is permanent: the tray keeps saying the startup entry
 * could not be written long after it was, and a surface that keeps naming a
 * fixed problem is one people stop reading. `HotkeyConflicts` grew a test for
 * exactly this, because a key is very often taken by an application that is
 * still closing and works on the retry a second later.
 *
 * Every id is a plain name or a string literal for this reason. The one that
 * is built up, the group the settings window owns, is built inside `status.rs`
 * behind a function that owns its withdrawal too, which is why that file is
 * not read here.
 */
{
  const withdrawn = new Set();
  const reported = [];

  for (const file of sources("src-tauri/src")) {
    // `status.rs` builds the one id that is not a plain name, and owns the
    // call that withdraws it, so reading it here would only find itself.
    if (extname(file) !== ".rs" || file.endsWith("status.rs")) continue;

    const text = readFileSync(file, "utf8");

    for (const at of text.matchAll(/status::resolved\(\s*&?[\w.]+\s*,\s*([^,)\r\n]+)/g)) {
      withdrawn.add(at[1].trim());
    }

    for (const at of text.matchAll(/status::report\(\s*&?[\w.]+\s*,\s*([^,\r\n]+),/g)) {
      const id = at[1].trim();
      const line = lineOf(text, at.index);

      if (!/^([A-Z_][A-Z0-9_]*|"[^"]*")$/.test(id)) {
        fail(
          file,
          line,
          `\`status::report\` is given \`${id}\`, which is neither a named constant nor a ` +
            "literal, so nothing can check that the trouble is ever withdrawn",
        );
        continue;
      }

      reported.push({ file, id, line });
    }
  }

  for (const { file, id, line } of reported) {
    if (withdrawn.has(id)) continue;

    fail(
      file,
      line,
      `\`${id}\` is reported as a trouble and never withdrawn by \`status::resolved\`, ` +
        "so it would keep saying a fixed thing is broken",
    );
  }
}

/*
 * A wrapper that swallows a refused command decides for every one of its
 * callers.
 *
 * `.catch(() => [])` turns a refusal into an empty list and the pane then
 * draws that list as if it were the answer: no search engines, no collections,
 * no drives on this machine. Tauri denies a command to a window missing from
 * `capabilities/default.json` **silently**, which is how the tray menu once
 * shipped completely dead, so an empty pane is far more likely to be a
 * permission than a fact.
 *
 * A catch taking no argument cannot report anything, because it threw the
 * reason away. So this refuses that shape wherever it is chained straight onto
 * an `invoke`, which is where the wrapper modules live and where one decision
 * is made on behalf of every caller in the application. Both answers are
 * available and both are named: `orElse` keeps the fallback and reports it,
 * `silently` keeps the fallback and says why it is enough. Whichever is right,
 * the code says which was chosen instead of defaulting to the one that is
 * easiest to type.
 *
 * It deliberately does not reach a `.catch` a page puts on a call of its own.
 * There the fallback, the failure and the reasoning are all on one screen for
 * whoever reads it next, which is the thing a wrapper hides.
 */
for (const file of sources("src")) {
  if (![".ts", ".svelte"].includes(extname(file)) || file.endsWith(".test.ts")) continue;

  const text = readFileSync(file, "utf8");

  // The call and its catch, across the line breaks a formatter puts in. The
  // body is bounded so an `invoke` far above an unrelated catch cannot pair
  // with it.
  for (const at of text.matchAll(/\binvoke[<(][^;]{0,400}?\.catch\(\(\)\s*=>/g)) {
    fail(
      file,
      lineOf(text, at.index),
      "a catch that takes no argument throws the reason away, so a command this " +
        "window was refused is drawn as an empty answer. Report it with `orElse`, " +
        "or say why the fallback is enough with `silently`",
    );
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
/*
 * A message with a run of spaces inside it.
 *
 * Rust joins a literal written across two lines only when the first line ends
 * in a backslash. Lose that backslash, or let a tool reflow the pair onto one
 * line, and the indentation that was holding the second half in place becomes
 * part of the sentence. It compiles, every test that checks for a substring
 * still passes, and the person reading it sees
 * "so it will                      ask again".
 *
 * Found twice in one merge, in a status report and in a log line, so this is a
 * real shape rather than a hypothetical one.
 *
 * Two things are deliberately not this bug. A literal carrying an escape is
 * formatting on purpose (`"  {} {}\\n    id: {}"` lines a listing up), and a
 * test fixture is allowed to contain whatever spacing it is testing the
 * handling of. Both are skipped, which leaves prose meant for a person.
 */
for (const file of sources("src-tauri/src")) {
  const text = readFileSync(file, "utf8");

  text.split("\n").forEach((line, at) => {
    // Comments are prose, and prose may be spaced however it likes.
    if (/^\s*(\/\/|\*|\/\*)/.test(line)) return;

    // A fixture says what spacing it expects; that is the point of it.
    if (line.includes("assert")) return;

    for (const m of line.matchAll(/"((?:[^"\u005C]|\u005C.)*)"/g)) {
      // An escape means the spacing was arranged on purpose.
      if (m[1].includes("\u005C")) continue;

      // Spaces around a colon line a reading up in a column, which is how
      // every `key    : value` and `key:    value` diagnostic in here is
      // written. Both sides, because the run sits before the colon when the
      // keys are padded and after it when the values are.
      const aligned = /\S {3,}[:|]|[:|] {3,}\S/;

      // Three, not two: two spaces after a full stop is a writing style,
      // while three is indentation that leaked in.
      if (/\S {3,}\S/.test(m[1]) && !aligned.test(m[1])) {
        fail(
          file,
          at + 1,
          "a message has a run of spaces in it, which is usually a line " +
            "continuation that lost its backslash",
        );
      }
    }
  });
}

/*
 * Everything that cannot be taken back has exactly one caller.
 *
 * The launcher asks before it shuts the machine down and before it empties the
 * recycle bin, and the asking is one function: `actions::once_answered` runs
 * `Irreversible::apply` from inside the arm that already holds the answer, and
 * from nowhere else. That is what makes "no single press ever means yes" a
 * fact about the shape of the code rather than a claim about how carefully
 * each call site was written.
 *
 * `Power::apply` needs no rule here, because it is private to `system.rs` and
 * the compiler already refuses a second caller. These two are reachable across
 * modules and so can gain one silently, and the way that failure looks is a
 * row that switches the machine off on the first press.
 *
 * A definition is not a call, so `fn apply` and `pub fn empty` are not
 * counted, and neither is a mention inside a comment.
 */
{
  const ONCE = [
    ["Irreversible::apply", /Irreversible::apply\s*\(|\babout\.apply\s*\(/g],
    ["recycle_bin::empty", /recycle_bin::empty\s*\(/g],
  ];

  for (const [what, pattern] of ONCE) {
    const found = [];

    for (const file of sources("src-tauri/src")) {
      if (extname(file) !== ".rs") continue;

      const text = readFileSync(file, "utf8");

      for (const at of text.matchAll(pattern)) {
        // A doc comment naming the rule is how the rule is explained. Only
        // code counts.
        const line = text.slice(0, at.index).split("\n").pop();
        if (/^\s*(\/\/|\*|\/\*)/.test(line)) continue;

        found.push(`${file}:${lineOf(text, at.index)}`);
      }
    }

    if (found.length !== 1) {
      fail(
        "src-tauri/src/actions/mod.rs",
        null,
        `${what} has ${found.length} call sites (${found.join(", ") || "none"}), ` +
          "and it must have exactly one, inside the arm of `once_answered` that " +
          "has already been answered. A second route to it is a way round the " +
          "question",
      );
    }
  }
}

/*
 * Nothing reaches the shell without going past `reach` first.
 *
 * `tauri_plugin_opener` hands a string to the operating system, and the
 * operating system decides what a string means. `javascript:` runs in whatever
 * browser is default, `file:` reads the disk, and Windows keeps discovering
 * that one more of its own protocol handlers executes something. None of that
 * is a problem while the string is Sill's; all of it is a problem the moment
 * the string arrives in an imported quicklink, in an extension's action
 * payload, or from a model that has just read a web page telling it what to
 * open.
 *
 * There were six call sites and not one of them checked. The failure this
 * catches is the seventh: a new one, written by somebody with no reason to
 * know that opening is guarded, which compiles and works perfectly on every
 * address anybody tries by hand.
 *
 * A literal `https://` passes without a guard, because a target the source
 * spells out cannot be somebody else's text.
 */
{
  const OPENS = /tauri_plugin_opener::open_(?:url|path)\(/g;

  for (const file of sources("src-tauri/src")) {
    const text = readFileSync(file, "utf8");

    for (const found of text.matchAll(OPENS)) {
      const before = text
        .slice(0, found.index)
        .split("\n")
        .slice(-15)
        .join("\n");
      const argument = text.slice(found.index, found.index + 200);

      if (before.includes("crate::reach::")) continue;
      if (/\(\s*(?:format!\()?"https:\/\//.test(argument)) continue;

      fail(
        file,
        lineOf(text, found.index),
        "this opens something with no `crate::reach::url` or `crate::reach::target` " +
          "above it, so a javascript:, data: or file: address reaches the shell",
      );
    }
  }
}

/*
 * Nothing a stranger asked for reaches an action without going past the gate.
 *
 * `outside.rs` is the one door a `sill://` address and a `sill run` command
 * come through, and `reach::may_run` is what decides which of the two is
 * asking and what that one is allowed to name. A page on the internet can
 * write an address; the registry behind that door moves files to the recycle
 * bin, runs scripts, quits processes and shuts the machine down.
 *
 * Two things are checked and neither is about how the gate is written. There
 * must be exactly ONE `perform` in the file, because a second is a second path
 * and the whole design is that there is not one, and `reach::may_run` must
 * appear before it, because a gate consulted afterwards is a gate.
 *
 * Written as a rule rather than left to the tests because the tests cannot see
 * it: an `ActionCtx` holds a concrete `AppHandle`, so nothing in a test can run
 * an action body, and the sequence of calls in this function is therefore
 * checkable only by reading it. This reads it.
 */
{
  const DOOR = "src-tauri/src/outside.rs";
  const text = readFileSync(DOOR, "utf8");

  const performs = [...text.matchAll(/\.perform\(/g)];
  const gate = text.indexOf("reach::may_run(");

  if (performs.length !== 1) {
    fail(
      DOOR,
      performs[1] ? lineOf(text, performs[1].index) : null,
      `this file runs an action in ${performs.length} places and must run it in ` +
        "one. A second route from a command line to the registry is a second " +
        "place to remember the gate",
    );
  }

  for (const found of performs) {
    if (gate !== -1 && gate < found.index) continue;

    fail(
      DOOR,
      lineOf(text, found.index),
      "this runs an action with no `reach::may_run` above it, so a `sill://` " +
        "address off a web page reaches whatever it named",
    );
  }
}

/*
 * Nothing a model asks for reaches an action without going past the gate.
 *
 * `ai/tools.rs` is the one door the AI panel and every MCP client come
 * through, and `acting::gate` is what decides whether a keypress is enough or
 * whether Windows Hello has to say a person is there. The threat is not a
 * mischievous model: it is a web page, a document or an extension's output
 * telling the model to run something, and the gate is where the person rather
 * than the text authorises it.
 *
 * Three things are checked. There must be exactly ONE `perform` in the file,
 * because a second is a second path and the whole design is that there is not
 * one. `acting::gate` must appear before it, because a gate consulted
 * afterwards is not a gate. And the `Gate::Hello` arm and the call it makes
 * must both sit between the two, because an arm that decides Hello is needed
 * and then falls through to the run is the gate being deleted while its
 * decision function still passes all of its own tests.
 *
 * Written as a rule rather than left to the tests for the reason the
 * `outside.rs` check above is: an `ActionCtx` holds a concrete `AppHandle`, so
 * nothing in a test can run an action body, and the sequence of calls in this
 * function is checkable only by reading it. This reads it.
 */
{
  const DOOR = "src-tauri/src/ai/tools.rs";
  const text = readFileSync(DOOR, "utf8");

  const performs = [...text.matchAll(/\.perform\(/g)];
  const gate = text.indexOf("acting::gate(");

  if (performs.length !== 1) {
    fail(
      DOOR,
      performs[1] ? lineOf(text, performs[1].index) : null,
      `this file runs an action in ${performs.length} places and must run it in ` +
        "one. A second route from a model to the registry is a second place to " +
        "remember the gate",
    );
  }

  for (const found of performs) {
    if (gate !== -1 && gate < found.index) continue;

    fail(
      DOOR,
      lineOf(text, found.index),
      "this runs an action with no `acting::gate` above it, so a model that " +
        "read an instruction in somebody else's document reaches whatever it named",
    );
  }

  if (gate !== -1 && performs[0]) {
    const between = text.slice(gate, performs[0].index);

    for (const wanted of ["Gate::Hello", "prove_somebody_is_there("]) {
      if (between.includes(wanted)) continue;

      fail(
        DOOR,
        lineOf(text, gate),
        `no \`${wanted}\` between the gate and the run, so running a command ` +
          "or writing a file never asks Windows Hello and nothing else notices",
      );
    }
  }
}

/*
 * The automation module runs nothing at rest, and gates what it writes down.
 *
 * `P8-02`'s whole claim is that Sill contributes no scheduler: a trigger is a
 * scheduled task, Windows holds the loop, and the only Sill code involved runs
 * when somebody opens the settings panel. That claim is one thread away from
 * being false, and nothing about adding a thread would fail a test, because a
 * background loop that works is a background loop that passes.
 *
 * The gate is the other half. `may_schedule` is what stops a trigger naming an
 * action that would stop and ask, which matters precisely because nobody is at
 * the machine when one fires. Consulted after the task is written it is not a
 * gate, and a task written without it never asks anybody anything.
 *
 * The commands are held to being called. A registered command is not evidence
 * that anything invokes it: a dead one found on this project held the only
 * extension timing there was, and read exactly like a working feature from the
 * Rust side. Scoped to the file this item added, which is the only part it can
 * honestly claim; ten other commands in the handler are named nowhere in the
 * frontend and are somebody else's item.
 */
{
  const CORE = "src-tauri/src/automation.rs";
  const DOOR = "src-tauri/src/commands/automation.rs";
  const FRONT = "src/lib/automations.ts";

  const core = readFileSync(CORE, "utf8");
  const door = readFileSync(DOOR, "utf8");
  const front = readFileSync(FRONT, "utf8");

  /*
   * Nothing periodic, and nothing resident.
   *
   * A generous list on purpose. The point is not to catch a clever evasion,
   * it is to make somebody reaching for one of these stop and reread the
   * paragraph at the top of that file about why there is no loop.
   *
   * `timers.rs` is held to the same line, and it is the file where somebody
   * would most reasonably reach for one: `P3-11` said timers must tick, and
   * the whole answer here is that the ticking is Windows' and not Sill's. A
   * countdown written here would work perfectly and would quietly undo the
   * only claim the feature makes.
   *
   * `notes.rs` too, for a different reason. Nothing about notes wants a
   * timer, which is exactly why one would arrive later as a convenience: an
   * autosave sweep, or a watcher on the file. Both are a thread woken on an
   * idle machine for a prototype that is switched off.
   */
  const QUIET = [CORE, "src-tauri/src/timers.rs", "src-tauri/src/notes.rs"];

  for (const file of QUIET) {
    const text = readFileSync(file, "utf8");

    for (const wakes of [
      "thread::spawn",
      "spawn_blocking",
      "tokio::time",
      "interval(",
      "Instant::now",
    ]) {
      if (!text.includes(wakes)) continue;

      fail(
        file,
        lineOf(text, text.indexOf(wakes)),
        "`" +
          wakes +
          "` here, and the point of this file is that Windows owns the " +
          "schedule and Sill owns no loop. Anything periodic belongs in the " +
          "task, not in the process",
      );
    }
  }

  // A second way to run an action, which is the one thing an automation must
  // never be. It reaches the registry through the command line `outside.rs`
  // already reads, or it does not reach it.
  for (const m of core.matchAll(/\.perform\(/g)) {
    fail(
      CORE,
      lineOf(core, m.index),
      "this runs an action, and an automation must reach the registry the way " +
        "everything else does: as a command line through `outside.rs`",
    );
  }

  const writes = [...door.matchAll(/automation::register\(/g)];

  /*
   * The gate has to be in the SAME command, not merely earlier in the file.
   *
   * Written as a file-wide `indexOf` first, and sabotage caught it: deleting
   * the check from the command that writes a task passed, because the command
   * that lists them consults `may_schedule` too and sits above it. So the
   * search starts at the enclosing `#[tauri::command]` rather than at the top.
   */
  for (const found of writes) {
    const boundary = door.lastIndexOf("#[tauri::command]", found.index);
    const inside = door.slice(boundary === -1 ? 0 : boundary, found.index);

    if (inside.includes("may_schedule(")) continue;

    fail(
      DOOR,
      lineOf(door, found.index),
      "this writes a scheduled task with no `may_schedule` in the same " +
        "command, so a trigger can name an action that stops to ask at a " +
        "moment nobody is there to answer",
    );
  }

  if (!writes.length) {
    fail(DOOR, null, "nothing here writes a scheduled task, so this rule is asleep");
  }

  const declared = /#\[tauri::command\]\s*\n\s*pub\(crate\)\s+(?:async\s+)?fn\s+(\w+)/g;

  for (const m of door.matchAll(declared)) {
    if (front.includes('"' + m[1] + '"')) continue;

    fail(
      DOOR,
      lineOf(door, m.index),
      "`" +
        m[1] +
        "` is registered and " +
        FRONT +
        " never invokes it, so nothing proves it is reachable and nothing " +
        "would fail if it stopped working",
    );
  }
}

/*
 * Pressing a control reaches nothing of Sill's, and costs nothing at rest.
 *
 * `P8-04`'s pressing half has two claims a test cannot make on this machine,
 * and both fail silently.
 *
 * The first is that Sill never presses its own buttons. The launcher's window
 * is on screen while somebody is choosing a row, so without a refusal the view
 * would offer Sill's own controls, and anything holding `ControlInvoke` would
 * be able to press them. `controls::is_ours` is the rule and it has a unit
 * test; what no unit test can hold is that both ways in still consult it,
 * because doing so needs a real window of this process and the library's test
 * binary has none. Deleting the call fails nothing at all.
 *
 * The second is the cost claim. Nothing in this module runs until somebody
 * opens the view, which is what makes its idle cost zero rather than nearly,
 * and it is one thread away from being false with nothing failing: a
 * background walk that works is a background walk that passes.
 *
 * The list is generous on purpose. It is not there to catch a clever evasion,
 * it is there to make somebody reaching for one of these reread the paragraph
 * at the top of that file about why there is nothing resident.
 */
{
  const CORE = "src-tauri/src/controls.rs";
  const DOOR = "src-tauri/src/commands/search.rs";

  const core = readFileSync(CORE, "utf8");
  const door = readFileSync(DOOR, "utf8");

  for (const wakes of [
    "thread::spawn",
    "tokio::time",
    "interval(",
    "OnceLock",
    "OnceCell",
    "Lazy<",
    "static mut",
  ]) {
    if (!core.includes(wakes)) continue;

    fail(
      CORE,
      lineOf(core, core.indexOf(wakes)),
      "`" +
        wakes +
        "` here, and the point of this module is that nothing exists between " +
        "two questions and nothing at all runs until somebody opens the view",
    );
  }

  /*
   * Both ways into another program's window refuse Sill's own.
   *
   * Scoped to the enclosing `pub fn` rather than searched file-wide, which is
   * the shape `may_schedule` had to be given after sabotage: a file-wide
   * `includes` passes as long as ONE function still checks, and the one that
   * stopped checking is the one that matters.
   */
  const doors = [...core.matchAll(/\n    pub fn (read|press)\(/g)];

  for (const found of doors) {
    const next = core.indexOf("\n    /", found.index + 1);
    const body = core.slice(found.index, next === -1 ? core.length : next);

    if (body.includes("refuse_our_own(")) continue;

    fail(
      CORE,
      lineOf(core, found.index),
      "`" +
        found[1] +
        "` reaches into a window without `refuse_our_own`, so the launcher " +
        "can read and press its own buttons",
    );
  }

  if (doors.length !== 2) {
    fail(
      CORE,
      null,
      "this rule expects a `read` and a `press` and found " +
        doors.length +
        ", so it is guarding something that has moved",
    );
  }

  // The command is held to being called. A registered command is not evidence
  // that anything invokes it: a dead one on this project held the only
  // extension timing there was and read exactly like a working feature.
  if (!readFileSync("src/lib/exthost/commands.ts", "utf8").includes('"search_controls"')) {
    fail(
      DOOR,
      lineOf(door, door.indexOf("search_controls")),
      "`search_controls` is registered and the frontend never invokes it, so " +
        "nothing proves it is reachable and nothing would fail if it stopped " +
        "working",
    );
  }

  /*
   * The window read is the one the launcher took the foreground from.
   *
   * Not a parameter, deliberately: a handle passed in from the page is a
   * window the page could get wrong, and there is no honest way for a
   * launcher's own field to name a window it is sitting on top of. Taking one
   * would compile and would look right.
   */
  const boundary = door.lastIndexOf("#[tauri::command]", door.indexOf("search_controls"));
  const command = door.slice(boundary, door.indexOf("controls::read", boundary));

  if (!command.includes("previous_foreground()")) {
    fail(
      DOOR,
      lineOf(door, door.indexOf("search_controls")),
      "the control search names its own window instead of reading whichever " +
        "one the launcher took the foreground from",
    );
  }
}

/*
 * The high contrast fallback, still in place.
 *
 * Windows high contrast replaces every colour the page chose and DELETES
 * `box-shadow` outright. Sill draws its focus ring, its selected row, its
 * switches and its keycaps with box-shadow, and turns the browser's own
 * outline off in about thirty places to do it. So in the one mode somebody is
 * using because they cannot make out low contrast, every one of those
 * indicators disappears and nothing shows which row is highlighted or which
 * control has focus.
 *
 * One block in `theme.css` gives them all back with system colours. Nothing
 * about deleting it fails: the interface looks identical to everybody who is
 * not in that mode, which is everybody who would notice.
 *
 * The check is deliberately shallow. It cannot tell whether the block is
 * right; it can tell whether somebody removed it, or replaced the system
 * colour keywords with a palette that forced colours will throw away again.
 */
{
  const THEME = "src/lib/theme/theme.css";
  const css = readFileSync(THEME, "utf8");
  const opens = css.indexOf("@media (forced-colors: active)");

  if (opens === -1) {
    fail(
      THEME,
      null,
      "no `@media (forced-colors: active)` block. Windows high contrast strips " +
        "every box-shadow, which is what draws the focus ring and the selected row",
    );
  } else {
    const block = css.slice(opens);

    if (!/outline:\s*\d/.test(block)) {
      fail(
        THEME,
        lineOf(css, opens),
        "the forced-colors block sets no `outline`, so focus and selection are " +
          "still invisible there",
      );
    }

    // `Highlight`, `HighlightText`, `CanvasText` and `ButtonBorder` are what
    // the OS maps to whatever the user picked. Any other colour written here
    // is thrown away by the same mechanism this block exists to answer.
    if (!/\b(Highlight|HighlightText|CanvasText|ButtonBorder|ButtonText)\b/.test(block)) {
      fail(
        THEME,
        lineOf(css, opens),
        "the forced-colors block names no system colour, so whatever it paints " +
          "is replaced by the palette the user chose",
      );
    }
  }
}

/*
 * Every labelled line in settings is in the catalogue, in the panel that draws
 * it.
 *
 * `settings_index.rs` is what the launcher searches when somebody types the
 * name of a setting instead of hunting for it, and it is a hand-written table
 * beside a two thousand line page. Nothing made the two agree. Three ways they
 * had already come apart:
 *
 * - a row nobody added to the table is invisible to search, and on screen it
 *   looks exactly like a row that is in it;
 * - a row whose entry names a different panel opens settings somewhere it is
 *   not, which is worse than not finding it, because the person is now looking
 *   at the wrong screen believing it is the right one. "Screenshot hotkey"
 *   pointed at Shortcuts while the control sat in General;
 * - a panel in the table with no branch to draw it is a dead deep link.
 *
 * So this runs in both directions. Every static `<Row title="...">` on a
 * settings surface must be in the catalogue under the panel whose branch
 * renders it, and every panel the catalogue names must be in the sidebar and
 * have a branch of its own.
 *
 * A row that is not a setting says so, in a comment above it that also says
 * why: a per-item control inside a list, or a reading rather than a switch.
 * That is a sentence somebody has to write on purpose, which is the point.
 *
 * A run of rows written as a table and drawn in an `{#each}` counts too, and
 * has to: the switcher and screenshot keys are three entries in `BINDABLE`,
 * and reading only `title="..."` would have let a table of four rows sit in
 * one panel while the catalogue filed them under another. That is the shape
 * the check exists for. Every other `title={...}` is skipped, because what it
 * says is not in the file.
 */
{
  const SKIP_ROW = /<!--\s*not a setting:/;
  const CATALOGUE = "src-tauri/src/settings_index.rs";
  const catalogue = readFileSync(CATALOGUE, "utf8");

  /** The `PANELS` list in Rust, which is what a catalogue entry may name. */
  const rustPanels = new Set(
    [
      ...(catalogue.match(/pub const PANELS: &\[&str\] = &\[([\s\S]*?)\];/)?.[1] ?? "").matchAll(
        /"([^"]+)"/g,
      ),
    ].map((m) => m[1]),
  );

  /** Every catalogue entry, keyed by its panel and its title together. */
  const entries = new Map();

  for (const m of catalogue.matchAll(
    /\bs\(\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",?\s*\)/g,
  )) {
    entries.set(`${m[1]} ${m[3]}`, {
      panel: m[1],
      title: m[3],
      line: lineOf(catalogue, m.index),
    });
  }

  /**
   * The panels the page offers, in the order the sidebar lists them.
   *
   * Read from the `PANELS` array rather than from the branches, because the
   * array is what puts an entry in the sidebar and a branch is what fills the
   * pane. A panel in one and not the other is a blank screen either way.
   */
  const sidebar = new Set(
    [...(page.match(/const PANELS: Panel\[\] = \[([\s\S]*?)\n {2}\];/)?.[1] ?? "").matchAll(
      /id: "([^"]+)"/g,
    )].map((m) => m[1]),
  );

  /**
   * Which panel each line of the page belongs to.
   *
   * The pane is one `{#if active === "..."}` chain, so walking the file and
   * remembering the last branch seen is the whole mapping.
   */
  const lines = page.split("\n");
  const drawn = [];
  let holding = null;

  for (const line of lines) {
    const branch = line.match(/\{(?:#if|:else if) active === "([^"]+)"\}/);
    if (branch) holding = branch[1];
    drawn.push(holding);
  }

  const branches = new Set(drawn.filter(Boolean));

  /**
   * Every settings row, with the panel that draws it.
   *
   * A panel component is attributed to whichever branch renders it, and so is
   * anything that component renders in turn, so a row two files down from the
   * branch is still known to belong to that panel.
   */
  const COMPONENTS = "src/lib/components/settings";
  const rows = [];

  function collect(file, text, panel) {
    const own = text.split("\n");

    for (const m of text.matchAll(/<Row\b[^>]*?\btitle="([^"]*)"/g)) {
      const line = lineOf(text, m.index);

      // The reason goes above the row, where somebody reading the row sees it.
      if (SKIP_ROW.test(own[line - 2] ?? "")) continue;

      rows.push({ file, line, panel, title: m[1] });
    }
  }

  function walk(file, panel, seen) {
    if (seen.has(file)) return;
    seen.add(file);

    const text = readFileSync(file, "utf8");
    collect(file, text, panel);

    for (const m of text.matchAll(/<([A-Z][A-Za-z]*)\b/g)) {
      const child = join(COMPONENTS, `${m[1]}.svelte`);

      try {
        statSync(child);
      } catch {
        continue;
      }

      walk(child, panel, seen);
    }
  }

  for (let at = 0; at < lines.length; at += 1) {
    if (!drawn[at]) continue;

    for (const m of lines[at].matchAll(/<([A-Z][A-Za-z]*)\b/g)) {
      const child = join(COMPONENTS, `${m[1]}.svelte`);

      try {
        statSync(child);
      } catch {
        continue;
      }

      walk(child, drawn[at], new Set());
    }
  }

  for (const m of page.matchAll(/<Row\b[^>]*?\btitle="([^"]*)"/g)) {
    const line = lineOf(page, m.index);

    if (SKIP_ROW.test(lines[line - 2] ?? "")) continue;
    if (!drawn[line - 1]) continue;

    rows.push({ file: SETTINGS_PAGE, line, panel: drawn[line - 1], title: m[1] });
  }

  /**
   * The rows written as a table and drawn by an `{#each}`.
   *
   * The titles are literals like any other, one indirection further away. A
   * table is looked up by the name the loop walks, so a table declared and
   * never drawn is not judged.
   */
  for (let at = 0; at < lines.length; at += 1) {
    if (!drawn[at]) continue;

    const loop = lines[at].match(/\{#each ([A-Za-z_$][\w$]*)\b/);
    if (!loop) continue;

    const table = page.match(
      new RegExp(`\\n {2}const ${loop[1]}(?::[^=]*)? = \\[([\\s\\S]*?)\\n {2}\\];`),
    );

    if (!table) continue;

    for (const m of table[1].matchAll(/\btitle: "([^"]*)"/g)) {
      rows.push({
        file: SETTINGS_PAGE,
        line: lineOf(page, page.indexOf(table[1]) + m.index),
        panel: drawn[at],
        title: m[1],
      });
    }
  }

  for (const row of rows) {
    if (entries.has(`${row.panel} ${row.title}`)) continue;

    const elsewhere = [...entries.keys()]
      .filter((key) => key.endsWith(` ${row.title}`))
      .map((key) => entries.get(key).panel);

    fail(
      row.file,
      row.line,
      elsewhere.length
        ? `${JSON.stringify(row.title)} is drawn in the ${row.panel} panel and ` +
            `the catalogue files it under ${elsewhere.join(", ")}, so searching ` +
            `for it opens a panel it is not in. Fix ${CATALOGUE}`
        : `${JSON.stringify(row.title)} is not in ${CATALOGUE}, so nobody can ` +
            "find it by searching. Add it there, or say above the row why it " +
            "is not a setting",
    );
  }

  for (const { panel, title, line } of entries.values()) {
    if (!branches.has(panel)) {
      fail(
        CATALOGUE,
        line,
        `${JSON.stringify(title)} names the ${panel} panel, which ` +
          `${SETTINGS_PAGE} has no branch for, so opening it lands on whatever ` +
          "was last shown",
      );
    }
  }

  for (const panel of rustPanels) {
    if (sidebar.has(panel)) continue;

    fail(
      CATALOGUE,
      null,
      `the catalogue lists a ${panel} panel and the sidebar has no entry for it`,
    );
  }

  for (const panel of sidebar) {
    if (!rustPanels.has(panel)) {
      fail(
        SETTINGS_PAGE,
        null,
        `the sidebar has a ${panel} panel and ${CATALOGUE} does not, so nothing ` +
          "in it can be found from the launcher",
      );
    }

    if (!branches.has(panel)) {
      fail(SETTINGS_PAGE, null, `the sidebar has a ${panel} panel and nothing draws it`);
    }
  }
}

/*
 * Everything `Redo` names is acted on when preferences are saved.
 *
 * `Redo` is the list of settings the index has to be told about: the source
 * switches, the script folders, the folders the file index covers. Working out
 * that one of them changed and then not doing anything about it is the exact
 * failure this whole comparison exists to end, and it is invisible, because
 * the panel saves, the file is written, and the setting is simply not true
 * yet.
 *
 * Checked against the struct rather than against a list here, so a fourth
 * thing added to `Redo` arrives with the same obligation as the first three
 * instead of being computed and dropped.
 */
{
  const WHERE = "src-tauri/src/commands/settings.rs";
  const OWNER = "src-tauri/src/preferences.rs";
  const owner = readFileSync(OWNER, "utf8");
  const acts = readFileSync(WHERE, "utf8");

  const body = owner.match(/pub struct Redo \{([\s\S]*?)\n\}/)?.[1];

  if (!body) {
    fail(OWNER, null, "`Redo` is gone, and it is what the settings save acts on");
  } else {
    /** What the arm guarded by `redo.<field>` actually does, comments aside. */
    function armAfter(at) {
      const opens = acts.indexOf("{", at);
      if (opens === -1) return "";

      let depth = 0;

      for (let i = opens; i < acts.length; i += 1) {
        if (acts[i] === "{") depth += 1;
        if (acts[i] === "}") depth -= 1;
        if (depth === 0) {
          return acts
            .slice(opens + 1, i)
            .split("\n")
            .map((line) => line.replace(/\/\/.*$/, ""))
            .join("\n");
        }
      }

      return "";
    }

    for (const m of body.matchAll(/\n {4}pub ([a-z_]+):/g)) {
      const at = acts.indexOf(`redo.${m[1]}`);

      // A call, not merely a mention. An arm holding a comment and nothing
      // else reads as handled and is the failure itself.
      if (at !== -1 && /\w\s*\(/.test(armAfter(at))) continue;

      fail(
        WHERE,
        null,
        `\`Redo\` says whether ${m[1]} changed and nothing here acts on it, so ` +
          "that setting is saved and not applied until the next start",
      );
    }
  }
}

/*
 * And the comparison is made between the two preferences, not conjured.
 *
 * The rule above asks that every field of `Redo` is acted on. It says nothing
 * about where the `Redo` came from, and that is a hole with a name: replacing
 * `Redo::between(&previous, &prefs)` with `Redo::default()` leaves all three
 * arms present, satisfies the rule above, and **passes 2,020 Rust tests**.
 * Every source switch, script folder and indexed root would then save, report
 * itself saved, and change nothing until the next start. Measured, not
 * assumed.
 *
 * Both arguments are named rather than only the call, because the diff is only
 * a diff if it is between the state that was stored and the state arriving.
 * `Redo::between(&prefs, &prefs)` is empty for every save ever made.
 */
{
  const WHERE = "src-tauri/src/commands/settings.rs";
  const acts = readFileSync(WHERE, "utf8");
  const at = acts.indexOf("pub(crate) async fn set_preferences(");

  if (at < 0) {
    fail(WHERE, null, "set_preferences is gone, and it is what a settings save calls");
  } else {
    // Counted from the function, not from the file. A whole-file search here
    // would be satisfied by the phrase appearing in a doc comment somewhere
    // else in a 1,300 line module, which is how two rules in this script
    // passed their own sabotage before.
    const opened = acts.indexOf("{", at);
    let depth = 0;
    let end = opened;
    for (let i = opened; i < acts.length; i += 1) {
      if (acts[i] === "{") depth += 1;
      else if (acts[i] === "}" && --depth === 0) {
        end = i;
        break;
      }
    }

    const body = acts.slice(opened, end);

    if (!/Redo::between\(\s*&previous\s*,\s*&prefs\s*\)/.test(body)) {
      fail(
        WHERE,
        lineOf(acts, at),
        "set_preferences does not compare the stored preferences with the " +
          "ones arriving, so nothing below it can be true: the sources, the " +
          "script folders and the indexed roots are saved and never applied",
      );
    }
  }
}

/*
 * Every source switch is one the comparison reads.
 *
 * `Sources::scanned` is the tuple `Redo::between` compares, and it is a
 * hand-written list of the struct's own fields. A switch added to `Sources`
 * and not added there is a switch somebody can turn on in Settings that
 * changes nothing at all: the panel saves, the file is written, and the index
 * is never told, with no restart short of a Rebuild putting it right.
 *
 * Proved rather than supposed. An eighth `pub bool` on `Sources`, absent from
 * `scanned`, passes **2,020 Rust tests and this whole script**. This project
 * has been bitten five times by two lists that must agree with nothing making
 * them agree, and this is that shape exactly.
 *
 * Only the booleans. `folders` is in the tuple as a slice, and `excluded`,
 * `hidden` and `pinned` are deliberately out of it: they are read on every
 * query, so a word added to one is in effect on the next keystroke and asking
 * the machine to scan itself again for them would be a minute of work to
 * change nothing.
 */
{
  const WHERE = "src-tauri/src/preferences.rs";
  const text = readFileSync(WHERE, "utf8");
  const struct = text.match(/pub struct Sources \{([\s\S]*?)\n\}/)?.[1];
  const at = text.indexOf("fn scanned(&self)");

  if (!struct) {
    fail(WHERE, null, "`Sources` is gone, and it is what the source switches are");
  } else if (at < 0) {
    fail(WHERE, null, "`Sources::scanned` is gone, and it is what a save compares");
  } else {
    const opened = text.indexOf("{", at);
    let depth = 0;
    let end = opened;
    for (let i = opened; i < text.length; i += 1) {
      if (text[i] === "{") depth += 1;
      else if (text[i] === "}" && --depth === 0) {
        end = i;
        break;
      }
    }

    // Comments stripped, so a field named only in the prose above the tuple
    // does not read as one the comparison looks at.
    const compared = text
      .slice(opened, end)
      .split("\n")
      .map((line) => line.replace(/\/\/.*$/, ""))
      .join("\n");

    for (const m of struct.matchAll(/\n {4}pub ([a-z_]+): bool,/g)) {
      if (compared.includes(`self.${m[1]}`)) continue;

      fail(
        WHERE,
        lineOf(text, at),
        `\`Sources::${m[1]}\` is a switch \`scanned\` never reads, so turning ` +
          "it on or off saves and leaves the index exactly as it was",
      );
    }
  }
}

/*
 * A stored extension host is only handed back while it is still answering.
 *
 * `ExtHost` marks itself dead when its stream ends, and `tests/exthost.rs`
 * proves that: it kills a real Node and watches `alive()` turn over. What no
 * test reaches is the half that uses the answer. `host_of` takes an
 * `AppHandle`, so **deleting the liveness check hands a corpse back for the
 * rest of the session and fails nothing at all**, including the integration
 * test whose own name is about a host that died. Measured.
 *
 * What that costs: a crashed host left its handle in the slot and every later
 * launch got it back and failed with "channel is closed". The idle watchdog
 * could not clear it either, because asking for the host is what marks it
 * used, so a dead host looked permanently busy. Extensions stayed broken until
 * Sill was restarted.
 */
{
  const WHERE = "src-tauri/src/host.rs";

  if (existsSync(WHERE)) {
    const text = readFileSync(WHERE, "utf8");
    const at = text.indexOf("pub(crate) async fn host_of(");

    if (at < 0) {
      fail(WHERE, null, "host_of is gone, and it is what every extension launch asks");
    } else {
      const opened = text.indexOf("{", at);
      let depth = 0;
      let end = opened;
      for (let i = opened; i < text.length; i += 1) {
        if (text[i] === "{") depth += 1;
        else if (text[i] === "}" && --depth === 0) {
          end = i;
          break;
        }
      }

      const body = text.slice(opened, end);

      for (const [needle, why] of [
        [
          "host.alive()",
          "host_of hands back whatever is in the slot without asking whether " +
            "it still answers, so one crash breaks every extension until Sill " +
            "is restarted",
        ],
        [
          "*slot = None",
          "host_of never clears a dead host out of the slot, so the next " +
            "launch finds it there again and starts nothing",
        ],
      ]) {
        if (!body.includes(needle)) {
          fail(WHERE, lineOf(text, at), why);
        }
      }
    }
  }
}

/*
 * The two pages that name keys do not write a chord down.
 *
 * Every key on the reference comes from `keyboard_reference`, and every
 * sentence on the welcome comes from `welcome`, both assembled in Rust from
 * the keys that are actually bound. A chord typed into either component is a
 * promise nothing keeps: it survives the key being rebound, and the person
 * reading it has no way to tell it is stale.
 *
 * The welcome is the worse of the two, and it is why this rule grew a second
 * file. It is read once, before anybody knows enough to doubt it, and the key
 * it names has been **refused at every start on this machine for weeks**. A
 * hand-typed "Press Alt+Space" there is a first sentence that is false for
 * whoever most needs it to be true.
 *
 * This project has been bitten four times by a hand-kept list quietly
 * disagreeing with the thing it describes, which is why the rule is a build
 * failure rather than a note.
 */
{
  const PAGES = [
    ["src/lib/components/KeySheet.svelte", "keyboard_reference"],
    ["src/lib/components/Welcome.svelte", "welcome"],
  ];
  const CHORD = /"(?:Ctrl|Alt|Shift|Cmd|Meta|Super)\+[A-Za-z0-9+]+"/g;

  for (const [sheet, from] of PAGES) {
    if (!existsSync(sheet)) continue;
    const text = readFileSync(sheet, "utf8");

    text.split("\n").forEach((line, at) => {
      // A comment may name a chord as an example; only code counts.
      if (/^\s*(\/\/|\*|\/\*|<!--)/.test(line)) return;

      for (const found of line.matchAll(CHORD)) {
        fail(
          sheet,
          at + 1,
          `${found[0]} is written here rather than read from ${from}, ` +
            "so it goes on saying so after the key is rebound",
        );
      }
    });
  }
}

/*
 * The welcome reads what registration answered, not what was configured.
 *
 * This is the whole point of `P5-08`. `preferences.hotkey.summon` is the key
 * that was **asked for**; `HotkeyConflicts` holds what Windows **gave**, and
 * the two have disagreed on this machine at every start for weeks. A `welcome`
 * command that built its sentences from preferences alone would compile, pass
 * every test that does not run it, and open somebody's first minute with Sill
 * by telling them to press a key that does nothing.
 *
 * Nothing else can catch it. The command needs an `AppHandle` so no unit test
 * reaches it, and the module it calls is pure and would go on passing its own
 * tests while being handed a `summon_taken` that is always false.
 */
{
  const WHERE = "src-tauri/src/commands/settings.rs";

  if (existsSync(WHERE)) {
    const text = readFileSync(WHERE, "utf8");
    const at = text.indexOf("pub(crate) async fn welcome(");

    if (at < 0) {
      fail(WHERE, null, "the welcome command is gone, and with it the first run");
    } else {
      // To the closing brace of the function, counting from its own body.
      const opened = text.indexOf("{", at);
      let depth = 0;
      let end = opened;
      for (let i = opened; i < text.length; i++) {
        if (text[i] === "{") depth++;
        else if (text[i] === "}" && --depth === 0) {
          end = i;
          break;
        }
      }

      if (!text.slice(opened, end).includes("HotkeyConflicts")) {
        fail(
          WHERE,
          lineOf(text, at),
          "the welcome is built without reading HotkeyConflicts, so it says " +
            "the key in the settings file opens Sill whether or not Windows " +
            "ever gave it to us",
        );
      }
    }
  }
}

/*
 * A refused summon key opens the panel that actually holds that key.
 *
 * With the summon key taken there is no launcher to read a message in, so
 * `P1-11` opens the settings window instead. It opened `general`, which held
 * the row when that was written and stopped holding it when `P5-06` moved
 * every hotkey under Shortcuts. The window went on opening, on the wrong
 * panel, showing everything except the one control it was opened for, and
 * nothing anywhere said so.
 *
 * The settings catalogue already records which panel every row is in, so the
 * two are checked against each other rather than both being trusted.
 */
{
  const WHERE = "src-tauri/src/lib.rs";
  const CATALOGUE = "src-tauri/src/settings_index.rs";

  if (existsSync(WHERE) && existsSync(CATALOGUE)) {
    const named = /const SUMMON_SECTION: &str = "([^"]+)";/.exec(readFileSync(WHERE, "utf8"));

    if (!named) {
      fail(
        WHERE,
        null,
        "SUMMON_SECTION is gone, so nothing says which panel a refused summon " +
          "key should open",
      );
    } else {
      /*
       * The entry for the row itself, as `s(panel, name, title, keywords)`.
       * Matched on the title rather than on position, because the catalogue is
       * ordered by panel and an entry moving is not the failure being caught.
       */
      const entry = new RegExp(
        String.raw`s\(\s*"([^"]+)",\s*"[^"]*",\s*"Summon hotkey"`,
      ).exec(readFileSync(CATALOGUE, "utf8"));

      if (!entry) {
        fail(CATALOGUE, null, 'the catalogue has no "Summon hotkey" row to open');
      } else if (entry[1] !== named[1]) {
        fail(
          WHERE,
          null,
          `a refused summon key opens the ${named[1]} panel and the row that ` +
            `sets it is in ${entry[1]}, so the window opens on everything ` +
            "except the one control it was opened for",
        );
      }
    }
  }
}

/*
 * A preview asks whether the file is a cloud placeholder before opening it.
 *
 * Touching a placeholder downloads it, over a connection nobody chose to
 * spend, because somebody moved the selection past a row. `look_unless` takes
 * the question as a parameter so a test can hand over an ordinary file and say
 * it is a placeholder, which is the only way to test the rule at all: the
 * attribute cannot be set on a temporary file without a cloud provider signed
 * in on the machine.
 *
 * That makes the rule testable and leaves the wiring untested, which is the
 * same hole `after_recording` below is guarded against. Replacing
 * `is_elsewhere` with a closure returning false failed no test at all.
 */
{
  const WATCHED = "src-tauri/src/previews.rs";

  if (existsSync(WATCHED)) {
    const text = readFileSync(WATCHED, "utf8");
    const at = text.indexOf("fn look_at(");

    if (at < 0) {
      fail(WATCHED, null, "look_at is gone, and with it the placeholder refusal");
    } else {
      const body = text.slice(at, text.indexOf("\n}", at));

      if (!/look_unless\(\s*path\s*,\s*is_elsewhere\s*\)/.test(body)) {
        fail(
          WATCHED,
          lineOf(text, at),
          "look_at does not hand look_unless the real cloud question, so a " +
            "placeholder is opened and downloaded when somebody moves the " +
            "selection past it",
        );
      }

      // And the real question is still the attribute one. A version of this
      // that always answered false would satisfy the rule above.
      const asks = text.indexOf("fn is_elsewhere(");
      const real = asks < 0 ? "" : text.slice(asks, text.indexOf("\n}", asks));

      if (!real.includes("wants_recall(")) {
        fail(
          WATCHED,
          asks < 0 ? null : lineOf(text, asks),
          "is_elsewhere no longer asks wants_recall, so every placeholder " +
            "reads as an ordinary file",
        );
      }
    }
  }
}

/*
 * The uninstaller takes everything Sill actually leaves behind.
 *
 * `leavings.rs` is Sill's own list of what it writes outside its install
 * directory, and `installer/hooks.nsh` is what the uninstaller does about it.
 * They are one fact written twice, in two languages, and nothing in either
 * file makes them agree.
 *
 * That shape has gone stale four times in this codebase already. The cost
 * here is worse than usual in both directions: a stale list leaves somebody's
 * clipboard history and their sealed keys on a machine they thought they had
 * cleaned, or it deletes a folder Sill never owned.
 *
 * So adding somewhere Sill writes means adding it to both, and this refuses
 * the build until it is.
 */
{
  const LIST = "src-tauri/src/leavings.rs";
  const HOOKS = "src-tauri/installer/hooks.nsh";

  if (existsSync(LIST) && existsSync(HOOKS)) {
    const list = readFileSync(LIST, "utf8");
    const hooks = readFileSync(HOOKS, "utf8");

    // `where_it_is: r"..."`, which is how every entry names its place.
    const places = [...list.matchAll(/where_it_is:\s*r?"([^"]+)"/g)].map((m) => m[1]);

    if (places.length === 0) {
      fail(LIST, null, "no leavings are listed, so the uninstaller cleans up nothing");
    }

    for (const place of places) {
      // A registry value is named in two parts by NSIS, so the key and the
      // value are checked rather than the path as one string.
      const wanted = place.startsWith("HKCU\\")
        ? place.slice("HKCU\\".length).split("\\").slice(0, -1).join("\\")
        : place;

      if (!hooks.includes(wanted)) {
        fail(
          LIST,
          lineOf(list, list.indexOf(place)),
          `${place} is something Sill writes and the uninstaller never mentions it, ` +
            "so it is left on the machine after an uninstall",
        );
      }
    }

    // And the other way: the hooks must not remove something nothing claims.
    for (const removed of [...hooks.matchAll(/RMDir \/r "([^"]+)"/g)].map((m) => m[1])) {
      if (!places.includes(removed)) {
        fail(
          HOOKS,
          lineOf(hooks, hooks.indexOf(removed)),
          `the uninstaller deletes ${removed} and leavings.rs does not say Sill wrote it`,
        );
      }
    }
  }
}

/*
 * Every list that reaches the screen has one row per id.
 *
 * A repeated key in the result list's `{#each}` does not throw and does not
 * draw twice: it draws one row and says nothing. Measured against Svelte 5.56
 * in `RootList.svelte.test.ts`, on the first render and on an update. So the
 * failure is a result somebody searched for quietly not being in the list,
 * which is the kind of thing nobody reports because it looks like the thing
 * not existing.
 *
 * `show` is the one seam where the ranked list and the later files and
 * browser pages are joined, and Rust's own `one_per_id` cannot see across it.
 * `show` cannot be unit tested, so removing the call fails nothing, which is
 * the same hole `after_recording` below is guarded against.
 */
{
  const PAGE = "src/routes/+page.svelte";

  if (existsSync(PAGE)) {
    const text = readFileSync(PAGE, "utf8");
    const at = text.indexOf("function show(rows:");

    if (at < 0) {
      fail(PAGE, null, "show is gone, and with it the one place every list is joined");
    } else {
      const opened = text.indexOf("{", at);
      let depth = 0;
      let end = opened;
      for (let i = opened; i < text.length; i++) {
        if (text[i] === "{") depth++;
        else if (text[i] === "}" && --depth === 0) {
          end = i;
          break;
        }
      }

      if (!text.slice(opened, end).includes("onePerId(")) {
        fail(
          PAGE,
          lineOf(text, at),
          "show does not put its rows through onePerId, so two sources naming " +
            "the same row silently lose one of them and nothing says so",
        );
      }
    }
  }
}

/*
 * Open windows are still ranked in the same pass as everything else.
 *
 * They used to come back from a command of their own and be appended after
 * the index results had already been capped, so on a short query the cap
 * filled with weak command matches and a window whose title was an exact
 * match landed past the end of the list. Two lists concatenated is not a
 * ranking, and `P1-01` fixed it by chaining windows into the same corpus.
 *
 * `an_exact_window_title_outranks_a_scattered_command_match` proves the
 * ranker does the right thing given a corpus with a window in it. It says
 * nothing about whether windows reach that corpus, and `search_commands`
 * takes Tauri state so no unit test can call it: **deleting the chain removes
 * windows from search entirely and fails nothing.** Measured, not assumed.
 *
 * The same hole as `after_recording` below, and the fourth of its kind found
 * in this codebase.
 */
{
  const SEARCH = "src-tauri/src/commands/search.rs";

  if (existsSync(SEARCH)) {
    const text = readFileSync(SEARCH, "utf8");
    const at = text.indexOf("pub(crate) async fn search_commands(");

    if (at < 0) {
      fail(SEARCH, null, "search_commands is gone, and it is what a keystroke asks");
    } else {
      const opened = text.indexOf("{", at);
      let depth = 0;
      let end = opened;
      for (let i = opened; i < text.length; i++) {
        if (text[i] === "{") depth++;
        else if (text[i] === "}" && --depth === 0) {
          end = i;
          break;
        }
      }

      const body = text.slice(opened, end);

      // Built from the real source, and chained into the one ranked corpus.
      for (const [needle, why] of [
        [
          "windowing::recent_records(",
          "search_commands no longer asks for the open windows, so nothing " +
            "you have open is findable by typing its title",
        ],
        [
          ".chain(windows.iter())",
          "the open windows are not chained into the ranked corpus, so they " +
            "are either absent or appended after the cap, which is where an " +
            "exact window title used to land and never be seen",
        ],
      ]) {
        if (!body.includes(needle)) {
          fail(SEARCH, lineOf(text, at), why);
        }
      }
    }
  }
}

/*
 * The MCP secret is compared without stopping early.
 *
 * `same_secret` exists so that how long the comparison takes does not depend
 * on how much of the secret a caller guessed. **No test can hold that.**
 * Replacing the fold with `.all()` is still perfectly correct, passes every
 * assertion about which secrets are accepted, and quietly gives the property
 * back. Measured: that sabotage passed the unit test.
 *
 * Timing it instead would be a flaky test measuring a debug build, which is
 * worse than nothing. So the shape of the code is what is held, and this is
 * the only place that can hold it.
 */
{
  const LINK = "src-tauri/src/ai/mcp/link.rs";

  if (existsSync(LINK)) {
    const text = readFileSync(LINK, "utf8");
    const at = text.indexOf("fn same_secret(");

    if (at < 0) {
      fail(LINK, null, "same_secret is gone, and with it the constant time comparison of the MCP secret");
    } else {
      const opened = text.indexOf("{", at);
      let depth = 0;
      let end = opened;
      for (let i = opened; i < text.length; i++) {
        if (text[i] === "{") depth++;
        else if (text[i] === "}" && --depth === 0) {
          end = i;
          break;
        }
      }

      const body = text.slice(opened, end);

      if (!body.includes("fold(")) {
        fail(
          LINK,
          lineOf(text, at),
          "same_secret no longer folds over every byte, so the comparison " +
            "can stop at the first one that differs and how long it takes " +
            "says how much of the secret was right",
        );
      }

      for (const early of [".all(", ".any(", ".position(", ".find("]) {
        if (body.includes(early)) {
          fail(
            LINK,
            lineOf(text, at),
            `same_secret uses ${early}, which stops at the first byte that ` +
              "differs. It is still correct, which is exactly why no test " +
              "catches it",
          );
        }
      }
    }
  }
}

/*
 * Every recorded copy is still followed by the housekeeping.
 *
 * The two bounds on the clipboard, retention and the row cap, run from the
 * recording path rather than a timer, because a copy is the only moment the
 * history grows and a thread waking daily to find nothing to do is the idle
 * cost rule 23 refuses.
 *
 * That is also how the pruning was lost once already: the call sat below the
 * text branch's `return`, so retention was honoured on a machine where
 * somebody screenshots and not on one where they copy words, and the setting
 * did nothing for anybody who copies text. It was found months later.
 *
 * `after_recording` takes an `AppHandle`, so no unit test can call it, and
 * removing either line from its body fails nothing. This is that missing
 * assertion: the body has to call both, and every branch that records has to
 * call the body.
 */
{
  const WATCHED = "src-tauri/src/clipboard/monitor.rs";

  if (existsSync(WATCHED)) {
    const text = readFileSync(WATCHED, "utf8");
    const at = text.indexOf("fn after_recording(");

    if (at < 0) {
      fail(WATCHED, null, "after_recording is gone, and with it both clipboard bounds");
    } else {
      // The body, to its closing brace at column zero indentation.
      const opened = text.indexOf("{", at);
      let depth = 0;
      let end = opened;
      for (let i = opened; i < text.length; i++) {
        if (text[i] === "{") depth++;
        else if (text[i] === "}" && --depth === 0) {
          end = i;
          break;
        }
      }

      const body = text.slice(opened, end);

      for (const wanted of ["prune_occasionally(", "cap_rows("]) {
        if (!body.includes(wanted)) {
          fail(
            WATCHED,
            lineOf(text, at),
            `after_recording does not call ${wanted}, so that bound on the ` +
              "clipboard runs nowhere: there is no timer behind it",
          );
        }
      }

      // And the branches that record still go through it. Three today: a
      // refusal that was superseded, an image, and text.
      // Not `fn after_recording(app`, which is the definition. Counting
      // that as a call is how the first version of this rule passed a
      // sabotage that removed a branch.
      const calls = (text.match(/(?<!fn )after_recording\(app/g) || []).length;
      if (calls < 3) {
        fail(
          WATCHED,
          null,
          `only ${calls} recording branches call after_recording, and there ` +
            "were three. A branch that returns before it is a bound that " +
            "silently stops applying to whatever it records",
        );
      }
    }
  }
}

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


/*
 * Six files holding one version number.
 *
 * `tauri.conf.json` reads `package.json`, so that one is a pointer rather
 * than a copy and the check is that it stayed a pointer. The other five
 * cannot read anything: Cargo and npm both want the number written in the
 * file, and their lock files want it again.
 *
 * Left alone they drift, and the drift is invisible until somebody reads a
 * bug report. `CARGO_PKG_VERSION` is the log header, the MCP handshake and
 * the store's User-Agent; Settings shows `package_info().version`, which
 * comes from the Tauri config and therefore from `package.json`. A bump that
 * missed `Cargo.toml` gives one build two version numbers and no error.
 *
 * `npm run version:set 0.2.0` sets all of them from one argument.
 */
{
  const wanted = source();

  if (tauriVersion() !== TAURI_POINTER) {
    fail(
      "src-tauri/tauri.conf.json",
      null,
      `version is ${JSON.stringify(tauriVersion())} rather than ` +
        `${JSON.stringify(TAURI_POINTER)}. Tauri takes a path to a package.json ` +
        `where a semver string would go, which is what makes ${SOURCE} the only ` +
        "place the number is decided",
    );
  }

  for (const copy of read()) {
    if (copy.version === wanted) continue;

    fail(
      copy.file,
      null,
      `says ${copy.version === null ? "nothing readable" : copy.version} where ` +
        `${SOURCE} says ${wanted}. This is ${copy.what}. ` +
        `Run \`npm run version:set ${wanted}\``,
    );
  }
}

/*
 * Nothing changes what an extension holds without telling the worker running it.
 *
 * The capabilities a command is gated by were handed to its worker once, in the
 * launch payload, and never again. So revoking in Settings wrote the file,
 * satisfied the next launch, and reached the command on screen not at all: the
 * extension somebody had just revoked went on reading the disk and reaching the
 * network until something unloaded it. A permission that can be revoked and
 * does not take effect is worse than one that cannot.
 *
 * `Granted::announce` is what closes that, and the failure this catches is a
 * fourth way to change a grant written by somebody with no reason to know that,
 * which compiles, passes every test about the file on disk, and silently puts
 * the hole back. Every method that changes the map has to announce, so the
 * check is "mutates and does not announce" rather than a list of method names
 * that would go stale the moment a fifth one is added.
 */
{
  const GRANTS = "src-tauri/src/exthost/grants.rs";
  const text = readFileSync(GRANTS, "utf8");

  // Each `fn` in the file with its body, cut at the next one. Crude and
  // sufficient: what matters is that a mutation and an announcement land in
  // the same slice.
  const bodies = text.split(/\n    (?:pub )?fn /).slice(1);
  let checked = 0;

  for (const body of bodies) {
    const name = body.split(/[(<]/)[0];
    if (!body.includes("by_extension.lock()")) continue;
    if (!/\.(?:entry|remove|get_mut)\(/.test(body)) continue;

    checked += 1;
    if (body.includes("self.announce(")) continue;

    fail(
      GRANTS,
      lineOf(text, text.indexOf(`fn ${name}`)),
      `\`${name}\` changes what an extension holds and does not call ` +
        "`self.announce`, so a command already running keeps the permission " +
        "somebody just took away",
    );
  }

  if (checked < 3) {
    fail(
      GRANTS,
      null,
      `only ${checked} grant-changing methods found, so this is parsing rather ` +
        "than checking",
    );
  }
}

/*
 * Nothing is removed by a name that was never resolved.
 *
 * An extension has two names and nothing makes them agree: the catalogue is
 * keyed by a store slug, its directory is named by its own `package.json`, and
 * `translate` in the store is `google-translate` on disk. `store::installed_as`
 * is the join, and the failure it exists to stop is quiet in both directions.
 * Removing by an unresolved slug deletes nothing, says "was not installed", and
 * leaves the bundles, the index entry and every granted permission where they
 * were; where something else is installed under that slug, it removes the wrong
 * extension.
 *
 * The check is here rather than inside `uninstall` because that function takes
 * paths and a directory name, which is the right signature for it: the caller
 * is the layer that knows which of the two names it was handed.
 */
{
  const REMOVES = /crate::store::install::uninstall\(/g;

  for (const file of sources("src-tauri/src")) {
    const text = readFileSync(file, "utf8");

    for (const found of text.matchAll(REMOVES)) {
      const before = text.slice(0, found.index).split("\n").slice(-30).join("\n");

      if (before.includes("installed_as(")) continue;

      fail(
        file,
        lineOf(text, found.index),
        "this removes an extension by a name nothing resolved through " +
          "`crate::store::installed_as`, so a store slug that differs from the " +
          "directory removes nothing or removes the wrong one",
      );
    }
  }
}

/*
 * The extension coverage table says what it is a table of.
 *
 * `docs/extensions.md` tells an author which Raycast APIs Sill answers, which
 * throw a reason of their own, and what the window does with every component
 * tag. That is exactly the kind of page that is wrong within a week and gives
 * the reader no way to tell: the module an extension receives is a proxy, so
 * anything the host does not export throws at the moment it is touched, and a
 * table that has drifted is a promise the runtime does not keep.
 *
 * So the rows are not maintained, they are held to the source. The names come
 * out of the host's own exports and its component declarations, and both
 * directions fail: an API added without a row, and a row left behind after the
 * API went. The prose beside each name is a person's job and is not checked.
 *
 * Three sets, because the page makes three different claims:
 *
 * - **answered**, everything exported that does the thing it names;
 * - **refused**, the few exported only to throw a reason of their own, which
 *   is not the same as a gap and must not be listed as though it were;
 * - **tags**, every component tag that reaches the window.
 */
{
  const DOC = "docs/extensions.md";
  const API = "host/src/api/index.ts";
  const UTILS = "host/src/utils/index.ts";
  const COMPONENTS = "host/src/api/components.ts";

  /**
   * What a module exports at runtime.
   *
   * Types are left out because they do not exist while the extension runs, and
   * the proxy that throws only ever sees values. `export { type X }` and
   * `export interface` are both skipped for that reason.
   */
  function exported(file) {
    const text = readFileSync(file, "utf8");
    const names = new Set();

    for (const found of text.matchAll(/^export\s*\{([^}]*)\}/gm)) {
      for (const entry of found[1].split(",")) {
        const bit = entry.trim();
        if (!bit || bit.startsWith("type ")) continue;
        names.add((bit.split(/\s+as\s+/).pop() ?? bit).trim());
      }
    }

    for (const found of text.matchAll(
      /^export\s+(?:async\s+)?(?:const|function|class)\s+([A-Za-z_$][\w$]*)/gm,
    )) {
      names.add(found[1]);
    }

    return names;
  }

  /**
   * The exports of `@raycast/utils` that exist to throw a reason.
   *
   * Sliced between one top-level declaration and the next, exported or not,
   * because `notHere` itself is declared in the middle of them and a slice cut
   * only at `export` swallows it into whichever function it follows. That
   * mistake reported `runPowerShellScript` as refused, which is the opposite
   * of true.
   */
  function refusedByName(file) {
    const text = readFileSync(file, "utf8");
    const declared = [
      ...text.matchAll(
        /^(export\s+)?(?:async\s+)?(?:const|function|class)\s+([A-Za-z_$][\w$]*)/gm,
      ),
    ];

    const refused = new Set();

    declared.forEach((one, index) => {
      if (!one[1]) return;
      const until = declared[index + 1]?.index ?? text.length;
      if (/\bnotHere\(/.test(text.slice(one.index, until))) refused.add(one[2]);
    });

    return refused;
  }

  /** Every component tag the API layer can put on the wire. */
  function componentTags(file) {
    const text = readFileSync(file, "utf8");
    const found = new Set([...text.matchAll(/host<AnyProps>\("([^"]+)"\)/g)].map((m) => m[1]));

    // `Action.Push` is not built by `host`, because it does its own work, and
    // it still reaches the window under its own tag. Read from the name it
    // sets rather than written out here, so a second one added the same way
    // arrives without anybody remembering this line.
    for (const named of text.matchAll(/\bdisplayName = "([^"]+)"/g)) found.add(named[1]);

    return found;
  }

  /** The backticked names in the first column of a marked region's table. */
  function listed(text, region) {
    const between = text.match(new RegExp(`<!-- ${region} -->([\\s\\S]*?)<!-- /${region} -->`));

    if (!between) {
      fail(DOC, null, `no \`${region}\` region, so nothing there is being checked`);
      return undefined;
    }

    const names = new Set();
    for (const row of between[1].split("\n")) {
      const cell = row.match(/^\|\s*`([^`]+)`\s*\|/);
      if (cell) names.add(cell[1]);
    }
    return names;
  }

  const doc = readFileSync(DOC, "utf8");

  const refused = refusedByName(UTILS);
  const answered = new Set(
    [...exported(API), ...exported(UTILS)].filter((name) => !refused.has(name)),
  );
  const drawn = componentTags(COMPONENTS);

  /*
   * A parse that found nothing would agree with an empty page and pass every
   * comparison below. These floors are far under the real counts and exist
   * only to turn a broken regex into a failure rather than into a silent yes,
   * which is the same treatment the grant-map rule above gets.
   */
  for (const [what, set, least] of [
    ["answered", answered, 20],
    ["refused", refused, 3],
    ["tag", drawn, 30],
  ]) {
    if (set.size >= least) continue;
    fail(
      DOC,
      null,
      `only ${set.size} ${what} name(s) found in the host, so this is parsing ` +
        "rather than checking",
    );
  }

  // Sill's own module, held to the page the same way. It is smaller and newer
  // than the Raycast surface, which is exactly when a table starts drifting:
  // there is not yet enough of it for anybody to notice a missing row.
  const SILL = "host/src/sill/index.ts";
  const sillsOwn = exported(SILL);

  if (sillsOwn.size < 3) {
    fail(
      SILL,
      null,
      `only ${sillsOwn.size} name(s) found in Sill's own API module, so this is ` +
        "parsing rather than checking",
    );
  }

  for (const [region, expected, where] of [
    ["coverage:answered", answered, `${API} and ${UTILS}`],
    ["coverage:refused", refused, UTILS],
    ["coverage:tags", drawn, COMPONENTS],
    ["coverage:sill", sillsOwn, SILL],
  ]) {
    const rows = listed(doc, region);
    if (!rows) continue;

    for (const name of expected) {
      if (rows.has(name)) continue;
      fail(
        DOC,
        null,
        `\`${name}\` is in ${where} and has no row under \`${region}\`, so the ` +
          "coverage table understates what an extension can reach",
      );
    }

    for (const name of rows) {
      if (expected.has(name)) continue;
      fail(
        DOC,
        null,
        `\`${name}\` has a row under \`${region}\` and is not in ${where}, so the ` +
          "table promises something the host does not answer",
      );
    }
  }
}

/*
 * The page's account of what an extension may reach is the gate's own.
 *
 * `docs/extensions.md` tells a reader which Node built-ins cost a permission,
 * which come free, and by implication which are refused outright, and somebody
 * choosing whether to install a stranger's code reads that. A page saying `fs`
 * is gated while the gate had quietly stopped gating it is the worst kind of
 * documentation drift there is, and this project has already shipped the
 * reverse of it once: the same page said `eval` and a dynamic `import()` were
 * ways out long after one of them was a hole and after none of them was.
 *
 * So both lists are held to `patch-require.ts`. The gated table is checked in
 * both columns, name and permission phrase, because a row naming the wrong
 * permission is a row that reads as a promise and is not one. The free list is
 * checked by name, and it is the security-relevant one: a built-in that stops
 * being free is a built-in the page still says costs nothing.
 */
{
  const DOC = "docs/extensions.md";
  const GATE = "host/src/worker/patch-require.ts";

  /** One section of a marked region, as raw text. */
  function region(text, name) {
    const between = text.match(new RegExp(`<!-- ${name} -->([\\s\\S]*?)<!-- /${name} -->`));
    if (!between) {
      fail(DOC, null, `no \`${name}\` region, so nothing there is being checked`);
      return undefined;
    }
    return between[1];
  }

  /** Everything backticked in a stretch of the page. */
  const backticked = (text) => new Set([...text.matchAll(/`([^`]+)`/g)].map((m) => m[1]));

  const gate = readFileSync(GATE, "utf8");

  /**
   * `GATED` as the gate declares it, module name to the phrase a refusal uses.
   *
   * Read between the declaration and its closing brace rather than over the
   * whole file, so the `BINDINGS` table below it, which is keyed by binding
   * name and not by module, cannot be swept in.
   */
  const gatedSource = gate.match(/const GATED[^{]*\{([\s\S]*?)\n\};/)?.[1] ?? "";
  const gated = new Map(
    [...gatedSource.matchAll(/^\s*(\w+):\s*\{[^}]*plainly:\s*"([^"]+)"/gm)].map((m) => [
      m[1],
      m[2],
    ]),
  );

  const freeSource = gate.match(/const FREE = new Set\(\[([\s\S]*?)\]\)/)?.[1] ?? "";
  const free = new Set([...freeSource.matchAll(/"([^"]+)"/g)].map((m) => m[1]));

  // A regex that found nothing agrees with anything, so the floors turn a
  // broken parse into a failure rather than a silent pass.
  for (const [what, size, least] of [
    ["gated", gated.size, 8],
    ["free", free.size, 15],
  ]) {
    if (size >= least) continue;
    fail(GATE, null, `only ${size} ${what} module(s) parsed out, so this is not checking`);
  }

  const gatedRegion = region(readFileSync(DOC, "utf8"), "coverage:gated");
  const freeRegion = region(readFileSync(DOC, "utf8"), "coverage:free");

  if (gatedRegion !== undefined) {
    const rows = new Map(
      [...gatedRegion.matchAll(/^\|\s*`([^`]+)`\s*\|\s*([^|]+?)\s*\|/gm)].map((m) => [m[1], m[2]]),
    );

    for (const [name, plainly] of gated) {
      if (rows.get(name) === plainly) continue;
      fail(
        DOC,
        null,
        `\`${name}\` needs a row under \`coverage:gated\` reading "${plainly}", the ` +
          `phrase its refusal uses; the page says ` +
          `${rows.has(name) ? `"${rows.get(name)}"` : "nothing"}`,
      );
    }

    for (const name of rows.keys()) {
      if (gated.has(name)) continue;
      fail(
        DOC,
        null,
        `\`${name}\` has a row under \`coverage:gated\` and ${GATE} does not gate it, ` +
          "so the page describes a permission nothing asks for",
      );
    }
  }

  if (freeRegion !== undefined) {
    const named = backticked(freeRegion);

    for (const name of free) {
      if (named.has(name)) continue;
      fail(DOC, null, `\`${name}\` is handed over for free and \`coverage:free\` omits it`);
    }

    for (const name of named) {
      if (free.has(name)) continue;
      fail(
        DOC,
        null,
        `\`coverage:free\` says \`${name}\` costs nothing and ${GATE} does not hand ` +
          "it over, so the page promises a module an extension will be refused",
      );
    }
  }
}

/*
 * One spelling for a kind, and one for a capability, across three languages.
 *
 * An extension's manifest writes `"actionOn": ["file"]`, Rust reads that name
 * back through `ObjectKind::named`, sends the object to the worker with the
 * same name on it, and `@sill/api` types it as a union so an author's editor
 * can complete it. Four places, one vocabulary. The failure if they drift is
 * the worst kind there is here: an action that is simply never offered, with
 * nothing anywhere saying why, because a name nobody knows is a name that gets
 * skipped.
 *
 * `Capability` is the same story from the permission side. A name in the union
 * that Rust does not serialise is an extension checking `holds("fileReading")`
 * and getting a permanent false while holding the permission.
 *
 * Read out of Rust rather than out of the doc page, because Rust is where both
 * enums live and a page is a third copy.
 */
{
  const KINDS = "src-tauri/src/object.rs";
  const CAPABILITIES = "src-tauri/src/action.rs";
  const TYPES = "host/src/sill/index.ts";

  const types = readFileSync(TYPES, "utf8");

  /** A `export type X = "a" | "b";` union, as a set of its members. */
  function union(name) {
    const declared = types.match(new RegExp(`export type ${name} =([\\s\\S]*?);`))?.[1];
    if (declared === undefined) {
      fail(TYPES, null, `no \`${name}\` union, so nothing about it is being checked`);
      return undefined;
    }
    return new Set([...declared.matchAll(/"([^"]+)"/g)].map((m) => m[1]));
  }

  /**
   * The names `ObjectKind::name` answers with.
   *
   * That function rather than the enum's variants, because it is the one that
   * decides the spelling, and its own unit test holds it to what serde does.
   */
  const kindBody =
    readFileSync(KINDS, "utf8").match(
      /pub fn name\(self\) -> &'static str \{[\s\S]*?\n {8}\}/,
    )?.[0] ?? "";
  const kinds = new Set([...kindBody.matchAll(/=> "([^"]+)",/g)].map((m) => m[1]));

  /**
   * The capability names, camelCased the way `#[serde(rename_all)]` does.
   *
   * Read between the enum's own braces, so the `Undo` and `Outcome` types
   * further down the same file cannot be swept in.
   */
  const capBody =
    readFileSync(CAPABILITIES, "utf8").match(/pub enum Capability \{([\s\S]*?)\n\}/)?.[1] ?? "";
  const capabilities = new Set(
    [...capBody.matchAll(/^ {4}([A-Z][A-Za-z]*),$/gm)].map(
      (m) => m[1].charAt(0).toLowerCase() + m[1].slice(1),
    ),
  );

  // Floors, because an empty set agrees with an empty union.
  for (const [what, set, least] of [
    ["object kind", kinds, 20],
    ["capability", capabilities, 10],
  ]) {
    if (set.size >= least) continue;
    fail(
      set === kinds ? KINDS : CAPABILITIES,
      null,
      `only ${set.size} ${what}(s) parsed out of Rust, so this is not checking`,
    );
  }

  for (const [name, fromRust, where] of [
    ["SillObjectKind", kinds, `${KINDS}'s ObjectKind::name`],
    ["SillCapability", capabilities, `${CAPABILITIES}'s Capability`],
  ]) {
    const declared = union(name);
    if (!declared) continue;

    for (const one of fromRust) {
      if (declared.has(one)) continue;
      fail(
        TYPES,
        null,
        `\`${one}\` is in ${where} and not in the \`${name}\` union, so an ` +
          "extension author cannot name it and their editor will call it a mistake",
      );
    }

    for (const one of declared) {
      if (fromRust.has(one)) continue;
      fail(
        TYPES,
        null,
        `\`${name}\` offers \`${one}\` and ${where} has no such name, so it type ` +
          "checks here and is never true at run time",
      );
    }
  }
}

/*
 * The thing an action was run on reaches the worker, all four hops of it.
 *
 * `Contributed::run` hands it to `open_extension_command`, which puts it on
 * the `LoadOptions`, which Rust serialises as `on`, which the host reads and
 * gives the worker as `sillObject`, which `@sill/api`'s `actionTarget` hands
 * back. Break any one and every contributed action silently behaves as though
 * it had been picked off the root list: `actionTarget()` is `undefined` and a
 * well-written extension politely declines to do anything.
 *
 * A rule rather than a test because the harness that runs the view gate stands
 * in for Rust and sends this field itself, so it agrees with a Rust that has
 * stopped sending it. That is the exact failure `run-extension.mjs`'s own
 * header warns about, where the gate ran green for months against a
 * `Clipboard.copy` that no Rust arm answered.
 */
{
  const hops = [
    [
      "src-tauri/src/actions/extension.rs",
      /open_extension_command\(ctx, &record, &self\.title, Some\(object\)\)/,
      "`Contributed::run` no longer hands over what it was run on",
    ],
    [
      "src-tauri/src/actions/mod.rs",
      /opts\.on = on\.cloned\(\);/,
      "`open_extension_command` no longer puts it on the load options",
    ],
    [
      "src-tauri/src/exthost/manager.rs",
      /pub on: Option<crate::object::Object>,/,
      "`LoadOptions` has no field to carry it",
    ],
    [
      "host/src/index.ts",
      /sillObject: \(opts\.on \?\? undefined\)/,
      "the host does not read it off the load options",
    ],
    [
      "host/src/worker/worker.ts",
      /on: data\.sillObject,/,
      "the worker does not put it on the bridge",
    ],
    [
      "host/src/sill/index.ts",
      /return getBridge\(\)\.on as SillObject \| undefined;/,
      "`actionTarget` does not read it off the bridge",
    ],
  ];

  for (const [file, expected, said] of hops) {
    if (expected.test(readFileSync(file, "utf8"))) continue;
    fail(
      file,
      null,
      `${said}, so every action an extension contributes runs as though ` +
        "somebody had picked it off the root list",
    );
  }
}

/*
 * The index's commands and the actions extensions contribute are replaced
 * together, in one place.
 *
 * They are one fact read twice: the index says which extension commands exist,
 * and the action registry says which of them can be run on a file. Set one
 * without the other and the panel offers to run a command out of an extension
 * that has just been uninstalled, or fails to offer one that has just been
 * installed until the next restart.
 *
 * This is the "two lists that must agree, with nothing making them agree"
 * shape, and the thing that makes them agree is that there is one funnel.
 */
{
  const LIB = "src-tauri/src/lib.rs";
  const text = readFileSync(LIB, "utf8");

  const funnel = text.match(/pub\(crate\) async fn adopt_commands\([\s\S]*?\n\}/)?.[0];

  if (funnel === undefined) {
    fail(LIB, null, "`adopt_commands` is gone, so nothing is keeping the two lists in step");
  } else if (!/\.contribute\(/.test(funnel)) {
    fail(
      LIB,
      null,
      "`adopt_commands` no longer tells the action registry what extensions " +
        "contribute, so installing one adds no action until Sill is restarted",
    );
  }

  for (const found of text.matchAll(/index\.commands = /g)) {
    const inside =
      funnel !== undefined &&
      found.index >= text.indexOf(funnel) &&
      found.index < text.indexOf(funnel) + funnel.length;

    if (inside) continue;

    fail(
      LIB,
      lineOf(text, found.index),
      "the index's commands are replaced outside `adopt_commands`, so what " +
        "extensions contribute to the action panel is not rebuilt with them",
    );
  }
}

/*
 * An MCP server is started by somebody pressing something, and by nothing else.
 *
 * **This is the rule the whole of `P8-05` rests on.** An extension is code Sill
 * installed; an MCP server is somebody else's program on the far end of a pipe,
 * which may be slow, may hang, and may have been uninstalled since the day it
 * was configured. The action panel is drawn on a keystroke. If drawing it asked
 * a server anything, one dead server would be a launcher that stops responding,
 * and the fix after the fact is a cache, and then the cache needs invalidating.
 *
 * So `actions::mcp::contributed` is a pure function over the declarations in
 * the preferences, and the client that starts a process is reachable from
 * exactly two places: running one of the actions, and the Check button in
 * Settings. Both are somebody pressing something.
 *
 * A rule rather than a test because the two call sites take Tauri state: one is
 * a `#[tauri::command]` and the other is an `Action::run` holding an
 * `AppHandle`, and neither can be constructed without a running application.
 * `tests/actions.rs::drawing_the_panel_never_waits_for_a_server` holds the
 * timing half; this holds the half a clock cannot see.
 */
{
  const CORE = "src-tauri/src/actions/mcp.rs";
  const DOOR = "src-tauri/src/commands/mcp.rs";
  const CLIENT = "src-tauri/src/ai/mcp/client.rs";

  const core = readFileSync(CORE, "utf8");

  /*
   * The builder names nothing that could start a process.
   *
   * Read out of the function rather than out of the file, because `run` in the
   * same file is *supposed* to call the client and a file-wide search would
   * either pass always or fail always. That is the mistake `P8-02` made first
   * and had to rewrite.
   */
  const builder = core.match(/pub fn contributed\([\s\S]*?\n\}/)?.[0];

  if (builder === undefined) {
    fail(CORE, null, "`contributed` is gone, so nothing builds the MCP half of the action panel");
  } else if (/client::|Command::new|spawn\(/.test(builder)) {
    fail(
      CORE,
      lineOf(core, core.indexOf(builder)),
      "`actions::mcp::contributed` reaches for the MCP client, so drawing the " +
        "action panel would start somebody else's program on a keystroke and " +
        "a dead server would hang the launcher",
    );
  }

  /*
   * And running one goes through the client rather than opening its own pipe.
   *
   * The seam a unit test cannot reach: `Contributed::run` takes an `ActionCtx`
   * holding a concrete `AppHandle`. What it must do is look the server up
   * live, because somebody can remove it between the panel being drawn and the
   * row being pressed, and then call the one client that carries the deadline.
   */
  for (const [expected, said] of [
    [
      /let server = configured\(ctx, &self\.server\)\.await\?;/,
      "`Contributed::run` no longer looks the server up at the moment it runs, " +
        "so a server somebody has removed is still started",
    ],
    [
      /crate::ai::mcp::client::call\(/,
      "`Contributed::run` does not go through the MCP client, so whatever it " +
        "does instead has no deadline and a hung server hangs the action",
    ],
  ]) {
    if (expected.test(core)) continue;
    fail(CORE, null, said);
  }

  /*
   * Nothing else in Sill may start one.
   *
   * Two files, and the reason each is allowed is different: one is an action
   * somebody ran, the other is a button somebody pressed. A third caller would
   * be a server started for a reason nobody chose, which is the whole of what
   * "nothing at rest" means here.
   */
  for (const found of sources("src-tauri/src")) {
    const file = found.replace(/\\/g, "/");
    if (file === CORE || file === DOOR || file === CLIENT) continue;
    if (!file.endsWith(".rs")) continue;

    const text = readFileSync(found, "utf8");

    for (const at of text.matchAll(/mcp::client::(call|tools)\b/g)) {
      fail(
        file,
        lineOf(text, at.index),
        "this starts an MCP server, and the only two things that may are " +
          "running one of its actions and the Check button in Settings",
      );
    }
  }

  /*
   * Both halves of what is contributed are built in the one funnel.
   *
   * `ActionRegistry::contribute` replaces the whole list, so a second place
   * that built only the MCP half would take every extension's action out of
   * the panel, and one that built only the extension half would do the
   * reverse. Neither failure says anything at the time. The rule above already
   * holds `.contribute(` to `adopt_commands`; this holds the MCP builder to it
   * too, so the funnel cannot be left half filled.
   */
  const lib = readFileSync("src-tauri/src/lib.rs", "utf8");
  const funnel = lib.match(/pub\(crate\) async fn adopt_commands\([\s\S]*?\n\}/)?.[0];

  for (const found of lib.matchAll(/actions::mcp::contributed\(/g)) {
    const inside =
      funnel !== undefined &&
      found.index >= lib.indexOf(funnel) &&
      found.index < lib.indexOf(funnel) + funnel.length;

    if (inside) continue;

    fail(
      "src-tauri/src/lib.rs",
      lineOf(lib, found.index),
      "what MCP servers contribute is built outside `adopt_commands`, so it " +
        "is not rebuilt together with what extensions contribute",
    );
  }

  if (funnel !== undefined && !/actions::mcp::contributed\(/.test(funnel)) {
    fail(
      "src-tauri/src/lib.rs",
      null,
      "`adopt_commands` no longer builds what MCP servers contribute, so a " +
        "rescan silently takes every MCP action out of the action panel",
    );
  }

  /*
   * And a settings save that changed them asks the funnel to run again.
   *
   * Without this the panel would go on offering the servers that were
   * configured when Sill started until something else happened to trigger a
   * rescan, which for most people is never.
   */
  const settings = readFileSync("src-tauri/src/commands/settings.rs", "utf8");

  if (!/previous\.mcp != prefs\.mcp[\s\S]{0,200}readopt_commands\(&app\)/.test(settings)) {
    fail(
      "src-tauri/src/commands/settings.rs",
      null,
      "saving a changed set of MCP servers does not rebuild the action panel, " +
        "so a server somebody just added contributes nothing until a restart",
    );
  }
}

/*
 * Nothing repeating in the window may talk to Rust unless being hidden stops
 * it.
 *
 * **This is the one performance budget a shared build agent can honestly
 * hold.** Every other row of `docs/budgets.md` is milliseconds or megabytes of
 * a running application, and an agent is a borrowed virtual machine with no
 * display, no graphics hardware and neighbours competing for its cores, so a
 * threshold in either unit there measures the agent. This one is a count, and
 * the count has to be zero.
 *
 * What it is protecting is a claim rather than a number: **a launcher nobody
 * is looking at makes no network calls.** That was false for a long time and
 * nothing said so. A widget pinned to the chin asked a weather service for a
 * reading every ten minutes for as long as the application was running,
 * because `setInterval` in `onMount` runs until the component is destroyed and
 * hiding a window destroys nothing. Six calls an hour, on behalf of a window
 * that had been put away since breakfast.
 *
 * The fix was `pollWhileVisible`, and this is what stops the next widget from
 * missing it. A repeating timer that reaches Rust is the shape the bug had:
 * `weather_now` is a network call and `machine_reading` walks every process on
 * the machine, and from this file neither is distinguishable from the other,
 * which is the point. Both are work done for nobody.
 *
 * **Scoped to `setInterval` deliberately.** A `setTimeout` that reschedules
 * itself is the same hazard and cannot be recognised without following the
 * code, and a rule that guessed would be red for reasons nobody could act on.
 * The clock widget is exactly that shape and is exactly why: it draws from the
 * machine's own time and never asks Rust anything, so the rule that would
 * catch it has nothing to catch.
 *
 * The Rust half of the same claim is not checked here and cannot usefully be.
 * A `sleep` in Rust is nearly always a one-shot wait for something to settle,
 * a few dozen of them are, and telling those from a poller needs the call
 * graph. What answers for Rust is the count taken on a real machine by
 * `scripts/measure-network.ps1`.
 */
{
  /**
   * Timers that repeat, reach Rust, and are allowed to.
   *
   * Each entry has to say why being hidden cannot leave it running. A file
   * added here without that reasoning is the check being switched off one
   * line at a time.
   */
  const ALLOWED = {
    // Stopped by Rust rather than by the page. `liveRows` returns nothing once
    // Rust decides the launcher is not visible, and the ticker stops itself on
    // an empty answer. Deliberately not the page's own decision: the window
    // goes away by the hotkey, by a click elsewhere and by an action putting
    // it away, and a timer recognising all three would be right until somebody
    // added a fourth.
    "src/routes/+page.svelte": "stops on an empty answer from `liveRows`",
    // The settings window, which is closed rather than hidden, so there is no
    // hidden state for a poller to survive into. It reads a local setup's
    // progress and reaches no network at all.
    "src/lib/components/settings/DictationPanel.svelte":
      "the settings window closes rather than hides, and it reads local setup progress",
    // One wakeup a minute, so "3 minutes ago" under an answer stays true.
    // Cleared by the effect that made it, so it lives exactly as long as the
    // window it is in, and it asks Rust nothing and reaches no network.
    "src/routes/ask/+page.svelte":
      "one wakeup a minute for relative times, cleared by its own effect",
  };

  /** The module that owns the rule, and therefore the one timer that is real. */
  const OWNS_THE_RULE = "src/lib/visible.ts";

  let checked = 0;

  /*
   * Counted as well as judged, because this row is on the published cost page
   * and the page never writes a number down itself.
   *
   * `scripts/measure-checks.mjs` reads the line printed below and records it,
   * so the count a reader sees is the one this rule arrived at rather than a
   * second count taken by something that reimplemented the rule.
   *
   * What gets published is the count this rule can stand behind: how many
   * repeating timers are in the window, and how many of those are unaccounted
   * for. Not how many reach Rust. This finds a call to Rust by the word
   * `invoke` in the same file, and the two timers that do reach it go through
   * a wrapper a module away, so a published figure for that would be a figure
   * this cannot see.
   */
  let unaccounted = 0;

  for (const file of sources("src")) {
    const at = file.split(/[\\/]/).join("/").replace(/^\.\//, "");
    if (at === OWNS_THE_RULE || at.endsWith(".test.ts")) continue;

    const text = readFileSync(file, "utf8");

    /*
     * Comments stripped first.
     *
     * `Clock.svelte`'s note explains why it is **not** `setInterval(1000)` for
     * a clock showing minutes, and the rule read that sentence as a timer. A
     * rule that fails on a file for saying it does not do the thing is a rule
     * somebody switches off.
     */
    const code = text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");
    if (!code.includes("setInterval(")) continue;

    checked += 1;

    /*
     * No exemption for merely mentioning `pollWhileVisible`.
     *
     * That was the first version of this rule and it was worth nothing.
     * Sabotage found it: putting the weather widget's bare `setInterval`
     * back, with its import of the helper left in place, passed. A file
     * that names the right helper and then writes its own timer anyway is
     * exactly the case worth catching.
     *
     * There is no honest exemption to write in its place, either, because
     * a caller of `pollWhileVisible` does not write `setInterval` at all.
     * The helper owns the only one, in `visible.ts`, which is skipped
     * above. Anything else holding a timer is holding it itself.
     */
    if (ALLOWED[at]) continue;

    unaccounted += 1;

    fail(
      at,
      lineOf(text, text.indexOf("setInterval(")),
      "a repeating timer of its own, and no entry here saying what stops it " +
        "when the window is hidden, so it keeps waking the machine for a " +
        "window nobody can see. Poll through `pollWhileVisible` instead, or " +
        "add an entry saying why this one is already accounted for",
    );
  }

  if (checked === 0) {
    fail(
      "scripts/verify-source.mjs",
      null,
      "no repeating timer was found anywhere in `src`, so this rule is " +
        "checking nothing and would stay green through the bug it exists for",
    );
  }

  for (const at of Object.keys(ALLOWED)) {
    if (!existsSync(at)) {
      fail(
        "scripts/verify-source.mjs",
        null,
        `${at} is allowed a repeating timer and no longer exists, so the ` +
          "list is now describing a tree that is not this one",
      );
    }
  }

  // For the published page. Said on every run rather than only when it breaks,
  // the same way the ranking budget prints what it measured, because a count
  // that speaks only on failure cannot show which way it is moving.
  console.log(
    `measurement no-unaccounted-timer :: ${unaccounted} unaccounted for, of ` +
      `${checked} repeating ${checked === 1 ? "timer" : "timers"} in the ` +
      `window :: ${unaccounted === 0}`,
  );
}

/*
 * Every form field an extension can declare is a field the form draws.
 *
 * `FormView.svelte` is a chain of `{:else if}` over tags with nothing at the
 * end of it, which is the shape this project keeps losing days to. What falls
 * off the end here is a field: `Form.FilePicker` was declared by the API layer
 * and drawn by nothing for as long as forms have existed, so an extension
 * asking somebody to choose a file showed them a form with a hole in it and
 * submitted an empty value. Nothing failed and nothing said anything.
 *
 * So the chain is held to the API layer. Every `Form.*` tag the host can put
 * on the wire either has an arm in the form or a line below saying which
 * component reads it instead, and adding a tag without doing one of those
 * fails here rather than on somebody's screen.
 *
 * The exceptions are a list rather than a default, for the same reason: a
 * default lets the next one through in silence, and each of these had to be
 * argued for once.
 */
{
  const COMPONENTS = "host/src/api/components.ts";
  const FORM = "src/lib/components/FormView.svelte";

  /** Tags that are not fields, and what reads them instead. */
  const NOT_A_FIELD = {
    "Form.Dropdown.Item": "read by Form.Dropdown, which draws the options",
    "Form.Dropdown.Section": "read by Form.Dropdown, which flattens its options",
    "Form.TagPicker.Item": "read by Form.TagPicker, which draws the chips",
    "Form.LinkAccessory":
      "a slot on the Form itself rather than a field in it, and Sill draws no " +
      "accessory beside a form's title yet",
  };

  const declared = new Set(
    [
      ...readFileSync(COMPONENTS, "utf8").matchAll(/host<AnyProps>\("(Form\.[^"]+)"\)/g),
    ].map((one) => one[1]),
  );

  if (declared.size < 10) {
    fail(
      COMPONENTS,
      null,
      `only ${declared.size} \`Form.*\` tag(s) found, so this is parsing rather ` +
        "than checking",
    );
  }

  /*
   * The markup only, which is where drawing happens.
   *
   * Sabotage found this. The first version searched the whole file, and the
   * script above names every tag it seeds a default value for, so renaming the
   * arm that draws a file picker left the seeding line behind and this passed
   * while the field vanished from the form. Seeding a value for a field and
   * drawing it are different things, and only one of them is what this rule
   * claims.
   */
  const whole = readFileSync(FORM, "utf8");
  const form = whole.slice(whole.indexOf("</script>"));

  if (!form) {
    fail(FORM, null, "no markup after `</script>`, so this is reading nothing");
  }

  for (const tag of declared) {
    if (form.includes(`field.tag === "${tag}"`)) continue;
    if (NOT_A_FIELD[tag]) continue;
    fail(
      FORM,
      null,
      `\`${tag}\` is a component an extension can declare and no arm here ` +
        "draws it, so a form using it is drawn with that field missing",
    );
  }

  for (const tag of Object.keys(NOT_A_FIELD)) {
    if (declared.has(tag)) continue;
    fail(
      "scripts/verify-source.mjs",
      null,
      `\`${tag}\` is excused from being drawn and ${COMPONENTS} no longer ` +
        "declares it, so this list describes a component set that is not this one",
    );
  }
}

/*
 * The extension file picker hands back paths and never looks at them.
 *
 * `pick_files` is the one door from an extension to a native file dialog, and
 * the reason it is allowed to exist without charging a permission is that it
 * cannot see anything: Windows draws the dialog, somebody chooses, and a path
 * comes back. The moment it reads or lists anything, the argument stops being
 * true and an extension granted nothing has a way to look inside a folder
 * through a form field.
 *
 * Nothing else would catch that. A filesystem call added here compiles, passes
 * every test, and reads as a helpful convenience.
 */
{
  const EXTENSIONS = "src-tauri/src/commands/extensions.rs";
  const text = readFileSync(EXTENSIONS, "utf8");
  const from = text.indexOf("pub(crate) async fn pick_files");

  if (from === -1) {
    fail(EXTENSIONS, null, "no `pick_files`, which is the extension file picker");
  } else {
    // To the next top-level item, so the arms of the match are all inside it
    // and nothing after it is read as part of it.
    const until = text.indexOf("\n#[", from);
    const body = text.slice(from, until === -1 ? text.length : until);

    for (const reaching of [
      "read_dir",
      "read_to_string",
      "fs::read",
      "fs::write",
      "File::open",
      "metadata(",
      "exists(",
    ]) {
      if (!body.includes(reaching)) continue;
      fail(
        EXTENSIONS,
        lineOf(text, from + body.indexOf(reaching)),
        `\`pick_files\` calls \`${reaching}\`, so a picker an extension can open ` +
          "with no permission now looks at the disk itself",
      );
    }
  }
}

/*
 * The window draws the icon names Rust says it draws, and only those.
 *
 * An extension writes `Icon.Cog` and gets back the string "Cog". Which
 * drawing that is, and that it is the same drawing `Icon.Gear` gets, is
 * interpretation of somebody else's vocabulary, so it is decided once in
 * `exthost/icons.rs`. The window still has to hold the same names to draw
 * with, and that is a second list of forty-eight names with nothing making it
 * agree with the first.
 *
 * This is what makes them agree. Four ways a pair like this comes apart, and
 * every one of them fails here:
 *
 * - a name Rust has and the window does not, which is a row drawing a letter
 *   tile while the table says it has a picture;
 * - a name the window has and Rust does not, which is a picture nothing owns;
 * - a name whose mark differs between them, which is the wrong drawing;
 * - a mark named by one side with no arm, or an arm for a mark nothing names,
 *   which is a drawing that can never be reached.
 *
 * The last pair is why the markup is keyed by the mark rather than by the
 * name. `{:else if drawn === "Gear" || drawn === "Cog"}` could say the two are
 * one picture while the table said they were two, and nothing could tell.
 */
{
  const RUST = "src-tauri/src/exthost/icons.rs";
  const DRAWS = "src/lib/components/ExtIcon.svelte";

  const rustText = readFileSync(RUST, "utf8");
  const drawsText = readFileSync(DRAWS, "utf8");

  /** The table itself, so a pair written in a doc comment is not read. */
  const table = rustText.match(/MARKS:\s*&\[\(&str,\s*&str\)\]\s*=\s*&\[([\s\S]*?)\n\];/);

  const inRust = new Map();
  if (!table) {
    fail(RUST, null, "no `MARKS` table, which the window's icon names are held to");
  } else {
    for (const row of table[1].matchAll(/\("([^"]+)",\s*"([^"]+)"\)/g)) {
      inRust.set(row[1], row[2]);
    }
  }

  const window = drawsText.match(/const MARKS: Record<string, string> = \{([\s\S]*?)\n  \};/);

  const inWindow = new Map();
  if (!window) {
    fail(DRAWS, null, "no `MARKS` map, so nothing here is being held to " + RUST);
  } else {
    for (const row of window[1].matchAll(/^\s*([A-Za-z][A-Za-z0-9]*): "([^"]+)",/gm)) {
      inWindow.set(row[1], row[2]);
    }
  }

  /*
   * A parse that found nothing agrees with everything, which is the failure
   * mode every other cross-file rule in this file guards the same way. The
   * floor is far under the real count and only turns a broken regex into a
   * failure rather than into a silent yes.
   */
  for (const [what, found, where] of [
    ["name", inRust, RUST],
    ["name", inWindow, DRAWS],
  ]) {
    if (found.size >= 20) continue;
    fail(where, null, `only ${found.size} icon ${what}(s) found, so this is parsing rather than checking`);
  }

  for (const [name, mark] of inRust) {
    if (!inWindow.has(name)) {
      fail(
        DRAWS,
        null,
        `\`${name}\` has a mark in ${RUST} and none here, so a row asking for ` +
          "it draws a lettered tile while the table says it has a picture",
      );
    } else if (inWindow.get(name) !== mark) {
      fail(
        DRAWS,
        null,
        `\`${name}\` is "${inWindow.get(name)}" here and "${mark}" in ${RUST}, ` +
          "so the window draws a different icon from the one Rust names",
      );
    }
  }

  for (const name of inWindow.keys()) {
    if (inRust.has(name)) continue;
    fail(
      DRAWS,
      null,
      `\`${name}\` is drawn here and is not in ${RUST}, so the window has an ` +
        "icon name of its own and the table has stopped being the table",
    );
  }

  /*
   * One arm draws one mark, and the whole condition is the mark.
   *
   * Sabotage found this: an arm rewritten to `drawn === "clock" || drawn ===
   * "gear"` drew a clock for every name the table folds onto the gear, and the
   * set of marks named by an arm was unchanged, so counting names passed. An
   * arm carrying two marks is the `||` chain this design exists to get rid of,
   * because it can say two names are one picture while the table says they are
   * two and nothing can see the difference.
   */
  for (const arm of drawsText.matchAll(/\{[:#](?:else )?if ([^}]*drawn ===[^}]*)\}/g)) {
    if (/^drawn === "[a-z][a-z-]*"$/.test(arm[1].trim())) continue;
    fail(
      DRAWS,
      lineOf(drawsText, arm.index),
      `\`${arm[1].trim()}\` draws on more than one mark, so which picture a ` +
        "name gets is decided here rather than in " + RUST,
    );
  }

  // Every mark either side names, against the arms that draw one.
  const wanted = new Set([...inRust.values(), ...inWindow.values()]);
  const arms = new Set(
    [...drawsText.matchAll(/drawn === "([a-z][a-z-]*)"/g)].map((one) => one[1]),
  );

  for (const mark of wanted) {
    if (arms.has(mark)) continue;
    fail(
      DRAWS,
      null,
      `the mark "${mark}" is named and nothing draws it, so every name folded ` +
        "onto it falls through to the lettered tile",
    );
  }

  for (const mark of arms) {
    if (wanted.has(mark)) continue;
    fail(
      DRAWS,
      null,
      `an arm draws the mark "${mark}" and no name resolves to it, so that ` +
        "drawing is unreachable",
    );
  }
}

console.log(
  failures === 0 ? "source verification passed" : `\n${failures} problem(s) found`,
);
process.exit(failures === 0 ? 0 : 1);
