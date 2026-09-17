import { describe, expect, it } from "vitest";
import { blocksOf, spansOf, type Block, type Span } from "./markdown";

/** The text a tree holds, for asserting on shape without spelling out spans. */
function textOf(spans: Span[]): string {
  return spans
    .map((span) => {
      if (span.kind === "text" || span.kind === "code") return span.text;
      // What an image puts into the document is its alt text. The address is
      // an attribute, and several tests below are about it staying one.
      if (span.kind === "image") return span.alt;
      return textOf(span.spans);
    })
    .join("");
}

function kinds(blocks: Block[]): string[] {
  return blocks.map((block) => block.kind);
}

describe("blocks", () => {
  it("reads a plain answer as one paragraph", () => {
    const blocks = blocksOf("Red, blue and yellow.");
    expect(kinds(blocks)).toEqual(["paragraph"]);
  });

  it("keeps separate paragraphs separate", () => {
    expect(kinds(blocksOf("One.\n\nTwo."))).toEqual(["paragraph", "paragraph"]);
  });

  it("joins the lines of one paragraph", () => {
    const [block] = blocksOf("a sentence\nthat wrapped");
    expect(block.kind).toBe("paragraph");
    if (block.kind !== "paragraph") return;
    expect(textOf(block.spans)).toBe("a sentence\nthat wrapped");
  });

  it("reads headings by their level", () => {
    const blocks = blocksOf("# One\n## Two\n###### Six");
    expect(blocks.map((b) => (b.kind === "heading" ? b.level : 0))).toEqual([1, 2, 6]);
  });

  it("does not read a hash inside a word as a heading", () => {
    expect(kinds(blocksOf("issue #42 is open"))).toEqual(["paragraph"]);
  });

  it("reads a rule", () => {
    expect(kinds(blocksOf("---"))).toEqual(["rule"]);
    expect(kinds(blocksOf("***"))).toEqual(["rule"]);
  });

  it("reads a quote, and what is inside it", () => {
    const [block] = blocksOf("> **careful**\n> this deletes things");
    expect(block.kind).toBe("quote");
    if (block.kind !== "quote") return;
    expect(kinds(block.blocks)).toEqual(["paragraph"]);
  });
});

describe("code", () => {
  it("keeps a fenced block whole, with its language", () => {
    const [block] = blocksOf("```rust\nfn main() {}\n```");
    expect(block).toEqual({ kind: "code", language: "rust", text: "fn main() {}" });
  });

  it("takes no language when none was given", () => {
    const [block] = blocksOf("```\nplain\n```");
    expect(block.kind === "code" && block.language).toBe("");
  });

  /*
   * The one that matters most. Everything inside a fence is text, so a shell
   * comment is not a heading and a glob is not emphasis. Getting this wrong
   * turns the rest of an answer bold.
   */
  it("reads nothing inside a fence as markup", () => {
    const [block] = blocksOf("```sh\n# not a heading\nrm *.log\n**not bold**\n```");
    expect(block.kind).toBe("code");
    if (block.kind !== "code") return;
    expect(block.text).toBe("# not a heading\nrm *.log\n**not bold**");
  });

  it("survives a fence the answer never closed", () => {
    const [block] = blocksOf("```\nstill arriving");
    expect(block).toEqual({ kind: "code", language: "", text: "still arriving" });
  });

  it("keeps blank lines inside a fence but not around it", () => {
    const [block] = blocksOf("```\n\none\n\ntwo\n\n```");
    expect(block.kind === "code" && block.text).toBe("one\n\ntwo");
  });

  it("accepts tildes as well as backticks", () => {
    const [block] = blocksOf("~~~js\nlet a = 1;\n~~~");
    expect(block.kind === "code" && block.text).toBe("let a = 1;");
  });
});

