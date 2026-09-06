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

  /**
   * What happens to something that looks like a credential.
   *
   * Worded as outcomes rather than as policy names. "Skip" says nothing about
   * what the user gets; "Do not store" does.
   */
  const SECRETS = [
    { value: "skip", label: "Do not store" },
    { value: "redact", label: "Note it only" },
    { value: "keep", label: "Store it" },
  ];

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

  /**
   * The other bound. An age says nothing about a week spent copying, and a
   * count says nothing about a code from last month that nothing has pushed
   * out yet, so both exist and neither replaces the other.
   */
  const LIMIT = [
    { value: "100", label: "100" },
    { value: "1000", label: "1,000" },
    { value: "10000", label: "10,000" },
    { value: "0", label: "No limit" },
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
        label="Keep history for"
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
    title="Keep at most"
    description="The oldest entries are deleted once there are more than this. Pinned entries, entries in a collection and whatever is open in the history are never counted or deleted."
  >
    {#snippet control()}
      <Segmented
        label="Keep at most"
        value={String(prefs.clipboard.maxEntries)}
        options={LIMIT}
        onchange={(next) => {
          prefs.clipboard.maxEntries = Number(next);
          commit();
        }}
      />
    {/snippet}
  </Row>

  <Row
    title="Things that look like passwords"
    description="Tokens, API keys and private keys are recognised by their published shapes, so a value that carries no such marker is never guessed at. The history is a plain file that anything running as you can read, which is why not storing it is the default."
    disabled={!prefs.clipboard.enabled}
  >
    {#snippet control()}
      <Segmented
        label="Things that look like passwords"
        value={prefs.clipboard.secrets}
        options={SECRETS}
        onchange={(next) => {
          prefs.clipboard.secrets = next as "skip" | "redact" | "keep";
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

  <Row
    title="Lock stored pictures"
    description="Pictures are kept so that only your Windows account can open them. Another account on this PC cannot, and neither can anyone holding a copy of the history file from a backup, a synced folder or the drive itself. It does not hide them from programs already running as you, which can unlock them exactly the way Sill does. Pictures already in the history are converted whichever way you change this, so nothing is lost by turning it on or off again. Text is not covered, because searching it means keeping it readable."
  >
    {#snippet control()}
      <Toggle
        bind:checked={prefs.clipboard.encryptImages}
        onchange={commit}
        label="Lock stored pictures"
      />
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
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4);
    border-radius: var(--radius-lg);
    background: var(--fill-0);
    box-shadow: var(--bevel-tile);
  }

  .figure {
    font-size: var(--text-display);
    font-weight: var(--weight-strong);
    font-variant-numeric: tabular-nums;
  }

  .unit {
    margin-left: var(--space-1);
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  .spacer {
    flex: 1;
  }

  .status {
    font-size: var(--text-meta);
    color: var(--accent);
  }

  .note {
    margin: var(--space-2) var(--space-half) 0;
    font-size: var(--text-meta);
    color: var(--text-3);
  }
</style>
