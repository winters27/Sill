/**
 * Markdown, as far as a model actually writes it.
 *
 * Parsed to a tree here and drawn by `Markdown.svelte`, which never puts a
 * string into the document as HTML. That is the whole reason this exists
 * rather than a library plus a sanitiser: an answer is written by a model that
 * has read something, and something it has read can be trying to write the
 * markup. A tree of typed nodes cannot carry a script tag, because there is no
 * node for one.
 *
 * The subset is deliberate. Headings, paragraphs, fenced code, lists, quotes,
 * rules, tables and images cover what these services emit; footnotes,
 * definition lists and raw HTML do not appear and are left as the plain text
 * they arrived as.
 *
 * An image is the only node whose address is *loaded* rather than followed, so
 * it is the only one whose allow-list is not a question about where a person
 * is sent. See [`pictureOf`], which is where that difference is written down.
 */

/** One run of inline text. */
export type Span =
  | { kind: "text"; text: string }
  | { kind: "code"; text: string }
  | { kind: "strong"; spans: Span[] }
  | { kind: "em"; spans: Span[] }
  | { kind: "strike"; spans: Span[] }
  | { kind: "link"; href: string; spans: Span[] }
  | {
      kind: "image";
      href: string;
      /** Kept even when empty: an empty alt is a deliberate "this says nothing". */
      alt: string;
      /** What the document asked for, which is not what it gets. See `pictureOf`. */
      width?: number;
      height?: number;
    };

/** An address a picture may be drawn from, with the size that was asked for. */
interface Picture {
  href: string;
  width?: number;
  height?: number;
}

/** One thing that stands on its own. */
export type Block =
  | { kind: "paragraph"; spans: Span[] }
  | { kind: "heading"; level: number; spans: Span[] }
  | { kind: "code"; language: string; text: string }
  | { kind: "list"; ordered: boolean; start: number; items: ListItem[] }
  | { kind: "quote"; blocks: Block[] }
  | { kind: "rule" }
  | { kind: "table"; head: Span[][]; rows: Span[][][] };

export interface ListItem {
  spans: Span[];
  /** How deep it is nested, counted in levels rather than spaces. */
  depth: number;
  /** `null` when it is not a checkbox at all. */
  done: boolean | null;
}

/** Two spaces of indent, or a tab, is one level. */
const INDENT = 2;

const FENCE = /^(\s*)(`{3,}|~{3,})\s*([^\s`]*)/;
const HEADING = /^(#{1,6})\s+(.*)$/;
const RULE = /^\s{0,3}([-*_])(\s*\1){2,}\s*$/;
const QUOTE = /^\s{0,3}>\s?(.*)$/;
const BULLET = /^(\s*)([-*+])\s+(.*)$/;
const NUMBER = /^(\s*)(\d{1,9})[.)]\s+(.*)$/;
const CHECK = /^\[([ xX])\]\s+(.*)$/;
const DIVIDER = /^\s*\|?[\s:-]*-[\s:|-]*\|?\s*$/;

/**
 * The whole answer, as blocks.
 *
 * Line based, because every block this handles is decided by how a line
 * starts. The one exception is a fenced code block, whose lines are taken
 * whole and never looked at again, which is what stops a `#` in a shell
 * comment becoming a heading.
 */
