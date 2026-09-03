/**
 * The one empty, loading and failed state.
 *
 * These hold the property the eleven hand-written `{#if}` chains did not:
 * exactly one state is ever drawn, and which one is decided in a single place.
 * `ActionPanel` drew two at once for months and nothing failed, because
 * `actions.length === 0` is true in both of the two branches that tested it.
 */
import { describe, expect, test } from "vitest";
import { couldNot, noMatch, standing, whileEmpty, type Reading } from "$lib/instead";

const reading = (over: Partial<Reading> = {}): Reading => ({
  failed: false,
  loading: false,
  count: 0,
  ...over,
});

describe("which state a view is in", () => {
  test("rows on screen are rows on screen", () => {
    expect(standing(reading({ count: 3 }))).toBe("content");
  });

  test("no rows and nothing happening is empty", () => {
    expect(standing(reading())).toBe("empty");
  });

  test("a read in flight with nothing yet drawn says so", () => {
    expect(standing(reading({ loading: true }))).toBe("loading");
  });

  /*
   * The half of the rule that stops a list flickering.
   *
   * A late page of files arrives on nearly every search, and replacing rows
   * somebody is already reading with "Reading..." to say more is coming would
   * make every query blink. A late page is an append; the list grows under it.
   */
  test("a read in flight over rows already drawn leaves them alone", () => {
    expect(standing(reading({ loading: true, count: 3 }))).toBe("content");
  });

  test("a failure wins over an empty list", () => {
    expect(standing(reading({ failed: true }))).toBe("failed");
  });

  /*
   * A view clears its failure when the retry succeeds rather than when it
   * starts, so this is the state a retry is in the whole time it runs. Saying
   * "Reading..." and then showing the same error again has told the reader
   * nothing twice.
   */
  test("a failure survives the retry that has not answered yet", () => {
    expect(standing(reading({ failed: true, loading: true }))).toBe("failed");
  });

  /*
   * The failure a list can be in while it still holds the last good answer.
   * The rows are stale and the reader has to be told, so the failure wins over
   * drawing them as if they were current.
   */
  test("a failure wins over rows left from before it", () => {
    expect(standing(reading({ failed: true, count: 3 }))).toBe("failed");
  });

  /*
   * The property, stated directly rather than inferred from the cases above:
   * one call cannot answer two things, which is what makes drawing two states
   * at once impossible rather than merely unlikely.
   */
  test("every combination answers exactly one of the four", () => {
    const answers = new Set<string>();

    for (const failed of [false, true]) {
      for (const loading of [false, true]) {
        for (const count of [0, 1, 400]) {
          const answer = standing({ failed, loading, count });
          expect(["failed", "loading", "empty", "content"]).toContain(answer);
          answers.add(answer);
        }
      }
    }

    // And all four are reachable, so none of them is dead code.
    expect(answers.size).toBe(4);
  });
});

describe("what the state says", () => {
  test("a filtered list names what was typed", () => {
    expect(noMatch("tada")).toBe("No results for tada");
  });

  test("what is being looked through can be named", () => {
    expect(noMatch("tada", "snippets")).toBe("No snippets for tada");
  });

  /*
   * The field is almost never actually empty in the launcher, but a filter box
   * is, and "No results for " with nothing after it reads as a bug.
   */
  test("an empty filter does not leave a sentence hanging", () => {
    expect(noMatch("   ")).toBe("No results");
  });

  /*
   * The house rule for a failure, held here because it is the one place the
   * sentence is built: it names what the person was trying to do, never the
   * command that refused. "Sill could not clipboard_search" is a message
   * nobody can act on.
   */
  test("a failure is a sentence about the person's errand", () => {
    expect(couldNot("read your clipboard history")).toBe(
      "Sill could not read your clipboard history",
    );
  });
});

/**
 * The empty state of a list an extension rendered.
 *
 * Its own describe because it is the one place `isLoading` is turned into
 * words. That prop was read nowhere, so a command still fetching its first
 * page said "No results", which is a statement about somebody's data made
 * before the answer arrived.
 */
describe("what an extension's own list says when it is empty", () => {
  const words = { headline: "No results", hint: "This command found nothing to show." };

  test("still fetching is not the same as nothing found", () => {
    const said = whileEmpty(reading({ loading: true }), "", words);
    expect(said.tone).toBe("loading");
    expect(said.headline).toBe("Still looking");
  });

  /*
   * Loading only wins while the screen is blank. A later page arriving over
   * rows somebody is already reading is an append, and replacing them with a
   * status line to say more is coming is worse than letting the list grow.
   */
  test("still fetching over rows already drawn says nothing at all", () => {
    expect(whileEmpty(reading({ loading: true, count: 4 }), "", words).tone).toBe("content");
  });

  test("a filter that matched nothing names the word that emptied it", () => {
    const said = whileEmpty(reading(), "tada", words);
    expect(said.tone).toBe("empty");
    expect(said.headline).toBe("No results for tada");
  });

  /*
   * A command that genuinely returned nothing gets the caller's words, because
   * an empty list and an empty grid are not the same sentence.
   */
  test("a command that returned nothing gets the view's own words", () => {
    expect(whileEmpty(reading(), "", words)).toMatchObject(words);
    expect(whileEmpty(reading(), "", { headline: "Nothing to show", hint: "" }).headline).toBe(
      "Nothing to show",
    );
  });

  /*
   * The order matters as much as the branches. A command that is fetching AND
   * has something typed in the field is fetching: saying "No results for tada"
   * before the answer is back is the lie this whole function exists to stop.
   */
  test("fetching beats a query that has not been answered yet", () => {
    expect(whileEmpty(reading({ loading: true }), "tada", words).headline).toBe("Still looking");
  });
});