describe("lists", () => {
  it("reads a bulleted list", () => {
    const [block] = blocksOf("- one\n- two\n- three");
    expect(block.kind).toBe("list");
    if (block.kind !== "list") return;
    expect(block.ordered).toBe(false);
    expect(block.items.map((item) => textOf(item.spans))).toEqual(["one", "two", "three"]);
  });

  it("reads a numbered list, and where it starts", () => {
    const [block] = blocksOf("3. three\n4. four");
    expect(block.kind).toBe("list");
    if (block.kind !== "list") return;
    expect(block.ordered).toBe(true);
    expect(block.start).toBe(3);
  });

  it("counts how deep each item is", () => {
    const [block] = blocksOf("- one\n  - under one\n    - deeper\n- two");
    expect(block.kind).toBe("list");
    if (block.kind !== "list") return;
    expect(block.items.map((item) => item.depth)).toEqual([0, 1, 2, 0]);
  });

  /** Models put a blank line between items constantly. */
  it("does not end a list at a blank line between its items", () => {
    const [block] = blocksOf("- one\n\n- two");
    expect(block.kind).toBe("list");
    if (block.kind !== "list") return;
    expect(block.items).toHaveLength(2);
  });

  it("ends a list when the answer moves on", () => {
    const blocks = blocksOf("- one\n- two\n\nAnd then this.");
    expect(kinds(blocks)).toEqual(["list", "paragraph"]);
  });

  it("reads a checkbox as one", () => {
    const [block] = blocksOf("- [x] done\n- [ ] not done\n- neither");
    expect(block.kind).toBe("list");
    if (block.kind !== "list") return;
    expect(block.items.map((item) => item.done)).toEqual([true, false, null]);
    expect(textOf(block.items[0].spans)).toBe("done");
  });

  it("reads marks inside an item", () => {
    const [block] = blocksOf("- run `ls` to **look**");
    expect(block.kind).toBe("list");
    if (block.kind !== "list") return;
    expect(block.items[0].spans.map((span) => span.kind)).toEqual([
      "text",
      "code",
      "text",
      "strong",
    ]);
  });
});

describe("tables", () => {
  it("reads a table with its heading row", () => {
    const [block] = blocksOf("| a | b |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |");
    expect(block.kind).toBe("table");
    if (block.kind !== "table") return;
    expect(block.head.map(textOf)).toEqual(["a", "b"]);
    expect(block.rows).toHaveLength(2);
    expect(block.rows[1].map(textOf)).toEqual(["3", "4"]);
  });

  /** Without the dashes under it, a line with pipes is a line with pipes. */
  it("does not read a sentence containing a pipe as a table", () => {
    expect(kinds(blocksOf("use grep | sort to chain them"))).toEqual(["paragraph"]);
  });

  it("takes an escaped pipe as content", () => {
    const [block] = blocksOf("| what |\n| --- |\n| a \\| b |");
    expect(block.kind).toBe("table");
    if (block.kind !== "table") return;
    expect(textOf(block.rows[0][0])).toBe("a | b");
  });
});

describe("inline", () => {
  it("reads bold, italic and struck text", () => {
    expect(spansOf("**b**").map((s) => s.kind)).toEqual(["strong"]);
    expect(spansOf("*i*").map((s) => s.kind)).toEqual(["em"]);
    expect(spansOf("~~gone~~").map((s) => s.kind)).toEqual(["strike"]);
  });

  it("does not read two asterisks as two lots of one", () => {
    const spans = spansOf("**both**");
    expect(spans).toHaveLength(1);
    expect(spans[0].kind).toBe("strong");
    expect(textOf(spans)).toBe("both");
  });

  /*
   * Code is opaque. Whatever is between the backticks is text, including the
   * asterisks that would otherwise be emphasis, and an identifier holding an
   * underscore is an identifier.
   */
  it("reads nothing inside inline code as markup", () => {
    const spans = spansOf("call `a ** b` now");
    expect(spans.map((s) => s.kind)).toEqual(["text", "code", "text"]);
    expect(spans[1].kind === "code" && spans[1].text).toBe("a ** b");
  });

  it("leaves an underscore inside a word alone", () => {
    expect(spansOf("read_file and write_file").map((s) => s.kind)).toEqual(["text"]);
  });

  it("reads a link", () => {
    const [span] = spansOf("[the docs](https://example.com/x)");
    expect(span).toEqual({
      kind: "link",
      href: "https://example.com/x",
      spans: [{ kind: "text", text: "the docs" }],
    });
  });

  /*
   * Nothing here ever becomes HTML, so this is not the last line of defence.
   * It is the first: a link that cannot be followed anywhere useful should not
   * look like a link somebody can follow.
   */
  it("refuses a scheme that is not a place", () => {
    const spans = spansOf("[click](javascript:alert(1))");
    expect(spans.every((span) => span.kind === "text")).toBe(true);
  });

  it("reads marks inside a link", () => {
    const [span] = spansOf("[**bold** link](https://example.com)");
    expect(span.kind).toBe("link");
    if (span.kind !== "link") return;
    expect(span.spans[0].kind).toBe("strong");
  });

  it("leaves an unclosed mark as text", () => {
    expect(spansOf("**still typing").map((s) => s.kind)).toEqual(["text"]);
    expect(spansOf("half a `code").map((s) => s.kind)).toEqual(["text"]);
  });

  it("reads inline code holding a backtick", () => {
    const spans = spansOf("write ``a ` b`` here");
    expect(spans[1].kind === "code" && spans[1].text).toBe("a ` b");
  });

  it("nests emphasis inside strength", () => {
    const [span] = spansOf("***both***");
    expect(span.kind).toBe("strong");
    if (span.kind !== "strong") return;
    expect(span.spans[0].kind).toBe("em");
    expect(textOf([span])).toBe("both");
  });

  it("takes the earliest mark, not the first kind checked", () => {
    const spans = spansOf("*first* then `second`");
    expect(spans.map((s) => s.kind)).toEqual(["em", "text", "code"]);
  });
});

