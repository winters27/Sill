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
  navigation_chords: {
    Down: "next",
    Up: "previous",
    Home: "first",
    End: "last",
    Enter: "open",
    Escape: "back",
    "Ctrl+K": "actions",
  },
  search_commands: [row("app:a", "Alpha"), row("app:b", "Beta"), row("app:c", "Gamma")],
  keyboard_reference: [{ title: "Moving", keys: [{ chord: "Down", action: "Next" }] }],
  index_building: false,
  ai_ready: false,
  // Null rather than an empty list: the welcome screen is a mode of its own,
  // and anything truthy here puts the launcher on it instead of the root list.
  welcome: null,
};

/**
 * Answers a test sets for itself, ahead of the shared ones, and cleared after
 * each test so one test's world never leaks into the next.
 */
const OVERRIDES: Record<string, unknown> = {};

const invoke = vi.fn(async (command: string, _args?: unknown) => {
  if (command in OVERRIDES) {
    const answer = OVERRIDES[command];
    return typeof answer === "function" ? (answer as () => unknown)() : answer;
  }
  if (command in ANSWERS) return ANSWERS[command];
  // Everything else is a list of rows. An empty root list is a legitimate
  // state and the one that keeps these tests about the keyboard.
  return [];
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) => invoke(command, args as never),
  convertFileSrc: (path: string) => path,
}));

/**
 * Every listener the page and its modules registered, by event name, so a
 * test can deliver an event the way Rust would.
 *
 * Kept for the whole file rather than cleared per test: `$lib/visible`
 * subscribes once for the life of the module, and clearing would leave it
 * deaf for every test after the first. The page's own listeners take
 * themselves out when it unmounts.
 */
const heard = new Map<string, Set<(event: { payload: unknown }) => void>>();

function hear(name: string, handler: (event: { payload: unknown }) => void) {
  const all = heard.get(name) ?? new Set();
  all.add(handler);
  heard.set(name, all);
  return () => all.delete(handler);
}

/** Delivers an event to everything listening for it. */
function fire(name: string, payload: unknown = null) {
  for (const handler of [...(heard.get(name) ?? [])]) handler({ payload });
}

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: (event: { payload: unknown }) => void) => hear(name, handler),
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
  for (const key of Object.keys(OVERRIDES)) delete OVERRIDES[key];

  /*
   * Reduced motion, so the panel and the menu open and close in no time.
   *
   * happy-dom runs a transition's animation and rejects it when the element
   * goes before it finishes, which is every panel a test opens and closes.
   * `$lib/motion` makes a zero-length transition under reduced motion, and
   * nothing these tests assert depends on an animation.
   */
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query: string) =>
      ({
        matches: query.includes("reduce"),
        media: query,
        onchange: null,
        addEventListener: () => {},
        removeEventListener: () => {},
        addListener: () => {},
        removeListener: () => {},
        dispatchEvent: () => false,
      }) as MediaQueryList,
  );
});

/** Lets promises settle and Svelte draw, a few times over. */
async function settle(tick: () => Promise<void>, rounds = 8) {
  for (let i = 0; i < rounds; i += 1) {
    await Promise.resolve();
    await tick();
  }
}

/** Every command the page asked Rust for since the last clear. */
function asked(): string[] {
  return invoke.mock.calls.map(([command]) => command);
}

