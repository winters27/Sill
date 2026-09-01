<script lang="ts">
  /**
   * What is installed, what each one runs, and what it is allowed to reach.
   *
   * ## Why this is a panel rather than a list
   *
   * The Extensions panel used to name what was installed and how many commands
   * it had, which is a receipt rather than a screen. Two things made it have to
   * grow up.
   *
   * The permission layer refuses `fs`, `net` and `child_process` at `require`.
   * That happens while a module loads, which is synchronous and has no RPC to
   * hang an approval card on, so an extension needing one **dies before it
   * renders** and the refusal says "Grant it in Settings, under Extensions".
   * Until this existed there was nothing there that could: the commands were
   * to list and to revoke, and nothing granted.
   *
   * And installing from the store now grants what its screen showed, so there
   * has to be somewhere to see what that was and change your mind.
   *
   * ## What a switch here means
   *
   * On is "yes, and stop asking". Off is "ask me again next time", not "never":
   * revoking puts the extension back to being questioned on the card the first
   * time it tries, which is the state everything starts in.
   */
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Toggle from "../Toggle.svelte";
  import Button from "./Button.svelte";
  import {
    grantPermission,
    installedExtensions,
    revokePermission,
    shortRevision,
    storeUninstall,
    type InstalledExtension,
  } from "$lib/store";

  interface Props {
    /** Whether Node is on the machine, which is what runs any of this. */
    nodeInstalled: boolean;
  }

  let { nodeInstalled }: Props = $props();

  let installed = $state<InstalledExtension[]>([]);
  let loading = $state(true);
  let status = $state("");

  async function refresh() {
    loading = true;
    try {
      installed = await installedExtensions();
    } catch (err) {
      status = `${err}`;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void refresh();
  });

  async function setPermission(extension: string, capability: string, granted: boolean) {
    try {
      await (granted ? grantPermission : revokePermission)(extension, capability);
      await refresh();
    } catch (err) {
      status = `${err}`;
    }
  }

  async function remove(one: InstalledExtension) {
    try {
      await storeUninstall(one.extension);
      status = `Removed ${one.title}`;
      await refresh();
    } catch (err) {
      status = `${err}`;
    }
  }

  /** Where it came from, said in one line. */
  function provenance(one: InstalledExtension): string {
    if (one.source === "store") {
      return `From the store, at ${shortRevision(one.revision)}`;
    }
    if (one.source === "folder") return `Built from ${one.path}`;
    // Anything installed before origins were recorded. Saying nothing is
    // right: inventing a source would be a guess presented as a fact.
    return "Nothing recorded where this came from";
  }

  const held = (one: InstalledExtension) => one.permissions.filter((it) => it.granted).length;
</script>

<Section
  label="Runs extensions in"
  description="Extensions are Node programs and Sill runs them in a Node process, so one has to be on the machine. Nothing else in Sill needs it."
>
  <Row
    title="Node.js"
    description={nodeInstalled
      ? "Found. Extensions can run."
      : "Not found, so no extension can start. Get it from nodejs.org, or run: winget install OpenJS.NodeJS.LTS"}
  />
</Section>

{#if status}
  <p class="said">{status}</p>
{/if}

{#if loading && installed.length === 0}
  <Section label="Installed" bare>
    <p class="empty">Reading what is installed…</p>
  </Section>
{:else if installed.length === 0}
  <Section label="Installed" bare>
    <p class="empty">
      Nothing installed yet. Open the launcher and search for Extension Store to browse them, or
      Install Extension to build one from a folder.
    </p>
  </Section>
{/if}

{#each installed as one (one.extension)}
  <Section
    label={one.title}
    description="{one.commands.length} {one.commands.length === 1 ? 'command' : 'commands'} · {provenance(
      one,
    )} · {held(one)} of {one.permissions.length} permissions"
  >
    <!--
      The commands, so this answers "what did installing this actually add"
      without going to the launcher and typing a guess.
    -->
    {#each one.commands as command (command.id)}
      <Row
        title={command.title}
        description={command.runnable
          ? command.subtitle
          : `${command.mode}, which Sill has nowhere to run`}
      />
    {/each}

    <!--
      Permissions as switches rather than a list with a revoke button, because
      the thing somebody comes here to do is turn one ON: they were told to,
      by a refusal they could not otherwise act on.
    -->
    {#each one.permissions as permission (permission.capability)}
      <Row title={permission.plainly}>
        {#snippet control()}
          <Toggle
            checked={permission.granted}
            onchange={(next: boolean) =>
              void setPermission(one.extension, permission.capability, next)}
            label={permission.plainly}
          />
        {/snippet}
      </Row>
    {/each}

    <Row
      title="Remove"
      description="Deletes its commands and forgets every permission it was given."
    >
      {#snippet control()}
        <Button onclick={() => void remove(one)} label="Remove" tone="danger" />
      {/snippet}
    </Row>
  </Section>
{/each}

<style>
  .empty,
  .said {
    margin: 0;
    padding: var(--space-3);
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: var(--line-meta);
  }
</style>