/*
 * A picture is the one node whose address is loaded rather than followed, and
 * every test here is about that difference. The extension that prompted them
 * is Speedtest, which draws its charts as SVG data URIs and had been printing
 * forty kilobytes of base64 into the pane as prose.
 */
describe("images", () => {
  it("reads a data URI picture as a picture", () => {
    const [span] = spansOf("![a chart](data:image/png;base64,iVBORw0KGgo=)");
    expect(span).toEqual({
      kind: "image",
      href: "data:image/png;base64,iVBORw0KGgo=",
      alt: "a chart",
    });
  });

  it("reads a picture with no alt text", () => {
    const [span] = spansOf("![](data:image/gif;base64,R0lGOD)");
    expect(span).toEqual({ kind: "image", href: "data:image/gif;base64,R0lGOD", alt: "" });
  });

  it("reads the media type whatever case it is written in", () => {
    expect(spansOf("![x](DATA:IMAGE/PNG;base64,iVBOR)")[0].kind).toBe("image");
  });

  /*
   * `?raycast-width=` is not a query string. A data URL has no query
   * component, so the suffix is part of the payload and is not in the base64
   * alphabet: left on, the picture does not decode and draws as broken. This
   * is the whole of the difference between fixing Speedtest and appearing to.
   */
  it("takes the size Raycast writes on the end off the address", () => {
    const [span] = spansOf("![gauge](data:image/svg+xml;base64,PHN2Zy8+?raycast-width=320)");
    expect(span).toEqual({
      kind: "image",
      href: "data:image/svg+xml;base64,PHN2Zy8+",
      alt: "gauge",
      width: 320,
    });
  });

  it("reads both sizes when both are written", () => {
    const [span] = spansOf("![x](data:image/png;base64,iVBOR?raycast-width=100&raycast-height=50)");
    expect(span).toEqual({
      kind: "image",
      href: "data:image/png;base64,iVBOR",
      alt: "x",
      width: 100,
      height: 50,
    });
  });

  /** A `?` the document meant is payload, and payload is not touched. */
  it("keeps a question mark that is not a size", () => {
    const [span] = spansOf("![x](data:image/svg+xml,%3Csvg?%3E)");
    expect(span).toEqual({ kind: "image", href: "data:image/svg+xml,%3Csvg?%3E", alt: "x" });
  });

  /*
   * `data:` generally also spells `data:text/html`, which is a document rather
   * than a picture. It must not become an image, and it must not fall through
   * to the link branch either, which would put a live address on screen.
   */
  it("refuses data:text/html, and does not fall back to a link", () => {
    const spans = spansOf("![page](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)");
    expect(spans.map((span) => span.kind)).toEqual(["text"]);
    expect(textOf(spans)).toBe("page");
  });

  it("refuses javascript:, as it does for a link", () => {
    const spans = spansOf("![go](javascript:alert)");
    expect(spans.every((span) => span.kind === "text")).toBe(true);
    expect(textOf(spans)).toBe("go");
  });

  /*
   * Deliberately tighter than the link above, which allows `https:`. A link is
   * followed by somebody who chose to; a picture is fetched by the act of
   * drawing the words, and this module also draws answers a model wrote.
   */
  it("refuses a remote address that the same link is allowed", () => {
    const spans = spansOf("![a cat](https://example.com/cat.png)");
    expect(spans.every((span) => span.kind === "text")).toBe(true);
    expect(textOf(spans)).toBe("a cat");

    expect(spansOf("[a cat](https://example.com/cat.png)")[0].kind).toBe("link");
  });

  /*
   * The bug this whole node exists for. Falling back to the plain text a mark
   * arrived as is right everywhere else in this module and is exactly wrong
   * here, because the text is the address and the address is a page of base64.
   */
  it("does not put a refused address into the document", () => {
    const blob = "data:text/html;base64," + "QUJD".repeat(500);
    const spans = spansOf(`![a chart](${blob})`);

    expect(textOf(spans)).toBe("a chart");
    expect(textOf(spans)).not.toContain("QUJD");
    expect(textOf(spans).length).toBeLessThan(20);
  });

  /** Nothing to draw and nothing said about it, so nothing is put on screen. */
  it("draws nothing for a refused picture with no alt text", () => {
    expect(textOf(spansOf("![](https://example.com/x.png)"))).toBe("");
  });

  it("does not read an exclamation mark that ends a sentence as a picture", () => {
    expect(spansOf("Wow! [docs](https://example.com)").map((span) => span.kind)).toEqual([
      "text",
      "link",
    ]);
  });

  /*
   * A wrapped list item is rejoined by unparsing its spans back to source and
   * parsing the result again. An image holds no spans, so the unparser has to
   * know about it or this throws rather than failing.
   */
  it("survives a picture in a list item that wraps", () => {
    const [block] = blocksOf("- ![c](data:image/png;base64,iVBOR)\n  and a caption");
    expect(block.kind).toBe("list");
    if (block.kind !== "list") return;
    expect(block.items[0].spans.map((span) => span.kind)).toEqual(["image", "text"]);
  });

  it("reads the line the Speedtest extension actually writes", () => {
    const [block] = blocksOf(
      "![speedtest-download-52-0-17.855](data:image/svg+xml;base64,PHN2Zy8+?raycast-width=560)",
    );

    expect(block.kind).toBe("paragraph");
    if (block.kind !== "paragraph") return;
    expect(block.spans).toEqual([
      {
        kind: "image",
        href: "data:image/svg+xml;base64,PHN2Zy8+",
        alt: "speedtest-download-52-0-17.855",
        width: 560,
      },
    ]);
  });
});

