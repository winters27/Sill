/**
 * Checks the source tree for damage that compiles.
 *
 * Everything here is a mistake that a compiler, a type checker and a test suite
 * all accept, which is why it needs a pass of its own. Each one has happened.
 */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, extname } from "node:path";
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
 * The keyboard reference does not write a chord down.
 *
 * Every key on that page comes from `keyboard_reference`, which assembles it
 * from the movement preset, the action shortcuts and the summon key. A chord
 * typed into the component is a promise nothing keeps: it survives the key
 * being rebound, and the person reading it has no way to tell it is stale.
 *
 * This project has been bitten four times by a hand-kept list quietly
 * disagreeing with the thing it describes, which is why the rule is a build
 * failure rather than a note.
 */
{
  const SHEET = "src/lib/components/KeySheet.svelte";
  const CHORD = /"(?:Ctrl|Alt|Shift|Cmd|Meta|Super)\+[A-Za-z0-9+]+"/g;

  if (existsSync(SHEET)) {
    const text = readFileSync(SHEET, "utf8");

    text.split("\n").forEach((line, at) => {
      // A comment may name a chord as an example; only code counts.
      if (/^\s*(\/\/|\*|\/\*|<!--)/.test(line)) return;

      for (const found of line.matchAll(CHORD)) {
        fail(
          SHEET,
          at + 1,
          `${found[0]} is written here rather than read from keyboard_reference, ` +
            "so it goes on saying so after the key is rebound",
        );
      }
    });
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

  for (const [region, expected, where] of [
    ["coverage:answered", answered, `${API} and ${UTILS}`],
    ["coverage:refused", refused, UTILS],
    ["coverage:tags", drawn, COMPONENTS],
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

console.log(
  failures === 0 ? "source verification passed" : `\n${failures} problem(s) found`,
);
process.exit(failures === 0 ? 0 : 1);
