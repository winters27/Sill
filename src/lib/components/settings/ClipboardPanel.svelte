<script lang="ts">
  import { onMount } from "svelte";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Segmented from "./Segmented.svelte";
  import Button from "./Button.svelte";
  import TermList from "./TermList.svelte";
  import Toggle from "../Toggle.svelte";
  import { clipboardClear, clipboardCount } from "$lib/clipboard";
  import type { Preferences } from "$lib/settings";

  interface Props {
    /** Not `$bindable`: nothing here reassigns it, only writes its fields. */
    prefs: Preferences;
    commit: () => void;
  }

  let { prefs, commit }: Props = $props();

  let count = $state(0);
  let status = $state("");
  let confirming = $state(false);

  /**
   * How long history is kept, as the handful of answers anyone actually
   * wants. A free-text number of days would be a worse question.
   */
  const RETENTION = [
    { value: "1", label: "1 day" },
    { value: "7", label: "1 week" },
    { value: "30", label: "1 month" },
    { value: "90", label: "3 months" },
    { value: "0", label: "Forever" },
  ];

  async function refresh() {
    try {
      count = await clipboardCount();
    } catch {
      // The history not being open yet is not worth a message; the panel
      // reads perfectly well without a count.
    }
  }

  async function clearAll() {
    if (!confirming) {
      confirming = true;
      // Reverts on its own, so an unconfirmed press does not leave the
      // button armed for whoever walks past next.
      setTimeout(() => (confirming = false), 4000);
      return;
    }
    confirming = false;
    const gone = await clipboardClear(false);
    await refresh();
    status = `Deleted ${gone.toLocaleString()} ${gone === 1 ? "entry" : "entries"}`;
    setTimeout(() => (status = ""), 2000);
  }

  onMount(refresh);
</script>

<Section
  label="History"
  description="Everything copied is kept on this machine so it can be found and pasted again. Nothing is sent anywhere."
>
  <Row
    title="Record what I copy"
    description="Turning this off stops new entries. What is already here stays until it is deleted or ages out."
  >
    {#snippet control()}
      <Toggle bind:checked={prefs.clipboard.enabled} onchange={commit} label="Record what I copy" />
    {/snippet}
  </Row>

  <Row
    title="Keep history for"
    description="Older unpinned entries are deleted on the next start. A clipboard collects one-time codes and whatever was typed near a password field, so an end date is a feature rather than a limitation."
  >
    {#snippet control()}
      <Segmented
        value={String(prefs.clipboard.retainDays)}
        options={RETENTION}
        onchange={(next) => {
          prefs.clipboard.retainDays = Number(next);
          commit();
        }}
      />
    {/snippet}
  </Row>

  <Row
    title="Keep images"
    description="Screenshots are much the largest thing a clipboard carries. Anything over 8 MB is listed but its pixels are not stored."
    disabled={!prefs.clipboard.enabled}
  >
    {#snippet control()}
      <Toggle bind:checked={prefs.clipboard.keepImages} onchange={commit} label="Keep images" />
    {/snippet}
  </Row>
</Section>

<Section
  label="Excluded applications"
  description="Copies made in these are never recorded. Matched loosely, so “chrome” covers every Chrome window. Password managers already exclude themselves through Windows, so this is for everything else worth keeping out."
>
  <Row title="Never record from">
    {#snippet children()}
      <TermList
        bind:terms={prefs.clipboard.ignoredApps}
        onchange={() => commit()}
      />
    {/snippet}
  </Row>
</Section>

<Section label="Stored now" bare>
  <div class="stored">
    <div>
      <span class="figure">{count.toLocaleString()}</span>
      <span class="unit">{count === 1 ? "entry" : "entries"}</span>
    </div>
    <span class="spacer"></span>
    {#if status}<span class="status">{status}</span>{/if}
    <Button
      label={confirming ? "Delete them all?" : "Clear history"}
      tone="danger"
      onclick={clearAll}
    />
  </div>
  <p class="note">Pinned entries are kept.</p>
</Section>

<style>
  .stored {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 18px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.02);
    box-shadow: var(--bevel-tile);
  }

  .figure {
    font-size: 22px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .unit {
    margin-left: 6px;
    font-size: var(--text-meta);
    color: var(--text-faint);
  }

  .spacer {
    flex: 1;
  }

  .status {
    font-size: var(--text-meta);
    color: var(--core-accent);
  }

  .note {
    margin: 8px 2px 0;
    font-size: var(--text-meta);
    color: var(--text-faint);
  }
</style>