describe("what a model actually sends", () => {
  it("reads the answer that started all this", () => {
    const blocks = blocksOf(
      "Here are three:\n\n" +
        "- `ls` to list directory contents\n" +
        "- `grep` to search text in files\n" +
        "- `echo` to print text\n\n" +
        "```sh\nls -la\n```\n",
    );

    expect(kinds(blocks)).toEqual(["paragraph", "list", "code"]);

    const list = blocks[1];
    expect(list.kind).toBe("list");
    if (list.kind !== "list") return;
    expect(list.items[0].spans[0].kind).toBe("code");
  });

  it("holds up to a whole document", () => {
    const blocks = blocksOf(
      "# Title\n\nSome **text**.\n\n## Steps\n\n1. First\n2. Second\n\n" +
        "> A note.\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n---\n\nDone.",
    );

    expect(kinds(blocks)).toEqual([
      "heading",
      "paragraph",
      "heading",
      "list",
      "quote",
      "table",
      "rule",
      "paragraph",
    ]);
  });

  it("returns nothing for nothing", () => {
    expect(blocksOf("")).toEqual([]);
    expect(blocksOf("   \n\n  ")).toEqual([]);
  });

  /*
   * Every prefix of an answer is parsed, because an answer is drawn while it
   * is arriving. None of them may throw: a parser that panics half way through
   * a code fence takes the whole conversation with it.
   */
  it("parses every prefix of an answer without throwing", () => {
    const whole =
      "# Heading\n\n- one\n- two\n\n```js\nconst a = [1, 2];\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n**end**";

    for (let at = 0; at <= whole.length; at += 1) {
      expect(() => blocksOf(whole.slice(0, at))).not.toThrow();
    }
  });
});
