<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import Toggle from "../Toggle.svelte";
  import TextField from "./TextField.svelte";
  import Segmented from "./Segmented.svelte";
  import { WIDGETS } from "$lib/widgets/registry";
  import type { Place, Preferences } from "$lib/settings";

  interface Props {
    prefs: Preferences;
    commit: () => void;
  }

  let { prefs, commit }: Props = $props();

  let typed = $state("");
  let looking = $state(false);
  let said = $state("");

  const pinned = $derived(new Set(prefs.widgets.pinned));
  const place = $derived(prefs.widgets.place);

  function pin(id: string, on: boolean) {
    const was = prefs.widgets.pinned.filter((one) => one !== id);
    prefs.widgets.pinned = on ? [...was, id] : was;
    commit();
  }

  async function findPlace() {
    if (!typed.trim()) return;

    looking = true;
    said = "";
    try {
      const found = await invoke<Place>("find_place", { name: typed });
      prefs.widgets.place = found;
      commit();
      typed = "";
    } catch (err) {
      said = `${err}`;
    } finally {
      looking = false;
    }
  }
</script>

<Section
  label="In the launcher"
  description="A pinned widget rides along the bottom of the launcher, so it is there every time you summon it without being somewhere you have to go."
>
  {#each WIDGETS as widget (widget.id)}
    <Row title={widget.name} description={widget.blurb}>
      {#snippet control()}
        <Toggle
          checked={pinned.has(widget.id)}
          onchange={(on) => pin(widget.id, on)}
          label={`Pin ${widget.name}`}
        />
      {/snippet}
    </Row>
  {/each}
</Section>

<Section
  label="Weather"
  description="Only a latitude and a longitude are ever sent, and only for the place you name here. Sill does not ask the machine where it is."
>
  <Row
    title="Where"
    description={place.name
      ? `Currently ${place.name}${place.region ? `, ${place.region}` : ""}.`
      : "Nothing set yet, so the weather widget has nowhere to report on."}
  >
    {#snippet control()}
      <div class="finder">
        <TextField
          value={typed}
          oninput={(next) => (typed = next)}
          placeholder="Portland, Zürich, Osaka…"
          ariaLabel="Where"
        />
        <Button
          label={looking ? "Looking" : "Find"}
          busy={looking}
          onclick={findPlace}
        />
      </div>
    {/snippet}
  </Row>

  <Row title="Degrees">
    {#snippet control()}
      <Segmented
        value={prefs.widgets.fahrenheit ? "f" : "c"}
        options={[
          { value: "f", label: "Fahrenheit" },
          { value: "c", label: "Celsius" },
        ]}
        onchange={(next) => {
          prefs.widgets.fahrenheit = next === "f";
          commit();
        }}
      />
    {/snippet}
  </Row>

  {#if said}
    <p class="said">{said}</p>
  {/if}
</Section>

<Section label="Clock">
  <Row
    title="Count the seconds"
    description="Off by default, and that is about cost rather than taste: seconds mean a redraw every second for as long as the launcher is open, and a clock nobody is watching should cost a redraw a minute."
  >
    {#snippet control()}
      <Toggle
        bind:checked={prefs.widgets.seconds}
        onchange={() => commit()}
        label="Count the seconds"
      />
    {/snippet}
  </Row>
</Section>

<style>
  .finder {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .said {
    margin: 0;
    padding: var(--space-2) 0 0;
    color: var(--text-2);
    font-size: var(--text-meta);
  }
</style>
