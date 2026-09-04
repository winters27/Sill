<script lang="ts">
  /**
   * MCP servers, and which of their tools appear in the action panel.
   *
   * Nothing here is a connection. Opening this panel reads what is already in
   * the preferences and starts nothing, which is the same promise the action
   * panel makes: a server that is dead, slow or uninstalled costs nothing
   * until somebody asks it for something.
   *
   * **Check is the one control that starts a program**, and it is a button
   * somebody presses about the one server they are working on. It sends the
   * form as it is on screen rather than what was last saved, because the point
   * of pressing it is to find out whether what was just typed works.
   */
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import Select from "./Select.svelte";
  import Instead from "../Instead.svelte";
  import { standing } from "$lib/instead";
  import {
    KINDS,
    actionId,
    joined,
    mcpTools,
    split,
    type McpServer,
    type McpTool,
  } from "$lib/mcp";
  import type { Preferences } from "$lib/settings";

  interface Props {
    prefs: Preferences;
    commit: () => void;
  }

  let { prefs, commit }: Props = $props();

  /** What each server last said it could do, keyed by its name. */
  let offered = $state<Record<string, McpTool[]>>({});
  /** What went wrong with which server, keyed the same way. */
  let trouble = $state<Record<string, string>>({});
  /** The one being asked right now, so only its own button says so. */
  let checking = $state("");
  /** The one whose Remove has been pressed once and is asking. */
  let confirming = $state("");

  const servers = $derived(prefs.mcp.servers);

  function change(index: number, edit: (server: McpServer) => void) {
    edit(prefs.mcp.servers[index]);
    // Rust replaces the whole contributed list on a save, so an edit here
    // takes effect the moment it lands rather than at the next restart.
    commit();
  }

  function add() {
    prefs.mcp.servers = [
      ...servers,
      { name: "", command: "", args: [], actions: [] },
    ];
    commit();
  }

  function remove(server: McpServer) {
    if (confirming !== server.name) {
      confirming = server.name;
      return;
    }

    confirming = "";
    prefs.mcp.servers = servers.filter((one) => one !== server);
    commit();
  }

  function addAction(index: number) {
    change(index, (server) => {
      server.actions = [
        ...server.actions,
        { tool: "", title: "", actsOn: ["file"], argument: "path" },
      ];
    });
  }

  function removeAction(index: number, at: number) {
    change(index, (server) => {
      server.actions = server.actions.filter((_, which) => which !== at);
    });
  }

  /**
   * Asks one server what it has.
   *
   * The answer fills a picker rather than being written anywhere, because it
   * is a fact about the server at this moment and not a setting: a server
   * updated tomorrow has different tools, and a copy of today's list kept in
   * the preferences would be a second answer to a question the server already
   * answers.
   */
  async function check(server: McpServer) {
    checking = server.name;
    trouble = { ...trouble, [server.name]: "" };

    try {
      offered = { ...offered, [server.name]: await mcpTools(server) };
    } catch (err) {
      trouble = { ...trouble, [server.name]: `${err}` };
    } finally {
      checking = "";
    }
  }

  function toggleKind(index: number, at: number, kind: string, on: boolean) {
    change(index, (server) => {
      const was = server.actions[at].actsOn.filter((one) => one !== kind);
      server.actions[at].actsOn = on ? [...was, kind] : was;
    });
  }
</script>

<Section
  label="Servers"
  description="An MCP server is a program you already have. Sill starts it when you run one of its actions and stops it again as soon as the answer comes back, so a server listed here costs nothing while you are not using it."
