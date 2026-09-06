<script lang="ts">
  /*
   * The one thing every Sill window has in common: the browser stays out of
   * it. See `$lib/quiet` for what that means and why the page has a part in
   * it at all. Nothing else belongs here; each window is its own route.
   */
  import { onMount } from "svelte";
  import { quiet } from "$lib/quiet";

  let { children } = $props();

  onMount(() => quiet());
</script>

{@render children()}
