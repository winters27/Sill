<script module lang="ts">
  /** A script that is running, or the last one that ran. */
  export interface Ran {
    job: string;
    title: string;
    running: boolean;
    stdout: string;
    stderr: string;
    code: number | null;
    ended: "finished" | "timedOut" | "cancelled" | "started";
  }
</script>

<script lang="ts">
  /**
   * What a script printed.
   *
   * For `fullOutput` that is the answer rather than a description of where the
   * answer is, and it stays on screen once the script has finished, because
   * somebody ran it deliberately to read it.
   *
   * ## Why the job is not held here
   *
   * The window owns the job. Escape stops a running script before it leaves,
   * the finish arrives on an event the page is already listening for, and
   * starting one is a branch of the Enter chain. All three are the launcher's
   * business rather than this block's, so this draws what it is given and
   * nothing else.
   */

  interface Props {
    output: Ran;
  }

  let { output }: Props = $props();
</script>

<div class="output">
  <p class="output-said">
    {#if output.running}
      Running {output.title}. Escape stops it.
    {:else if output.ended === "cancelled"}
      {output.title} was stopped.
    {:else if output.ended === "timedOut"}
      {output.title} ran too long and was stopped.
    {:else if output.ended === "started"}
      <!-- Before the exit code, because there is not one. Windows started
           it as administrator and a process at that level hands nothing
           back to one below it: no output, no code, and no way to stop
           it. Saying "finished" here would be claiming to know it
           worked. -->
      {output.title} was started as administrator. Sill cannot see what it does.
    {:else if output.code !== 0}
      {output.title} failed with code {output.code}.
    {:else}
      {output.title} finished.
    {/if}
  </p>

  {#if output.stdout.trim()}
    <pre class="output-text sill-scrolls">{output.stdout}</pre>
  {/if}

  {#if output.stderr.trim()}
    <!-- Kept apart from the output rather than mixed into it. A script
         that printed a result and a warning has said two things, and
         running them together loses which was which. -->
    <pre class="output-text output-wrong sill-scrolls">{output.stderr}</pre>
  {/if}

  <!-- Not for an elevated start, which printed nothing here because Sill
       was never holding its output, rather than because it was quiet. -->
  {#if !output.running && output.ended !== "started" && !output.stdout.trim() && !output.stderr.trim()}
    <p class="output-said">It printed nothing.</p>
  {/if}
</div>

<style>
  .output {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    overflow: hidden;
  }

  .output-said {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .output-text {
    max-height: 40vh;
    margin: 0;
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    line-height: 1.5;
    overflow: auto;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .output-wrong {
    color: var(--text-2);
  }
</style>
