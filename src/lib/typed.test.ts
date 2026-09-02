import { describe, expect, it } from "vitest";

import { asUrl, isPath, isUrl } from "./typed";

describe("recognising an address", () => {
  it("takes anything with a scheme", () => {
    for (const text of [
      "https://example.com",
      "http://localhost:1425",
      "https://example.com/a/b?c=d#e",
      "ftp://files.example.org",
    ]) {
      expect(isUrl(text), `${text} is an address`).toBe(true);
    }
  });

  it("takes a bare host", () => {
    for (const text of ["example.com", "www.bbc.co.uk", "github.com/winters27/sill"]) {
      expect(isUrl(text), `${text} is an address`).toBe(true);
    }
  });

  /**
   * The half that matters. A launcher is mostly typed at with words, and a
   * row offering to open one as a web address would be wrong far more often
   * than it was right.
   */
  it("leaves ordinary queries alone", () => {
    for (const text of [
      "notepad",
      "visual studio code",
      "how do i center a div",
      "sill",
      "2 + 2",
      "",
      "   ",
    ]) {
      expect(isUrl(text), `${text} is not an address`).toBe(false);
    }
  });

  /**
   * File names are the trap: `readme.md` has exactly the shape of a host with
   * a two letter ending, and `.md` is a real top level domain.
   */
  it("leaves file names alone", () => {
    for (const text of [
      "readme.md",
      "notepad.exe",
      "main.rs",
      "package.json",
      "docker-compose.yml",
      "screenshot.png",
      "build.ps1",
    ]) {
      expect(isUrl(text), `${text} is a file name`).toBe(false);
    }
  });

  it("does not offer to open half a scheme", () => {
    expect(isUrl("https://")).toBe(false);
    expect(isUrl("http:/")).toBe(false);
  });

  it("leaves a path alone", () => {
    expect(isUrl("C:\\Users\\Brandon")).toBe(false);
    expect(isUrl("C:/Users/Brandon")).toBe(false);
  });

  it("puts a scheme on a bare host and leaves one that has it", () => {
    expect(asUrl("example.com")).toBe("https://example.com");
    expect(asUrl("  www.bbc.co.uk ")).toBe("https://www.bbc.co.uk");
    expect(asUrl("http://localhost:1425")).toBe("http://localhost:1425");
  });
});

describe("recognising a path", () => {
  it("takes a drive, a share and a shell variable", () => {
    for (const text of [
      "C:\\Users\\Brandon",
      "C:/Users/Brandon",
      "c:\\",
      "\\\\nas\\media",
      "%APPDATA%\\app.winters.sill",
      "%USERPROFILE%/Desktop",
    ]) {
      expect(isPath(text), `${text} is a path`).toBe(true);
    }
  });

  /**
   * A query is not a path because somebody typed a slash in it.
   */
  it("leaves everything else alone", () => {
    for (const text of [
      "notepad",
      "and/or",
      "C:",
      "https://example.com",
      "2/3",
      "",
    ]) {
      expect(isPath(text), `${text} is not a path`).toBe(false);
    }
  });
});