export function blocksOf(markdown: string): Block[] {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];

  let at = 0;

  while (at < lines.length) {
    const line = lines[at];

    if (!line.trim()) {
      at += 1;
      continue;
    }

    // Code first, so nothing inside a fence is read as anything else.
    const fence = FENCE.exec(line);
    if (fence) {
      const [, , marks, language] = fence;
      const body: string[] = [];
      at += 1;

      while (at < lines.length && !lines[at].trim().startsWith(marks[0].repeat(3))) {
        body.push(lines[at]);
        at += 1;
      }

      // A fence the answer never closed. Everything after it is still code:
      // that is what the writer meant, and it stops a half-arrived stream
      // flickering between code and prose as it lands.
      if (at < lines.length) at += 1;

      blocks.push({ kind: "code", language, text: trimBlankEdges(body).join("\n") });
      continue;
    }

    if (RULE.test(line)) {
      blocks.push({ kind: "rule" });
      at += 1;
      continue;
    }

    const heading = HEADING.exec(line.trimStart());
    if (heading) {
      blocks.push({
        kind: "heading",
        level: heading[1].length,
        spans: spansOf(heading[2].replace(/\s+#+\s*$/, "")),
      });
      at += 1;
      continue;
    }

    if (QUOTE.test(line)) {
      const quoted: string[] = [];
      while (at < lines.length && QUOTE.test(lines[at])) {
        quoted.push(QUOTE.exec(lines[at])![1]);
        at += 1;
      }
      blocks.push({ kind: "quote", blocks: blocksOf(quoted.join("\n")) });
      continue;
    }

    if (BULLET.test(line) || NUMBER.test(line)) {
      const [list, next] = listAt(lines, at);
      blocks.push(list);
      at = next;
      continue;
    }

    // A table is a header row and a row of dashes under it. Without the
    // second, the first is an ordinary paragraph that happens to hold pipes.
    if (line.includes("|") && at + 1 < lines.length && DIVIDER.test(lines[at + 1])) {
      const [table, next] = tableAt(lines, at);
      blocks.push(table);
      at = next;
      continue;
    }

    const paragraph: string[] = [];
    while (at < lines.length && lines[at].trim() && !startsSomethingElse(lines, at)) {
      paragraph.push(lines[at].trim());
      at += 1;
    }

    /*
     * A line that begins a block nothing above consumed.
     *
     * It cannot happen while the branches above and `startsSomethingElse`
     * agree, and that is exactly why it is guarded: they are two lists
     * that have to match, with nothing making them. If they ever disagree
     * the paragraph loop takes no lines, `at` does not move, and the
     * parser spins forever on one line, taking the window with it. Losing
     * the markup on one line is something a person can see and report. A
     * hang is not.
     */
    if (paragraph.length === 0) {
      blocks.push({ kind: "paragraph", spans: [{ kind: "text", text: lines[at].trim() }] });
      at += 1;
      continue;
    }

    // A line ending in two spaces is a hard break, which is how a model writes
    // an address or a list of names it does not want run together.
    blocks.push({ kind: "paragraph", spans: spansOf(paragraph.join("\n")) });
  }

  return blocks;
}

/** Whether this line begins a block, so a paragraph must stop before it. */
function startsSomethingElse(lines: string[], at: number): boolean {
  const line = lines[at];

  return (
    FENCE.test(line) ||
    RULE.test(line) ||
    HEADING.test(line.trimStart()) ||
    QUOTE.test(line) ||
    BULLET.test(line) ||
    NUMBER.test(line) ||
    (line.includes("|") && at + 1 < lines.length && DIVIDER.test(lines[at + 1]))
  );
}

function trimBlankEdges(lines: string[]): string[] {
  const out = [...lines];
  while (out.length && !out[0].trim()) out.shift();
  while (out.length && !out[out.length - 1].trim()) out.pop();
  return out;
}

/**
 * One list, however deep it goes.
 *
 * Depth is carried on each item rather than nesting the structure. A launcher
 * shows an answer, it does not let you edit one, and a flat list with a depth
 * on each row is drawn with one indent rule and read without recursion.
 */
function listAt(lines: string[], from: number): [Block, number] {
  const first = BULLET.exec(lines[from]) ?? NUMBER.exec(lines[from])!;
  const ordered = !BULLET.test(lines[from]);
  const start = ordered ? Number(first[2]) : 1;

  const items: ListItem[] = [];
  let at = from;

  while (at < lines.length) {
    const line = lines[at];

    if (!line.trim()) {
      // A blank line inside a list only ends it if what follows is not another
      // item. Models put blank lines between items constantly.
      const next = lines[at + 1];
      if (next && (BULLET.test(next) || NUMBER.test(next))) {
        at += 1;
        continue;
      }
      break;
    }

    const found = BULLET.exec(line) ?? NUMBER.exec(line);
    if (!found) {
      // An indented continuation of the item above, rather than a new one.
      if (items.length && /^\s{2,}/.test(line)) {
        const last = items[items.length - 1];
        last.spans = spansOf(textOf(last.spans) + "\n" + line.trim());
        at += 1;
        continue;
      }
      break;
    }

    const [, indent, , rest] = found;
    const box = CHECK.exec(rest);

    items.push({
      spans: spansOf(box ? box[2] : rest),
      depth: Math.floor(indent.replace(/\t/g, "  ").length / INDENT),
      done: box ? box[1].toLowerCase() === "x" : null,
    });

    at += 1;
  }

  return [{ kind: "list", ordered, start, items }, at];
}

/** The text a run of spans holds, for rejoining a wrapped list item. */
function textOf(spans: Span[]): string {
  return spans
    .map((span) => {
      if (span.kind === "text") return span.text;
      if (span.kind === "code") return "`" + span.text + "`";
      // Back to the source it was read from, because this is the *unparse*
      // half of a round trip: the caller appends a wrapped line and parses the
      // result again. An image holds no spans to recurse into.
      if (span.kind === "image") return "![" + span.alt + "](" + span.href + ")";
      return textOf(span.spans);
    })
    .join("");
}

function tableAt(lines: string[], from: number): [Block, number] {
  const cellsOf = (line: string): Span[][] =>
    line
      .trim()
      .replace(/^\|/, "")
      .replace(/\|$/, "")
      .split(/(?<!\\)\|/)
      .map((cell) => spansOf(cell.trim().replace(/\\\|/g, "|")));

  const head = cellsOf(lines[from]);
  const rows: Span[][][] = [];
  let at = from + 2;

  while (at < lines.length && lines[at].includes("|") && lines[at].trim()) {
    rows.push(cellsOf(lines[at]));
    at += 1;
  }

  return [{ kind: "table", head, rows }, at];
}

/**
 * One line of text, as spans.
 *
 * Code first and unconditionally, because a backtick run is opaque: whatever
 * is inside it is text, including the asterisks that would otherwise be
 * emphasis. Getting that order wrong is how `**` inside a code sample turns
 * the rest of an answer bold.
 */
export function spansOf(text: string): Span[] {
  const spans: Span[] = [];
  let rest = text;

  while (rest) {
    const found = firstMark(rest);

    if (!found) {
      spans.push({ kind: "text", text: rest });
      break;
    }

    if (found.at > 0) {
      spans.push({ kind: "text", text: rest.slice(0, found.at) });
    }

    spans.push(found.span);
    rest = rest.slice(found.at + found.length);
  }

  return spans;
}

interface Found {
  at: number;
  length: number;
  span: Span;
}

/** The earliest mark in the line, and what it makes. */
function firstMark(text: string): Found | null {
  const tried: (Found | null)[] = [
    codeMark(text),
    linkMark(text),
    wrapMark(text, "***", "strong-em"),
    wrapMark(text, "**", "strong"),
    wrapMark(text, "__", "strong"),
    wrapMark(text, "~~", "strike"),
    wrapMark(text, "*", "em"),
    wrapMark(text, "_", "em"),
  ];

  const marks = tried.filter((mark): mark is Found => mark !== null);

  if (marks.length === 0) return null;

  return marks.reduce((best, mark) => {
    if (mark.at !== best.at) return mark.at < best.at ? mark : best;
    // Same place: the longer opener wins, so `**` is never read as two `*`.
    return mark.length > best.length ? mark : best;
  });
}

function codeMark(text: string): Found | null {
  const open = text.indexOf("`");
  if (open < 0) return null;

  // The opener may be several backticks, which is how a sample containing a
  // backtick is written.
  let ticks = 0;
  while (text[open + ticks] === "`") ticks += 1;

  const marks = "`".repeat(ticks);
  const close = text.indexOf(marks, open + ticks);
  if (close < 0) return null;

  return {
    at: open,
    length: close + ticks - open,
    span: { kind: "code", text: text.slice(open + ticks, close).trim() },
  };
}

/**
 * A link, or a picture.
 *
 * One matcher for both, because they are one syntax with a mark in front of
 * it. Two would be a ninth regex tried against every inline scan, and that
 * scan already runs against every prefix of an answer as it arrives.
 */
function linkMark(text: string): Found | null {
  const found = /(!?)\[([^\]]*)\]\(([^\s)]+)(?:\s+"[^"]*")?\)/.exec(text);
  if (!found || found.index === undefined) return null;

  const [whole, bang, label, href] = found;
  const at = found.index;

  if (bang) {
    const picture = pictureOf(href);

    /*
     * A refused picture is its alt text, and nothing else.
     *
     * Everywhere else here falls back to the plain text a mark arrived as, and
     * that is exactly wrong in the case this node exists for: the address is
     * forty kilobytes of base64, and printing it *is* the bug. An empty alt
     * draws nothing, because a document that said nothing about a picture it
     * cannot show has given this nothing to put on screen.
     */
    return {
      at,
      length: whole.length,
      span: picture ? { kind: "image", alt: label, ...picture } : { kind: "text", text: label },
    };
  }

  // Only schemes that go somewhere a person meant to go. `javascript:` is the
  // reason this is a list of what is allowed rather than a list of what is
  // not, even though nothing here ever becomes HTML.
  if (!/^(https?:|mailto:)/i.test(href)) {
    return { at, length: whole.length, span: { kind: "text", text: whole } };
  }

  return { at, length: whole.length, span: { kind: "link", href, spans: spansOf(label) } };
}

