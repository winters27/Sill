import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Preferences } from "$lib/settings";

/**
 * Room for the first mount, which compiles and renders 4,500 lines of Svelte.
 *
 * Set here rather than on the suite: the rest of the component tests draw one
 * small component and a five second ceiling is the right one for them. Still a
 * ceiling, so a handler that genuinely hangs fails rather than runs forever.
 */
vi.setConfig({ testTimeout: 30_000 });

/**
 * The launcher's own keydown handler, driven by real key events.
 *
 * Named for what it drives rather than for the file it imports, because
 * SvelteKit reserves the `+` prefix inside `src/routes` and refuses to sync a
 * tree containing a `+page.svelte.test.ts`. It tests `+page.svelte`.
 *
 * `P5-07` left `onKeydown` in this file rather than moving it out, and said
 * why: it reads about twenty-five pieces of the page's state and threading
 * them through a context object would cost more than the line count bought.
 * `P7-03` then named it as untested, because nothing here could render a
 * component at all.
 *
 * Both halves of that are addressed by rendering the page instead of moving
 * the function. What these prove is the half no unit test can reach: the
 * decisions in `$lib/typing` are well covered on their own, and **every one
 * of their call sites is inside this handler**, so deleting the call leaves
 * the pure function passing its tests while the key does nothing. That is the
 * fifth time that shape has been found in this codebase, and it is the shape
 * these tests exist to close.
 *
 * Rust is mocked at the one boundary it speaks through. Nothing below asserts
 * anything about what Rust answers; every assertion is about what the window
 * decides for itself when a key arrives.
 */

/**
 * What `get_preferences` answers.
 *
 * `satisfies Record<keyof Preferences, unknown>` rather than a loose object,
 * so a section added to `Preferences` and forgotten here fails `npm run check`
 * instead of failing a keystroke at run time. Two lists that must agree, with
 * the type system making them agree.
 */
const PREFERENCES = {
  appearance: {
    glassStrength: 1,
    font: "inter",
    theme: "midnight",
    chromaStrength: 1,
    visibleRows: 8,
    windowWidth: 720,
    backdrop: "acrylic",
    tintAlpha: 0.5,
    summonOn: "cursor",
  },
  hotkey: { summon: "Alt+Space", dismissOnBlur: true },
  navigation: { numeric: false },
  widgets: { pinned: [] },
  general: {},
  snippets: {},
  taps: {},
  ai: {},
  dictation: {},
  tts: {},
  clipboard: {},
  sources: {},
  files: {},
  browsers: {},
  store: {},
  webSearch: {},
  screenshot: {},
  scripts: {},
  hyper: {},
  bindings: [],
  aliases: [],
  actionKeys: {},
  emoji: {},
  privacy: {},
  mcp: {},
  layouts: [],
} satisfies Record<keyof Preferences, unknown>;

/** One ranked row, with only the fields the root list reads. */
function row(id: string, title: string) {
  return {
    id,
    extension: "app",
    extensionTitle: "Application",
    title,
    subtitle: "",
    mode: "app",
    entrypoint: `C:/programs/${id}.exe`,
    icon: "",
    matched: [],
  };
}

/** Answers for the commands the first mount asks for, by name. */
const ANSWERS: Record<string, unknown> = {
  get_preferences: PREFERENCES,
  default_browser: null,
  // The movement preset, which is decided in Rust so the window and the
  // settings screen cannot disagree about which key means what.
  navigation_chords: { Down: "next", Up: "previous", Home: "first" },
  search_commands: [row("app:a", "Alpha"), row("app:b", "Beta"), row("app:c", "Gamma")],
  keyboard_reference: [{ title: "Moving", keys: [{ chord: "Down", action: "Next" }] }],
  index_building: false,
  ai_ready: false,
  // Null rather than an empty list: the welcome screen is a mode of its own,
  // and anything truthy here puts the launcher on it instead of the root list.
  welcome: null,
};

const invoke = vi.fn(async (command: string, _args?: unknown) => {
  if (command in ANSWERS) return ANSWERS[command];
  // Everything else is a list of rows. An empty root list is a legitimate
  // state and the one that keeps these tests about the keyboard.
  return [];
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) => invoke(command, args as never),
  convertFileSrc: (path: string) => path,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
  emit: async () => {},
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    label: "main",
    listen: async () => () => {},
    onFocusChanged: async () => () => {},
    hide: async () => {},
    show: async () => {},
    isVisible: async () => true,
  }),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    label: "main",
    listen: async () => () => {},
    onFocusChanged: async () => () => {},
  }),
}));

let mounted: Record<string, unknown> | null = null;

/** The page, mounted and settled, plus its search field. */
async function launcher() {
  const { mount, tick } = await import("svelte");
  const Page = (await import("./+page.svelte")).default;

  const target = document.createElement("div");
  document.body.append(target);

  mounted = mount(Page, { target, props: {} as never });

  // `onMount` awaits Rust before it draws anything but the field, so the
  // first paint alone is not the state a keystroke arrives in.
  for (let i = 0; i < 12; i += 1) {
    await Promise.resolve();
    await tick();
  }

  const field = target.querySelector<HTMLInputElement>('input[aria-label="Search"]');
  if (!field) throw new Error("the launcher drew no search field");

  return { target, field, tick };
}

