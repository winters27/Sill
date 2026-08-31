import { describe, expect, it } from "vitest";
import { blocksOf, spansOf, type Block, type Span } from "./markdown";

/** The text a tree holds, for asserting on shape without spelling out spans. */
function textOf(spans: Span[]): string {
  return spans
    .map((span) => (span.kind === "text" || span.kind === "code" ? span.text : textOf(span.spans)))
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