>
  {#each servers as server, index (index)}
    <div class="server">
      <div class="form">
        <label class="field">
          <span>Name</span>
          <input
            value={server.name}
            oninput={(event) =>
              change(index, (one) => (one.name = event.currentTarget.value))}
            placeholder="notes"
            spellcheck="false"
            autocomplete="off"
          />
        </label>

        <label class="field">
          <span>Command</span>
          <input
            value={joined(server)}
            oninput={(event) =>
              change(index, (one) => {
                const parts = split(event.currentTarget.value);
                one.command = parts.command;
                one.args = parts.args;
              })}
            placeholder="npx -y @modelcontextprotocol/server-filesystem C:\Notes"
            spellcheck="false"
            autocomplete="off"
          />
        </label>

        <div class="actions">
          <Button
            label="Check"
            busy={checking === server.name}
            onclick={() => void check(server)}
          />
          <Button
            label={confirming === server.name ? "Remove it?" : "Remove"}
            tone="danger"
            onclick={() => remove(server)}
          />
        </div>

        {#if trouble[server.name]}
          <p class="error">{trouble[server.name]}</p>
        {:else if offered[server.name]}
          <p class="note">
            {offered[server.name].length} tool{offered[server.name].length === 1 ? "" : "s"}:
            {offered[server.name].map((tool) => tool.name).join(", ")}
          </p>
        {/if}
      </div>

      {#each server.actions as declared, at (at)}
        <div class="declared">
          <div class="pair">
            <label class="field">
              <span>Tool</span>
              {#if offered[server.name]?.length}
                <Select
                  value={declared.tool}
                  options={offered[server.name].map((tool) => ({
                    value: tool.name,
                    label: tool.name,
                  }))}
                  onchange={(value) =>
                    change(index, (one) => (one.actions[at].tool = value))}
                  ariaLabel="Which tool this action runs"
                  full
                />
              {:else}
                <input
                  value={declared.tool}
                  oninput={(event) =>
                    change(index, (one) => (one.actions[at].tool = event.currentTarget.value))}
                  placeholder="read_file"
                  spellcheck="false"
                  autocomplete="off"
                />
              {/if}
            </label>

            <label class="field">
              <span>Shown as</span>
              <input
                value={declared.title}
                oninput={(event) =>
                  change(index, (one) => (one.actions[at].title = event.currentTarget.value))}
                placeholder={declared.tool || "Summarise"}
                spellcheck="false"
              />
            </label>

            <label class="field narrow">
              <span>Passed as</span>
              <input
                value={declared.argument}
                oninput={(event) =>
                  change(index, (one) => (one.actions[at].argument = event.currentTarget.value))}
                placeholder="path"
                spellcheck="false"
                autocomplete="off"
              />
            </label>
          </div>

          <div class="kinds">
            {#each KINDS as kind (kind.value)}
              <label class="kind">
                <input
                  type="checkbox"
                  checked={declared.actsOn.includes(kind.value)}
                  onchange={(event) =>
                    toggleKind(index, at, kind.value, event.currentTarget.checked)}
                />
                <span>{kind.label}</span>
              </label>
            {/each}
          </div>

          <div class="actions">
            <p class="note">
              {#if declared.tool && server.name}
                Runs as <code>{actionId(server, declared)}</code>, which is how a keyboard
                shortcut names it.
              {:else}
                Give the server a name and pick a tool, and this appears in the action panel.
              {/if}
            </p>
            <Button label="Remove" tone="danger" onclick={() => removeAction(index, at)} />
          </div>
        </div>
      {/each}

      <div class="actions">
        <Button label="Add an action" onclick={() => addAction(index)} />
      </div>
    </div>
  {/each}

  <Instead
    tone={standing({ failed: false, loading: false, count: servers.length })}
    inline
    headline="No MCP servers yet"
  >
    Add one and Sill offers its tools in the action panel, beside everything Sill
    itself can do to a file.
  </Instead>

  <div class="actions">
    <Button label="Add a server" onclick={add} />
  </div>
</Section>

<Section
  label="What running one costs"
  description="Sill starts the program, asks it the one thing, reads the answer and closes it. Nothing is held open between actions, so a server you have not used today has not been started today."
>
  <!-- not a setting: a reading of what the design guarantees, not a switch. -->
  <Row
    title="MCP servers"
    description="A server is started only when you run one of its actions or press Check. If it does not answer, Sill stops waiting, closes it, and says which one it was."
  />
  <!-- not a setting: an explanation of a rule, with nothing to change. -->
  <Row
    title="Actions from a server"
    description="An action from a server counts as running a program, because that is what it does. So a scheduled trigger cannot use one, and Sill's own AI has to prove somebody is at the machine before it can."
  />
  <!-- not a setting: a reading of what a call costs, not a switch. -->
  <Row
    title="Starting one takes a moment"
    description="A call is a whole process start, every time. Naming the program directly is around a second; going through npx is five or six, because npx resolves the package name again on every start."
  />
</Section>

<style>
  .server {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) 0;
  }

  .server + .server {
    border-top: 1px solid var(--hairline);
  }

  .form,
  .declared {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .declared {
    padding-left: var(--space-3);
    border-left: 1px solid var(--hairline);
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
    flex: 0 0 8rem;
  }

  .field span {
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  input:not([type]) {
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

  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
  }

  .kind {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .note {
    margin: 0;
    flex: 1;
    font-size: var(--text-meta);
    color: var(--text-2);
  }

  code {
    font-family: var(--font-mono);
  }

  .error {
    margin: 0;
    font-size: var(--text-meta);
    color: var(--danger);
  }
</style>
