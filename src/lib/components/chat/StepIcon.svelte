<script lang="ts">
  /**
   * The mark beside a step in the timeline.
   *
   * Eight small line drawings, one per kind of thing a tool does, drawn here
   * rather than taken from an icon set: they are read at ten pixels inside a
   * node, where a detailed glyph is a smudge and a plain one is a shape.
   */
  import type { StepIconName } from "$lib/chat/verbs";

  interface Props {
    name: StepIconName;
  }

  let { name }: Props = $props();

  const PATHS: Record<StepIconName, string> = {
    search: "M10.5 18a7.5 7.5 0 1 0 0-15 7.5 7.5 0 0 0 0 15ZM16 16l5 5",
    file: "M6 3h8l5 5v13H6zM14 3v5h5",
    folder: "M3 6h6l2 2h10v11H3z",
    clipboard: "M9 4h6v3H9zM7 6H5v15h14V6h-2",
    window: "M3 5h18v14H3zM3 9h18",
    machine: "M4 5h16v11H4zM9 20h6M12 16v4",
    eye: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12ZM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
    act: "M5 12l5 5L20 7",
  };
</script>

<svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
  <path
    d={PATHS[name]}
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
</svg>

<style>
  svg {
    width: var(--timeline-icon);
    height: var(--timeline-icon);
    display: block;
  }
</style>