/** One key, on the window, exactly as the browser delivers it. */
function press(key: string, modifiers: Partial<KeyboardEventInit> = {}) {
  const event = new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
    ...modifiers,
  });
  window.dispatchEvent(event);
  return event;
}

beforeEach(() => {
  invoke.mockClear();
});

afterEach(async () => {
  if (mounted) {
    const { unmount } = await import("svelte");
    await unmount(mounted, { outro: false });
  }
  mounted = null;
  document.body.innerHTML = "";
});

describe("the keys the launcher answers", () => {
  /**
   * `?` on an empty field opens the keyboard reference.
   *
   * `askedForTheKeys` is tested on its own in `typing.test.ts` and its only
   * caller in the application is the arm inside `onKeydown`. Delete the arm
   * and every one of those tests still passes while the key does nothing.
   */
  it("opens the key sheet for ? on an empty field", async () => {
    const { target, tick } = await launcher();

    expect(target.querySelector(".sheet")).toBe(null);

    press("?");
    await tick();

    expect(target.querySelector(".sheet")).not.toBe(null);
  });

  /**
   * And with something typed it is a question mark, which is the half that
   * makes the test above mean something.
   *
   * A rule that only ever says yes is not a rule. This is the fixture the
   * arm has to reject: same key, same mode, different field.
   */
  it("treats ? as a character once something is typed", async () => {
    const { target, field, tick } = await launcher();

    field.focus();
    field.value = "note";
    field.dispatchEvent(new Event("input", { bubbles: true }));
    await tick();

    press("?");
    await tick();

    expect(target.querySelector(".sheet")).toBe(null);
  });

  /**
   * A character arriving while the field does not have focus still lands
   * in it.
   *
   * The launcher is summoned by a key and typed into at once, so there is
   * always a moment where the window is up and the field is not focused yet.
   * `typedInto` decides what the field becomes and is covered on its own;
   * nothing covered that anybody calls it. A dropped first letter gives back
   * a wrong query rather than a slow one.
   */
  it("puts a character typed with focus elsewhere into the field", async () => {
    const { field, tick } = await launcher();

    // Focus deliberately on the body: the state right after a summon, and
    // after a picture is dismissed or a row is clicked.
    field.blur();
    document.body.focus();
    expect(document.activeElement).not.toBe(field);

    const event = press("f");
    await tick();

    expect(field.value).toBe("f");
    expect(event.defaultPrevented).toBe(true);
  });

  /**
   * A chord is not a character, so it is not typed into the field.
   *
   * The negative half of the same arm: `isTyping` refuses anything with a
   * modifier held, because every movement preset is a Ctrl chord and the
   * field has focus the whole time.
   */
  it("does not type a chord into the field", async () => {
    const { field, tick } = await launcher();

    field.blur();
    document.body.focus();

    press("f", { ctrlKey: true });
    await tick();

    expect(field.value).toBe("");
  });

  /**
   * Ctrl+, opens Settings, which is the convention in essentially every
   * application and the only way in that does not need the launcher menu.
   */
  it("opens settings on ctrl+comma", async () => {
    const { tick } = await launcher();

    invoke.mockClear();
    press(",", { ctrlKey: true });
    await tick();

    expect(invoke.mock.calls.map(([command]) => command)).toContain("open_settings");
  });
});

/** Which row the list says is current, by its text. */
function highlighted(target: HTMLElement): string | undefined {
  return [...target.querySelectorAll('[role="option"]')]
    .find((one) => one.getAttribute("aria-selected") === "true")
    ?.textContent?.trim();
}

describe("moving through the results", () => {
  /**
   * The movement preset actually moves the highlight.
   *
   * Which key means "next" is decided in Rust and looked up here, so this is
   * two things at once: that the preset is consulted at all, and that the
   * answer is applied to the selection rather than to a variable nothing
   * draws. `roving.test.ts` and `selection.test.ts` cover the arithmetic; no
   * test before this one pressed a key.
   */
  it("moves the highlight down and back up", async () => {
    const { target, tick } = await launcher();

    expect(target.querySelectorAll('[role="option"]').length).toBe(3);
    expect(highlighted(target)).toContain("Alpha");

    press("ArrowDown");
    await tick();
    expect(highlighted(target)).toContain("Beta");

    press("ArrowDown");
    await tick();
    expect(highlighted(target)).toContain("Gamma");

    press("ArrowUp");
    await tick();
    expect(highlighted(target)).toContain("Beta");
  });

  /**
   * Off the end it wraps, rather than sticking or running past the list.
   *
   * The wrap is the half that a modulo written the other way gets wrong, and
   * the half somebody notices: holding Down at the bottom of a short list and
   * having nothing happen reads as the launcher having stopped responding.
   */
  it("wraps from the last row round to the first", async () => {
    const { target, tick } = await launcher();

    press("Home");
    press("ArrowUp");
    await tick();

    expect(highlighted(target)).toContain("Gamma");

    press("ArrowDown");
    await tick();
    expect(highlighted(target)).toContain("Alpha");
  });
});
