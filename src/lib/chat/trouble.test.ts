import { describe, expect, it } from "vitest";

import { tier } from "./trouble";

describe("what kind of trouble a message is", () => {
  it("reads a limit as a limit", () => {
    expect(tier("That service is rate limiting this key. Try again in a minute.")).toBe("limit");
    expect(tier("Quota exceeded for this model (429)")).toBe("limit");
  });

  it("reads everything else as an error", () => {
    expect(tier("could not reach Ollama: connection refused")).toBe("error");
    expect(tier("That key was not accepted.")).toBe("error");
    expect(tier("")).toBe("error");
  });
});
