<script lang="ts">
  /**
   * Triggers, which Windows holds and Sill only writes down.
   *
   * Nothing on this panel is stored on this side. The list is read from Task
   * Scheduler every time the panel opens, so what it draws is what the
   * machine actually has rather than what Sill last believed about it, and a
   * trigger somebody removed in Task Scheduler is simply gone from here.
   *
   * The action picker only offers what a trigger may name. Offering the whole
   * registry and refusing afterwards would let somebody fill the form in and
   * then be told the thing they wanted was never possible.
   */
  import { onMount } from "svelte";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import Select from "./Select.svelte";
  import Instead from "../Instead.svelte";
  import { standing } from "$lib/instead";
  import {
    DEFAULT_TIME,
    listAutomations,
    said,
    schedulableActions,
    scheduleAutomation,
    timeToWhen,
    unscheduleAutomation,
    type Offer,
    type Row as Trigger,
    type When,
  } from "$lib/automations";

  let triggers = $state<Trigger[]>([]);
  let offers = $state<Offer[]>([]);
  let loading = $state(true);
  let failed = $state(false);

  let name = $state("");
  let action = $state("");
  let target = $state("");
  let schedule = $state<"daily" | "atLogon" | "onUnlock">("daily");
  let time = $state(DEFAULT_TIME);

  let note = $state("");
  let error = $state("");
  let busy = $state(false);
  /** The one whose Remove has been pressed once and is asking. */
  let confirming = $state("");

  const SCHEDULES = [
    { value: "daily", label: "Every day at" },
    { value: "atLogon", label: "When I sign in" },
    { value: "onUnlock", label: "When I unlock this PC" },
  ];

  /** The schedule the form describes, or null while the time is unreadable. */
  const when = $derived.by((): When | null => {
    if (schedule === "atLogon") return { kind: "atLogon" };
    if (schedule === "onUnlock") return { kind: "onUnlock" };
    return timeToWhen(time);
  });

  const ready = $derived(
    name.trim().length > 0 && action.length > 0 && target.trim().length > 0 && when !== null,
  );

  /**
   * Reads the list back out of Windows.
   *
   * A failure is drawn rather than swallowed. The scheduler service can be
   * stopped or refused by policy, and an empty list in that case would say
   * "no triggers" about a machine that may well have several.
   */
  async function refresh() {
    try {
      [triggers, offers] = await Promise.all([listAutomations(), schedulableActions()]);
      failed = false;
      if (!action && offers.length) action = offers[0].id;
    } catch (err) {
      failed = true;
      error = `${err}`;
    } finally {
      loading = false;
    }
  }

  async function add() {
    if (!ready || !when) return;

    busy = true;
    error = "";
    note = "";

    try {
      note = await scheduleAutomation({
        name,
        action,
        target,
        kind: null,
        argument: null,
        when,
      });

      name = "";
      target = "";
      await refresh();
    } catch (err) {
      error = `${err}`;
    } finally {
      busy = false;
    }
  }

  /**
   * Takes one out of Windows, on the second press.
   *
   * Asking first because this is the only control on the panel that changes
   * the machine outside Sill, and because a trigger removed by a stray click
   * is one nobody notices missing until the morning it did not run.
   */
  async function remove(row: Trigger) {
    if (confirming !== row.name) {
      confirming = row.name;
      return;
    }

    confirming = "";
    error = "";
    note = "";

    try {
      await unscheduleAutomation(row.name);
      note = `${row.name} is no longer in Task Scheduler.`;
      await refresh();
    } catch (err) {
      error = `${err}`;
    }
  }

  onMount(refresh);
</script>

<Section
  label="Triggers"
  description="Windows runs these, not Sill. Each one is a scheduled task that starts Sill and asks it for a single action, so nothing here costs anything while the launcher is sitting idle."
>
  {#each triggers as row (row.name)}
    <Row
      title={row.name}
      description={row.suspect
        ? `Sill will not vouch for this one: ${row.suspect}`
        : `${row.title} on ${row.target}`}
    >
      {#snippet control()}
        <div class="right">
          {#if row.next}<span class="next">next {row.next}</span>{/if}
          {#if !row.enabled}<span class="off">turned off</span>{/if}
          <Button
            label={confirming === row.name ? "Remove it?" : "Remove"}
            tone="danger"
            onclick={() => void remove(row)}
          />
        </div>
      {/snippet}
    </Row>
  {/each}

  <Instead
    tone={failed ? "failed" : standing({ failed: false, loading, count: triggers.length })}
    inline
    headline={failed ? "Task Scheduler could not be read" : "No triggers yet"}
  >
    {#if failed}
      {error}
    {:else}
      Windows keeps these, so one made here survives a restart and shows up in Task Scheduler
      under a folder called Sill.
    {/if}
  </Instead>
</Section>

<Section
  label="Add a trigger"
  description="Only actions that never stop to ask can be put on a schedule. A trigger fires when nobody is at the machine, so anything that would want an answer first has nobody to get one from."
  bare
>
  <div class="form">
    <label class="field">
      <span>Name</span>
      <input bind:value={name} placeholder="Morning notes" spellcheck="false" />
    </label>

    <label class="field">
      <span>Do this</span>
      <Select
        value={action}
        options={offers.map((offer) => ({ value: offer.id, label: offer.title }))}
        onchange={(value) => (action = value)}
        ariaLabel="Which action the trigger runs"
        full
      />
    </label>

    <label class="field">
      <span>To this</span>
      <input
        bind:value={target}
        placeholder="C:\Users\you\notes.txt"
        spellcheck="false"
        autocomplete="off"
      />
    </label>

    <div class="pair">
      <label class="field">
        <span>When</span>
        <Select
          value={schedule}
          options={SCHEDULES}
          onchange={(value) => (schedule = value as typeof schedule)}
          ariaLabel="When the trigger runs"
          full
        />
      </label>

      {#if schedule === "daily"}
        <label class="field narrow">
          <span>Time</span>
          <input type="time" bind:value={time} />
        </label>
      {/if}
    </div>

    {#if when}
      <p class="note">Windows will run this {said(when)}.</p>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}
    {#if note && !error}<p class="note">{note}</p>{/if}

    <div class="actions">
      <Button label="Add" onclick={() => void add()} busy={busy} />
    </div>
  </div>
</Section>

<Section
  label="Where these live"
  description="A trigger is a real scheduled task, in a folder called Sill. It is visible in Task Scheduler, it keeps running whether Sill is open or not, and removing Sill does not remove it."
>
  <Row
    title="Triggers in Task Scheduler"
    description="Open Task Scheduler and look under Task Scheduler Library, then Sill. Anything in there was put there from this panel, and anything removed there stops showing up here."
  />
</Section>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .pair {
    display: flex;
    gap: var(--space-3);
  }

  .field {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: var(--space-1);
  }

  .narrow {
    flex: 0 0 auto;
  }

  .field span {
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  input {
    padding: var(--space-2) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-body);
  }

  input:focus-visible {
    box-shadow: var(--ring-strong);
  }

  .actions {
    display: flex;
    gap: var(--space-2);
  }

  .right {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .next,
  .off {
    font-size: var(--text-meta);
    color: var(--text-2);
    white-space: nowrap;
  }

  .off {
    color: var(--danger);
  }

  .note {
    margin: 0;
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  .error {
    margin: 0;
    font-size: var(--text-meta);
    color: var(--danger);
  }
</style>