/** Types into a field the way a person does, through its input event. */
function typeInto(field: HTMLInputElement, text: string) {
  field.focus();
  field.value = text;
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

/** One action, as `actions_for` answers. */
const COPY = { id: "sill.file.copy", title: "Copy", icon: "Clipboard", primary: false };

/**
 * Enough actions for the panel to draw its filter, which it does only once
 * there are more than five to narrow.
 */
const SIX = ["one", "two", "three", "four", "five", "six"].map((name) => ({
  ...COPY,
  id: `sill.test.${name}`,
  title: `Do ${name}`,
}));

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

describe("the action panel owns the keys typed into it", () => {
  /**
   * `P0`: Delete in the panel's filter forgot the conversation under it.
   *
   * The conversation list's Delete ran before the panel had a chance to
   * claim the key, and it read the launcher's empty query rather than the
   * filter, so a Delete meant for one letter forgot a conversation for good.
   */
  it("edits the filter rather than forgetting a conversation", async () => {
    OVERRIDES.ai_conversations = [
      { id: "c1", title: "First", replies: 1, age: 5, open: false },
      { id: "c2", title: "Second", replies: 1, age: 9, open: false },
    ];
    OVERRIDES.actions_for = SIX;

    const { target, tick } = await launcher();
    fire("sill://run", "sill:conversations");
    await settle(tick);

    press("k", { ctrlKey: true });
    await settle(tick);

    const filter = target.querySelector<HTMLInputElement>('input[aria-label="Filter actions"]');
    expect(filter, "the panel did not open").not.toBe(null);
    typeInto(filter!, "x");
    await tick();

    invoke.mockClear();
    press("Delete");
    await settle(tick);

    expect(asked()).not.toContain("ai_forget");
  });

  /** The other half: with no panel open, Delete on an empty field does forget. */
  it("still forgets the conversation when no panel is open", async () => {
    OVERRIDES.ai_conversations = [{ id: "c1", title: "First", replies: 1, age: 5, open: false }];

    const { tick } = await launcher();
    fire("sill://run", "sill:conversations");
    await settle(tick);

    invoke.mockClear();
    press("Delete");
    await settle(tick);

    expect(asked()).toContain("ai_forget");
  });

  it("types a question mark into the filter instead of opening the key sheet", async () => {
    OVERRIDES.actions_for = SIX;

    const { target, tick } = await launcher();
    press("k", { ctrlKey: true });
    await settle(tick);
    expect(target.querySelector('input[aria-label="Filter actions"]')).not.toBe(null);

    press("?");
    await settle(tick);

    expect(target.querySelector(".sheet")).toBe(null);
  });

  it("closes when the launcher is hidden", async () => {
    OVERRIDES.actions_for = [COPY];

    const { target, tick } = await launcher();
    press("k", { ctrlKey: true });
    await settle(tick);
    expect(target.querySelector('[role="menu"].panel')).not.toBe(null);

    fire("sill://hidden", "main");
    await settle(tick);

    expect(target.querySelector('[role="menu"].panel')).toBe(null);

    // And the keys are the list's again: Down moves the highlight rather than
    // walking a panel nobody can see.
    press("ArrowDown");
    await tick();
    expect(highlighted(target)).toContain("Beta");
  });
});

describe("undo is offered for the moment after an action", () => {
  async function ranAnAction() {
    OVERRIDES.actions_for = [COPY];
    OVERRIDES.run_action = { message: "Copied Alpha", undoneBy: 7 };

    const view = await launcher();
    press("k", { ctrlKey: true });
    await settle(view.tick);
    press("Enter");
    await settle(view.tick);
    view.field.focus();
    return view;
  }

  it("takes back the action on Ctrl+Z straight afterwards", async () => {
    const { tick } = await ranAnAction();

    invoke.mockClear();
    press("z", { ctrlKey: true });
    await settle(tick);

    expect(asked()).toContain("undo_activity");
  });

  /**
   * `P0`: Ctrl+Z undid the last action whenever one was on offer, even while
   * text was being edited, even hours later. Typing is somebody who has moved
   * on, and from then on Ctrl+Z is theirs.
   */
  it("withdraws the offer once something is typed", async () => {
    const { field, tick } = await ranAnAction();

    typeInto(field, "a");
    await settle(tick);

    invoke.mockClear();
    press("z", { ctrlKey: true });
    await settle(tick);

    expect(asked()).not.toContain("undo_activity");
  });
});

describe("Enter acts on the answer to what was typed", () => {
  /**
   * `P1`: an Enter that beat the search opened a row from the previous
   * query's list. The screenshot runbook recorded two runs that opened
   * Clipboard History that way while meaning to open something else.
   */
  it("waits for the search to answer before opening", async () => {
    let answer: (rows: unknown) => void = () => {};
    const { field, tick } = await launcher();

    OVERRIDES.search_commands = () => new Promise((resolve) => (answer = resolve));
    typeInto(field, "br");
    await settle(tick);

    invoke.mockClear();
    press("Enter");
    await settle(tick);
    expect(asked(), "opened before the answer arrived").not.toContain("launch_command");

    answer([row("app:bravo", "Bravo")]);
    await settle(tick, 16);

    const launched = invoke.mock.calls.find(([command]) => command === "launch_command");
    expect(launched, "the waiting Enter never ran").toBeDefined();
    expect(JSON.stringify(launched![1])).toContain("app:bravo");
  });

  it("does not open anything when the search it waited for fails", async () => {
    let fail: (why: unknown) => void = () => {};
    const { field, tick } = await launcher();

    OVERRIDES.search_commands = () => new Promise((_, reject) => (fail = reject));
    typeInto(field, "br");
    await settle(tick);

    invoke.mockClear();
    press("Enter");
    await settle(tick);
    fail(new Error("index unavailable"));
    await settle(tick, 16);

    expect(asked()).not.toContain("launch_command");
  });

  it("abandons a waiting Enter when another key is typed first", async () => {
    const answers: ((rows: unknown) => void)[] = [];
    const { field, tick } = await launcher();

    OVERRIDES.search_commands = () => new Promise((resolve) => answers.push(resolve));
    typeInto(field, "b");
    await settle(tick);
    press("Enter");
    typeInto(field, "br");
    await settle(tick);

    invoke.mockClear();
    answers[0]?.([row("app:b", "B")]);
    answers[1]?.([row("app:bravo", "Bravo")]);
    await settle(tick, 16);

    expect(asked()).not.toContain("launch_command");
  });
});

describe("what a summon comes back to", () => {
  /**
   * `P1`: the switcher's own key opened it, Escape put it away, and the
   * ordinary summon brought the switcher back, where Escape put it away
   * again. The root was unreachable without the tray.
   */
  it("leaves the switcher for the root once it is put away", async () => {
    const { field, tick } = await launcher();

    fire("sill://switcher");
    await settle(tick);
    expect(field.placeholder).toContain("Switch to a window");

    press("Escape");
    await settle(tick);
    fire("sill://hidden", "main");
    await settle(tick);
    fire("sill://shown", "main");
    await settle(tick);

    expect(field.placeholder).toContain("Search for apps and commands");
  });

  it("goes back from the key sheet instead of closing the launcher", async () => {
    const { target, field, tick } = await launcher();

    press("?");
    await settle(tick);
    expect(target.querySelector(".sheet")).not.toBe(null);

    invoke.mockClear();
    press("Escape");
    await settle(tick);

    expect(target.querySelector(".sheet")).toBe(null);
    expect(asked()).not.toContain("dismiss");
    expect(field.placeholder).toContain("Search for apps and commands");
  });
});

describe("a permission card", () => {
  const CARD = { id: "card-1", title: "Read the clipboard", subject: "history", touches: "reads what you copied" };

  /**
   * `P0`: a card that arrives while somebody is typing was allowed by the
   * next Enter, which was meant for the row or the draft.
   */
  it("does not let an Enter that arrives with it allow it", async () => {
    let now = 1000;
    const clock = vi.spyOn(performance, "now").mockImplementation(() => now);
    try {
      const { tick } = await launcher();
      fire("sill://ai-asking", CARD);
      await settle(tick);

      invoke.mockClear();
      press("Enter");
      await settle(tick);
      expect(asked(), "allowed before it could be read").not.toContain("ai_decide");
      expect(asked(), "the Enter fell through to the row").not.toContain("launch_command");

      now += 5000;
      press("Enter");
      await settle(tick);
      const decided = invoke.mock.calls.find(([command]) => command === "ai_decide");
      expect(decided).toBeDefined();
      expect(JSON.stringify(decided![1])).toContain("true");
    } finally {
      clock.mockRestore();
    }
  });

  it("is taken down on a summon once Rust says it was settled elsewhere", async () => {
    const { target, tick } = await launcher();
    fire("sill://ai-asking", CARD);
    await settle(tick);
    expect(target.textContent).toContain("Read the clipboard");

    OVERRIDES.ai_outstanding = null;
    fire("sill://shown", "main");
    await settle(tick);

    expect(target.textContent).not.toContain("Read the clipboard");
  });
});

describe("starting up", () => {
  /**
   * The arrows, Enter and Escape live in the chord map. It was read after
   * three other calls, and any of them failing left the launcher unable to
   * move for the rest of the run.
   */
  it("can still move when an earlier read fails", async () => {
    OVERRIDES.actions_for = () => Promise.reject(new Error("not ready"));

    const { target, tick } = await launcher();
    press("ArrowDown");
    await tick();

    expect(highlighted(target)).toContain("Beta");
  });
});