/**
 * An address this window will draw from, or `null`.
 *
 * `data:image/` and nothing else, and the two halves of that are separate
 * decisions.
 *
 * Not `data:` generally, because `data:text/html` is a document and a document
 * runs script. The media type is the whole of the check that this is a
 * picture.
 *
 * Not `https:` either, and that is the one worth writing down, because the
 * link above allows it and this does not. A link is followed by somebody who
 * chose to; a picture is *fetched by the act of drawing the words*. This
 * module renders an extension's detail pane and an answer a model wrote, and
 * in both the address is content rather than something its author typed: it
 * came from an API response, or from a page the model was asked to read. A
 * remote one is a request nobody asked for, made the moment the text appears,
 * carrying whatever is in its path to whoever is listening for it. A data URI
 * is bytes that already arrived, so drawing one reaches nothing at all.
 *
 * This is deliberately tighter than `exthost/present.ts` is for an icon, where
 * the source is a literal in the extension author's own code.
 */
function pictureOf(href: string): Picture | null {
  const picture = sized(href);

  // Case-insensitive because both halves are: the scheme by RFC 3986 and the
  // media type by RFC 2045, and extensions write `data:image/PNG`.
  return /^data:image\//i.test(picture.href) ? picture : null;
}

/**
 * The address, with the size Raycast writes on the end of it taken off.
 *
 * `?raycast-width=` and `?raycast-height=` are how a Raycast extension asks
 * for a picture at a size, and they are **not a query string**: a data URL has
 * no query component, everything after the comma is payload, and
 * `?raycast-width=320` is not in the base64 alphabet. Left on, the picture
 * does not decode at all, which is a broken image rather than a wall of text
 * and is no better. Every chart the Speedtest extension draws is written this
 * way.
 *
 * Taken off only when the whole of what follows the last `?` is those two
 * parameters, so a `?` that is part of a payload stays part of it.
 */
