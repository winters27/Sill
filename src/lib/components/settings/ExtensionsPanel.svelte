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
  import Select from "./Select.svelte";
  import TextField from "./TextField.svelte";
  import Instead from "../Instead.svelte";
  import ExtensionCosts from "./ExtensionCosts.svelte";
  import { standing } from "$lib/instead";
  import type { CostRow } from "$lib/costs";
  import {
    extensionPreferences,
    extensionResources,
    grantPermission,
    installedExtensions,
    revokePermission,
    setExtensionPreference,
    shortRevision,
    storeUninstall,
    type ExtensionCost,
    type ExtensionPreference,
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

  /**
   * The settings each extension declares, keyed by its name.
   *
   * Read alongside the list rather than on demand, because the alternative is
   * a section that appears a moment after the one above it and moves the page
   * under whoever is reading it.
   */
  let settings = $state<Record<string, ExtensionPreference[]>>({});

  /**
   * What each extension has cost this run, dearest first.
   *
   * Read alongside the list rather than on a timer. Nothing here changes
   * unless somebody opens an extension, and a panel that re-asked every few
   * seconds would be waking a Node process to be told the same thing, on a
   * screen that is open for a minute at a time.
   */
  let costs = $state<ExtensionCost[]>([]);

  /** The same, with the names people call the extensions by. */
  const rows = $derived<CostRow[]>(
    costs.map((cost) => ({
      extension: cost.extension,
      title:
        installed.find((one) => one.extension === cost.extension)?.title ?? cost.extension,
      cost,
    })),
  );

  async function refresh() {
    loading = true;
    try {
      installed = await installedExtensions();
      // After the list, because a row is named from it. A failure here is not
      // worth losing the panel over: the readings are the one part of this
      // screen nothing depends on.
      costs = await extensionResources().catch(() => []);
      settings = Object.fromEntries(
        await Promise.all(
          installed.map(
            async (one) => [one.extension, await extensionPreferences(one.extension)] as const,
          ),
        ),
      );
    } catch (err) {
      status = `${err}`;
    } finally {
      loading = false;
    }
  }

  /**
   * Saves one setting.
   *
   * On change rather than on every keystroke, so a text field writes once when
   * it is left instead of once per character. Nothing is held here waiting to
   * be flushed: a debounced write with an unmount to remember is how a setting
   * gets lost, and this window can be closed at any moment.
   */
  async function setPreference(
    extension: string,
    preference: ExtensionPreference,
    value: unknown,
  ) {
    try {
      await setExtensionPreference(extension, preference.command, preference.name, value);
      settings[extension] = await extensionPreferences(extension);
    } catch (err) {
      status = `${err}`;
    }
  }

  /** What a row says under its title. */
  function about(preference: ExtensionPreference): string {
    const parts = [preference.description];

    if (preference.required && preference.isDefault) {
      parts.push("Required. Commands that need it will not start until it is set.");
    }
    if (preference.commandTitle) {
      parts.push(`Only for ${preference.commandTitle}.`);
    }

    return parts.filter(Boolean).join(" ");
  }

  /** A preference's value as a string, for the controls that take one. */
  function asText(preference: ExtensionPreference): string {
    const value = preference.value;
    if (value === null || value === undefined) return "";
    return typeof value === "string" ? value : JSON.stringify(value);
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
      // What Rust said, rather than what this hoped. The removal is a registry
      // action now and it reports whether there was anything there to remove.
      status = await storeUninstall(one.extension);
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
  <!-- not a setting: a reading of whether Node was found, not a control -->
  <Row
    title="Node.js"
    description={nodeInstalled
      ? "Found. Extensions can run."
      : "Not found, so no extension can start. Get it from nodejs.org, or run: winget install OpenJS.NodeJS.LTS"}
  />
</Section>

<!--
  What they cost, which is the one thing nothing here could answer before.

  Nothing is measured on a schedule. Openings are timed as they happen, on a
  path a person triggers, and the memory is asked of the extension runtime when
  this screen is drawn. With the window shut, none of it costs anything.
-->
<ExtensionCosts {rows} />

{#if status}
  <p class="said">{status}</p>
{/if}

{#if installed.length === 0}
  <Section label="Installed" bare>
    <Instead
      tone={standing({ failed: false, loading, count: installed.length })}
      inline
      headline={loading ? "Reading what is installed" : "Nothing installed yet"}
      hint={loading
        ? ""
        : "Open the launcher and search for Extension Store to browse them, or Install Extension to build one from a folder."}
    />
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
      What the extension can be told. Half the store needs an API key to do
      anything at all, and until this existed the answer to "set your token in
      preferences" was a screen that did not exist.
    -->
    {#each settings[one.extension] ?? [] as preference (`${preference.command}/${preference.name}`)}
      <Row title={preference.title} description={about(preference)}>
        {#snippet control()}
          {#if preference.kind === "checkbox"}
            <Toggle
              checked={preference.value === true}
              onchange={(next: boolean) => void setPreference(one.extension, preference, next)}
              label={preference.title}
            />
          {:else if preference.kind === "dropdown" && preference.choices.length > 0}
            <Select
              value={asText(preference)}
              options={preference.choices.map((choice) => ({
                value: String(choice.value),
                label: choice.title,
              }))}
              onchange={(next: string) => void setPreference(one.extension, preference, next)}
              steady
              ariaLabel={preference.title}
            />
          {:else if preference.kind === "password"}
            <!--
              Never the value. What is stored is sealed and what arrives here
              is whether anything is set, so the field is blank and typing into
              it replaces what is there.
            -->
            <TextField
              value=""
              secret
              mono
              placeholder={preference.value === true ? "Set" : "Not set"}

              onchange={(next: string) => void setPreference(one.extension, preference, next)}
              ariaLabel={preference.title}
            />
          {:else}
            <TextField
              value={asText(preference)}
              onchange={(next: string) => void setPreference(one.extension, preference, next)}

              ariaLabel={preference.title}
            />
          {/if}
        {/snippet}
      </Row>
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

    <!-- not a setting: one installed extension, drawn again for every other one -->
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
  .said {
    margin: 0;
    padding: var(--space-3);
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: var(--line-meta);
  }
</style>
