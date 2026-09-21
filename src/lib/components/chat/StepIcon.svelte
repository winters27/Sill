<script lang="ts">
  /**
   * The mark beside a step in the timeline.
   *
   * Eight Phosphor glyphs at bold weight, from the generated table. Read at
   * ten pixels inside a node, where regular's 16-unit stroke on the 256 box
   * is 0.63px, under a device pixel, and bold's 24 units is 0.94px, the
   * weight the hand-drawn lines they replace put on screen. Monochrome and
   * bare: the node around them is already a filled circle, and a coloured
   * square inside a circle at this size is a smear.
   */
  import type { StepIconName } from "$lib/chat/verbs";
  import { STEP_GLYPHS } from "$lib/components/glyphs";

  interface Props {
    name: StepIconName;
  }

  let { name }: Props = $props();
</script>

<svg viewBox="0 0 256 256" fill="currentColor" aria-hidden="true">
  <path d={STEP_GLYPHS[name]} />
</svg>

<style>
  svg {
    width: var(--timeline-icon);
    height: var(--timeline-icon);
    display: block;
  }
</style>