function sized(href: string): Picture {
  const mark = href.lastIndexOf("?");
  if (mark < 0) return { href };

  const picture: Picture = { href: href.slice(0, mark) };

  for (const part of href.slice(mark + 1).split("&")) {
    const found = /^raycast-(width|height)=(\d{1,5})$/i.exec(part);
    if (!found) return { href };
    picture[found[1].toLowerCase() as "width" | "height"] = Number(found[2]);
  }

  return picture;
}

function wrapMark(text: string, marks: string, kind: string): Found | null {
  const open = text.indexOf(marks);
  if (open < 0) return null;

  // A single `_` inside a word is a word, not emphasis: `read_file` must not
  // become "read" plus italics. The double forms are unambiguous.
  if (marks === "_" && open > 0 && /\w/.test(text[open - 1])) return null;

  const close = text.indexOf(marks, open + marks.length);
  if (close < 0) return null;

  const inner = text.slice(open + marks.length, close);
  if (!inner.trim()) return null;

  const length = close + marks.length - open;

  if (kind === "strong-em") {
    return {
      at: open,
      length,
      span: { kind: "strong", spans: [{ kind: "em", spans: spansOf(inner) }] },
    };
  }

  return {
    at: open,
    length,
    span: { kind: kind as "strong" | "em" | "strike", spans: spansOf(inner) },
  };
}
