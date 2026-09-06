<script lang="ts">
  /**
   * A permission card over whatever the launcher is showing.
   *
   * An extension asks for a permission at the moment it reaches for the
   * thing, which is usually while its own view is on screen. The card the
   * AI conversation draws lives inside that conversation, so in every other
   * mode a question arrived, sat in state nobody rendered, and refused
   * itself ninety seconds later. This draws the same card in front of any
   * mode, and answers it with the same two keys.
   *
   * Hears the question itself rather than being handed it, so the page has
   * nothing to wire beyond mounting this once. Rust sends the card to every
   * window, so a chat window that is also open shows it too; whichever
   * answers first wins, and the other's answer lands on a card that is gone,
   * which Rust ignores.
   *
   * `hidden` is for the one mode that already draws the card in its own
   * place. Two cards for one question would answer it twice.
   */
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  import { aiDecide, type AiAsking } from "$lib/exthost/commands";
  import ApprovalCard from "./chat/ApprovalCard.svelte";

  interface Props {
    hidden?: boolean;
  }

  let { hidden = false }: Props = $props();

  let asked = $state<AiAsking | null>(null);

  onMount(() => {
    let forget: UnlistenFn | null = null;
    let gone = false;

    void listen<AiAsking>("sill://ai-asking", ({ payload }) => {
      asked = payload;
    }).then((unlisten) => {
      if (gone) unlisten();
      else forget = unlisten;
    });

    return () => {
      gone = true;
      forget?.();
    };
  });

  function decide(allowed: boolean) {
    if (!asked) return;
    void aiDecide(asked.id, allowed);
    asked = null;
  }

  /**
   * Enter and Escape answer the card, ahead of whatever the mode would do
   * with them. Captured at the window so the launcher's own handler, which
   * listens on the same window in the bubbling phase, never sees the key.
   */
  function onKeydown(event: KeyboardEvent) {
    if (hidden || !asked) return;

    if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      decide(true);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      decide(false);
    }
  }
</script>

<svelte:window onkeydowncapture={onKeydown} />

{#if asked && !hidden}
  <div class="scrim" role="dialog" aria-modal="true" aria-label={asked.title}>
    <ApprovalCard {asked} ondecide={decide} />
  </div>
{/if}

<style>
  /* Inside the window's own box, so its rounded, transparent corners clip
     this too. As a fixed sibling it painted square into all four corners of
     a window DWM draws round, and sat under the launcher menu. No blur: see
     theme.css on `backdrop-filter`. */
  .scrim {
    position: absolute;
    inset: 0;
    z-index: var(--z-dialog-scrim);
    display: grid;
    place-items: center;
    padding: var(--space-6);
    background: var(--scrim);
  }
</style>
