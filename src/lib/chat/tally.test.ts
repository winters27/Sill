import { describe, expect, it } from "vitest";

import type { AiReady, AiSpent } from "$lib/exthost/commands";
import { dollars, perSecond, reading, tokens } from "./tally";

function ready(over: Partial<AiReady> = {}): AiReady {
  return {
    ready: true,
    id: "xai",
    name: "xAI Grok",
    model: "grok-4.6",
    kind: "key",
    price: { input: 2, output: 6 },
    whyNot: "",
    ...over,
  };
}

function spent(over: Partial<AiSpent> = {}): AiSpent {
  return { input: 1200, output: 300, cost: 0.0042, unpriced: 0, rate: null, answers: 2, ...over };
}

const idle = { asking: false, streamed: 0, streamBegan: 0, now: 0 };

describe("writing a count in a pill's width", () => {
  it("shortens tokens only once they need it", () => {
    expect(tokens(0)).toBe("0");
    expect(tokens(845)).toBe("845");
    expect(tokens(1000)).toBe("1k");
    expect(tokens(1234)).toBe("1.2k");
    expect(tokens(9990)).toBe("10k");
    expect(tokens(84_500)).toBe("85k");
    expect(tokens(1_250_000)).toBe("1.3M");
  });

  it("gives dollars the places the amount deserves", () => {
    expect(dollars(0)).toBe("$0");
    expect(dollars(0.0004)).toBe("<$0.001");
    expect(dollars(0.0042)).toBe("$0.004");
    expect(dollars(0.25)).toBe("$0.250");
    expect(dollars(1.5)).toBe("$1.50");
    expect(dollars(123.4)).toBe("$123");
  });

  it("says a rate the way somebody would", () => {
    expect(perSecond(4.25)).toBe("4.3 tok/s");
    expect(perSecond(42.6)).toBe("43 tok/s");
  });
});

describe("what the pill reads", () => {
  it("says nothing before there is anything to count", () => {
    expect(reading(null, idle, ready())).toBeNull();
    expect(reading(spent({ answers: 0 }), idle, ready())).toBeNull();
    expect(reading(spent(), idle, null)).toBeNull();
    expect(reading(spent(), idle, ready({ ready: false }))).toBeNull();
    // Asking, but nothing has arrived yet: still nothing to show.
    expect(reading(null, { ...idle, asking: true }, ready())).toBeNull();
  });

  it("reads the total Rust added up, with the price on hover", () => {
    const shown = reading(spent(), idle, ready());
    expect(shown).toMatchObject({ tokens: "1.5k", cost: "$0.004", rate: null, live: false });
    expect(shown?.hint).toBe(
      "1,200 in, 300 out over 2 answers. grok-4.6 costs $2.00 in and $6.00 out per million tokens.",
    );
  });

  it("names the answers it could not price rather than hiding them", () => {
    const shown = reading(spent({ unpriced: 1 }), idle, ready());
    expect(shown?.hint).toContain("1 of them could not be priced");
  });

  it("counts tokens alone for a model nobody priced, and says so", () => {
    const shown = reading(spent({ cost: null }), idle, ready({ model: "grok-9", price: null }));
    expect(shown).toMatchObject({ tokens: "1.5k", cost: null });
    expect(shown?.hint).toContain("No price is known for grok-9");
  });

  it("shows a local model its speed and never a price", () => {
    const local = ready({ id: "ollama", name: "Ollama", model: "qwen3:9b", kind: "local", price: null });
    const shown = reading(spent({ cost: null, rate: 42.6 }), idle, local);
    expect(shown).toMatchObject({ tokens: "1.5k", cost: null, rate: "43 tok/s" });
    expect(shown?.hint).toContain("nothing is spent");
  });

  it("ticks while the answer arrives, priced at the output rate", () => {
    const arriving = { asking: true, streamed: 500, streamBegan: 1000, now: 3000 };
    const shown = reading(spent(), arriving, ready());
    // 1,500 counted plus 500 streamed; $0.0042 plus 500 tokens at $6 a million.
    expect(shown).toMatchObject({ tokens: "2k", cost: "$0.007", live: true });
  });

  it("ticks from nothing on the first answer", () => {
    const arriving = { asking: true, streamed: 12, streamBegan: 1000, now: 1500 };
    expect(reading(null, arriving, ready())).toMatchObject({ tokens: "12", cost: "<$0.001", live: true });
  });

  it("reads a local model's speed off the pieces while they arrive", () => {
    const local = ready({ kind: "local", price: null });
    const arriving = { asking: true, streamed: 100, streamBegan: 1000, now: 3000 };
    expect(reading(null, arriving, local)).toMatchObject({ rate: "50 tok/s", cost: null });

    // Too early to be a rate: two pieces in ten milliseconds is not 200 a second.
    const early = { asking: true, streamed: 2, streamBegan: 1000, now: 1010 };
    expect(reading(null, early, local)?.rate).toBeNull();
  });
});
