import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

let answer: string | null = null;

vi.mock("$lib/exthost/commands", () => ({
  windowPreview: async () => answer,
  forgetPreviews: async () => {},
}));

const SwitcherPreview = (await import("./SwitcherPreview.svelte")).default;

let mounted: ReturnType<typeof mount> | null = null;

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

async function drawn(entrypoint: string | undefined) {
  vi.useFakeTimers();
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({ entrypoint });
  mounted = mount(SwitcherPreview, { target, props });
  flushSync();
  await vi.advanceTimersByTimeAsync(200);
  flushSync();
  return { strip: target.querySelector("aside")!, props };
}

describe("the switcher's picture strip", () => {
  /**
   * The strip held its width before it had drawn anything, so a short list
   * sat in the left two thirds of the window beside a blank third.
   */
  it("takes no room while it has nothing to show", async () => {
    answer = null;
    const { strip } = await drawn("hwnd:1");

    expect(strip.classList.contains("unused")).toBe(true);
  });

  it("opens once there is a picture, and stays open for the list", async () => {
    answer = "data:image/png;base64,AAAA";
    const { strip, props } = await drawn("hwnd:1");
    expect(strip.classList.contains("unused")).toBe(false);

    // A window that cannot be photographed does not close it again, so the
    // rows do not shuffle sideways as somebody arrows past it.
    answer = null;
    props.entrypoint = "hwnd:2";
    flushSync();
    await vi.advanceTimersByTimeAsync(200);
    flushSync();
    expect(strip.classList.contains("unused")).toBe(false);
  });
});
