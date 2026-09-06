import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount, type ComponentProps } from "svelte";

/*
 * Rust answers what already runs on a key. Here that answer is a script, so
 * the recorder can be shown a taken key without a launcher behind it.
 */
const owners = vi.hoisted(() =>
  vi.fn(async (_accelerator: string) => [] as { chord: string; does: string; section: string }[]),
);

vi.mock("$lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$lib/settings")>()),
  keyOwners: owners,
}));

import KeyRecorder from "./KeyRecorder.svelte";

type Props = ComponentProps<typeof KeyRecorder>;

let mounted: Record<string, unknown> | null = null;

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  owners.mockReset();
  owners.mockResolvedValue([]);
  document.body.innerHTML = "";
});

function draw(props: Partial<Props> = {}) {
  const onsave = vi.fn(async (_chord: string) => {});
  const all: Props = {
    chord: "",
    scope: "hotkey",
    section: "From anywhere",
    onsave,
    ...props,
  };
  mounted = mount(KeyRecorder, { target: document.body, props: all });
  flushSync();
  const button = document.querySelector<HTMLButtonElement>("button.key");
  if (!button) throw new Error("the recorder drew no button");
  return { onsave, button };
}

function press(
  button: HTMLButtonElement,
  key: string,
  held: { ctrl?: boolean; alt?: boolean; shift?: boolean; meta?: boolean } = {},
) {
  button.dispatchEvent(
    new KeyboardEvent("keydown", {
      key,
      ctrlKey: held.ctrl ?? false,
      altKey: held.alt ?? false,
      shiftKey: held.shift ?? false,
      metaKey: held.meta ?? false,
      bubbles: true,
      cancelable: true,
    }),
  );
  flushSync();
}

/** Lets the awaited owners lookup and save settle. */
async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

describe("recording a key", () => {
  it("shows the modifiers held so far and saves nothing for them", () => {
    const { onsave, button } = draw();
    button.click();
    flushSync();

    press(button, "Control", { ctrl: true });

    expect(document.body.textContent).toContain("Ctrl");
    expect(document.body.textContent).toContain("then a key");
    expect(onsave).not.toHaveBeenCalled();
  });

  it("saves the chord once a key finishes it and nothing already runs on it", async () => {
    const { onsave, button } = draw();
    button.click();
    flushSync();

    press(button, "k", { ctrl: true });
    await settle();

    expect(owners).toHaveBeenCalledWith("Ctrl+K");
    expect(onsave).toHaveBeenCalledWith("Ctrl+K");
    expect(button.getAttribute("aria-pressed")).toBe("false");
  });

  it("refuses a key that already does something in the same section, and says what", async () => {
    owners.mockResolvedValue([{ chord: "Ctrl+K", does: "Copy Path", section: "From anywhere" }]);
    const { onsave, button } = draw();
    button.click();
    flushSync();

    press(button, "k", { ctrl: true });
    await settle();

    expect(onsave).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain("Already copy Path here");
    // Still recording, so the next key can be tried without another click.
    expect(button.getAttribute("aria-pressed")).toBe("true");
  });

  it("saves a key that runs something in another section, and mentions it", async () => {
    owners.mockResolvedValue([{ chord: "Ctrl+K", does: "Show actions", section: "Moving around" }]);
    const { onsave, button } = draw();
    button.click();
    flushSync();

    press(button, "k", { ctrl: true });
    await settle();

    expect(onsave).toHaveBeenCalledWith("Ctrl+K");
    expect(document.body.textContent).toContain("Also show actions (Moving around)");
  });

  it("refuses the Windows key for an action key with the reason, and saves nothing", () => {
    const { onsave, button } = draw({ scope: "action", section: "Acting on a row" });
    button.click();
    flushSync();

    press(button, "k", { meta: true });

    expect(onsave).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain("The Windows key cannot run an action");
  });

  it("leaves the chord alone on Escape", () => {
    const { onsave, button } = draw({ chord: "Alt+Space" });
    button.click();
    flushSync();
    press(button, "Escape");

    expect(onsave).not.toHaveBeenCalled();
    expect(button.getAttribute("aria-pressed")).toBe("false");
    expect(document.body.textContent).toContain("Alt");
    expect(document.body.textContent).toContain("Space");
  });

  it("clears on Backspace where the key can be off", async () => {
    const onclear = vi.fn(async () => {});
    const { button } = draw({ chord: "Ctrl+Alt+S", onclear });
    button.click();
    flushSync();
    press(button, "Backspace");
    await settle();

    expect(onclear).toHaveBeenCalled();
  });
});
