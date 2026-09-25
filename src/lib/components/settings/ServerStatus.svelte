<script lang="ts">
  import Button from "./Button.svelte";
  import { formatBytes, releaseOf, type LocalSetupStatus } from "$lib/dictation";

  interface Props {
    status: LocalSetupStatus | null;
    /** Set while the engine or a model is being fetched. */
    installing: boolean;
    /** The stage line from `dictation:setup`, when one is arriving. */
    stage: string;
    /** 0 to 1 while something is downloading, otherwise null. */
    progress: number | null;
    oninstall: () => void;
    onstop: () => void;
    /** `retry` is set when the build on offer already failed here once. */
    onupdate: (retry: boolean) => void;
  }

  let { status, installing, stage, progress, oninstall, onstop, onupdate }: Props = $props();

  const update = $derived(status?.engine.update ?? null);
  const rejected = $derived(status?.engine.rejected ?? null);

  type State = "missing" | "ready" | "running" | "working";

  const state = $derived.by((): State => {
    if (installing) return "working";
    if (status?.server) return "running";
    if (status?.engineInstalled && status?.modelInstalled) return "ready";
    return "missing";
  });

  const HEADLINE: Record<State, string> = {
    running: "Live",
    ready: "Ready",
    working: "Installing",
    missing: "Not installed",
  };

  const DETAIL: Record<State, string> = {
    running: "The model is loaded and answering on this machine.",
    ready: "The server starts on the first dictation, then stays warm.",
    working: "This runs once. Nothing leaves the machine afterwards.",
    missing: "Dictation runs whisper.cpp locally, so audio never leaves the machine.",
  };

  /** "4m", "2h 11m". Seconds only while it is genuinely seconds old. */
  function duration(seconds: number): string {
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m`;
    return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
  }

  /** How long until the idle server shuts itself down. */
  const shutsDownIn = $derived.by(() => {
    if (!status?.server) return null;
    const left = status.server.idleTimeoutSeconds - status.server.idleSeconds;
    return left > 0 ? duration(left) : null;
  });
</script>

<div class="card" data-state={state}>
  {#if update && !installing && status?.engineVersion}
    <!--
      First in the card, not last. At the foot of the facts it read as one
      more fact, and a newer engine is the one thing on this card that asks
      somebody to do something.
    -->
    <div class="update" class:rejected>
      <div class="update-titles">
        <span class="update-headline">
          {rejected
            ? `whisper.cpp ${releaseOf(update.version)} did not work on this machine`
            : `whisper.cpp ${releaseOf(update.version)} is available`}
        </span>
        <span class="update-detail" id="engine-update">
          {#if rejected}
            Still on {releaseOf(status.engineVersion)}. {rejected.reason}
          {:else}
            You have {releaseOf(status.engineVersion)}. Dictation keeps working on it until the
            new engine has downloaded, passed its checks and loaded the model.
          {/if}
        </span>
      </div>
      <button
        type="button"
        class="update-button"
        aria-describedby="engine-update"
        onclick={() => onupdate(rejected !== null)}
      >
        {rejected ? "Try again" : `Update to ${releaseOf(update.version)}`}
        <span class="size">{formatBytes(update.bytes)}</span>
      </button>
    </div>
  {/if}

  <div class="head">
    <span class="beacon" aria-hidden="true"></span>

    <div class="titles">
      <span class="headline">{HEADLINE[state]}</span>
      <span class="detail">{stage || DETAIL[state]}</span>
    </div>

    <div class="actions">
      {#if state === "running"}
        <Button label="Stop" tone="danger" onclick={onstop} />
      {:else if state === "ready"}
        <Button label="Start now" busy={installing} onclick={oninstall} />
      {:else}
        <Button
          label={installing
            ? "Installing"
            : `Install (${formatBytes(status?.downloadBytes ?? 0)})`}
          busy={installing}
          onclick={oninstall}
        />
      {/if}
    </div>
  </div>

  {#if progress !== null}
    <div
      class="bar"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(progress * 100)}
    >
      <div class="fill" style:width="{Math.round(progress * 100)}%"></div>
    </div>
  {/if}

  {#if status}
    <dl class="facts">
      <div>
        <dt>Model</dt>
        <dd>{status.modelLabel}</dd>
      </div>
      <div>
        <dt>Memory</dt>
        <dd>
          {status.server
            ? formatBytes(status.server.memoryBytes)
            : `~${formatBytes(status.modelMemoryBytes)} when loaded`}
        </dd>
      </div>
      {#if status.server}
        <div>
          <dt>Address</dt>
          <dd class="mono">127.0.0.1:{status.server.port}</dd>
        </div>
        <div>
          <dt>Up for</dt>
          <dd>{duration(status.server.uptimeSeconds)}</dd>
        </div>
        {#if shutsDownIn}
          <div>
            <dt>Sleeps in</dt>
            <dd>{shutsDownIn}</dd>
          </div>
        {/if}
      {/if}
      {#if status.engineVersion}
        <div>
          <dt>Engine</dt>
          <dd class="mono">{status.engineVersion}</dd>
        </div>
      {/if}
    </dl>

  {/if}
</div>

<style>
  .card {
    padding: var(--space-4) var(--space-4);
    border-radius: var(--radius-lg);
    background: var(--fill-0);
    box-shadow: var(--bevel-tile);
    transition:
      background-color var(--motion-travel) var(--ease),
      box-shadow var(--motion-travel) var(--ease);
  }

  .card[data-state="running"],
  .card[data-state="working"] {
    background-color: var(--fill-1);
    box-shadow: var(--bevel-tile), var(--ring-fill);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  /*
    The one deliberate garnish in the panel. A server holding half a gigabyte
    of model in memory is the only genuinely live thing Sill runs, and a word
    alone does not carry that.
  */
  .beacon {
    position: relative;
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--text-3);
    transition: background-color var(--motion-travel) var(--ease);
  }

  .card[data-state="ready"] .beacon {
    background: var(--accent);
  }

  .card[data-state="running"] .beacon,
  .card[data-state="working"] .beacon {
    background: var(--accent-bright);
  }

  /* The halo, not the dot, does the pulsing: a dot that changes size drags
     the text beside it around. */
  .card[data-state="running"] .beacon::after,
  .card[data-state="working"] .beacon::after {
    content: "";
    position: absolute;
    inset: -5px;
    border-radius: 50%;
    background: var(--accent-bright);
    animation: pulse var(--motion-pulse-slow) ease-out infinite;
  }

  @keyframes pulse {
    0% {
      opacity: 0.35;
      transform: scale(0.6);
    }
    70% {
      opacity: 0;
      transform: scale(1.25);
    }
    100% {
      opacity: 0;
      transform: scale(1.25);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .card[data-state="running"] .beacon::after,
    .card[data-state="working"] .beacon::after {
      animation: none;
      opacity: 0.25;
    }
  }

  .titles {
    flex: 1;
    min-width: 0;
  }

  .headline {
    display: block;
    font-size: var(--text-body);
    font-weight: var(--weight-strong);
  }

  .detail {
    display: block;
    margin-top: var(--space-half);
    max-width: 62ch;
    font-size: var(--text-meta);
    line-height: 1.5;
    color: var(--text-2);
  }

  .actions {
    flex: none;
  }

  .bar {
    height: 4px;
    margin-top: var(--space-4);
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    border-radius: var(--radius-pill);
    background: var(--accent);
    transition: width var(--motion-travel) linear;
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-6);
    margin: var(--space-4) 0 0;
    padding-top: var(--space-3);
    border-top: 1px solid var(--hairline);
  }

  .facts div {
    min-width: 0;
  }

  dt {
    font-size: var(--text-micro);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--text-3);
  }

  dd {
    margin: var(--space-half) 0 0;
    font-size: var(--text-body);
    /* Fixed width digits, so a counter that ticks does not shuffle the row. */
    font-variant-numeric: tabular-nums;
  }

  .mono {
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  /*
   * The engine update, in the same tinted pane Sill's own update wears in the
   * launcher chin, so "there is a newer one" looks the same wherever it is said.
   */
  .update {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin-bottom: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-lg);
    background-color: var(--info-fill);
    background-image: var(--sheen);
  }

  .update.rejected {
    background-color: var(--fill-1);
  }

  .update-titles {
    flex: 1;
    min-width: 0;
  }

  .update-headline {
    display: block;
    font-size: var(--text-body);
    font-weight: var(--weight-strong);
    color: var(--info);
  }

  .update.rejected .update-headline {
    color: var(--text-1);
  }

  .update-detail {
    display: block;
    margin-top: var(--space-half);
    max-width: 62ch;
    font-size: var(--text-meta);
    line-height: 1.5;
    color: var(--text-2);
  }

  /* The chin's update pill: light through a tinted pane, for a control that
     has to be found. */
  .update-button {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
    height: var(--control-height);
    padding: 0 var(--space-4);
    border: 0;
    border-radius: var(--radius-lg);
    background-color: var(--info-fill-strong);
    background-image: var(--sheen);
    box-shadow: var(--info-lit);
    color: var(--info);
    font: inherit;
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
    white-space: nowrap;
    cursor: pointer;
    transition: color var(--motion-state) var(--ease);
  }

  .update-button:hover {
    color: var(--text-1);
  }

  .update-button:focus-visible {
    outline: none;
    box-shadow: var(--info-lit), var(--focus-ring-gapped);
  }

  .update-button .size {
    font-weight: normal;
    color: var(--text-2);
  }
</style>
