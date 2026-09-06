<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import Toggle from "../Toggle.svelte";
  import TextField from "./TextField.svelte";
  import Segmented from "./Segmented.svelte";
  import TermList from "./TermList.svelte";
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

  /**
   * Takes a typed coordinate, if it is one.
   *
   * A half-typed number is not an error to report, it is somebody in the
   * middle of typing: "-" and "45." both parse to nothing and are simply not
   * saved yet. What is refused is a value outside the world, because a
   * latitude of 200 is not a place and the forecast for it is a confusing
   * failure rather than an obvious one.
   */
  function setCoordinate(which: "latitude" | "longitude", typed: string) {
    const value = Number(typed.trim());
    if (typed.trim() === "" || Number.isNaN(value)) return;

    const limit = which === "latitude" ? 90 : 180;
    if (Math.abs(value) > limit) {
      said = `A ${which} runs from -${limit} to ${limit}.`;
      return;
    }

    said = "";
    prefs.widgets.place[which] = value;
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

<!--
  One section per widget, in the order the launcher draws them.

  Each opens with the switch that pins it and carries its own settings under
  that switch, so the reader never has to scroll to find out whether a widget
  has any. It used to be one list of switches and then three unrelated
  sections, and "This machine" had none, which could only be learned by
  looking for it.
-->
<!-- indexed as "Clock", "World clock", "Weather", "This machine" -->
{#each WIDGETS as widget (widget.id)}
  <Section
    label={widget.name}
    description={widget.id === "weather"
      ? "Only a latitude and a longitude are ever sent, and only for the place you name here. Sill does not ask the machine where it is."
      : widget.id === "worldclock"
        ? "Cities by the names Windows lists them under: Tokyo, Paris, Eastern. Their clocks tick on this machine's own time, so nothing is asked of anything while they are shown."
        : undefined}
  >
    <Row
      title={widget.name}
      description={`${widget.blurb}. A pinned widget rides along the bottom of the launcher, so it is there every time you summon it.`}
    >
      {#snippet control()}
        <Toggle
          checked={pinned.has(widget.id)}
          onchange={(on) => pin(widget.id, on)}
          label={`Pin ${widget.name}`}
        />
      {/snippet}
    </Row>

    {#if widget.id === "clock"}
      <!-- not a setting: one pinned widget, drawn again for every other one -->
      <Row
        title="Count the seconds"
        description="Off by default, and that is about cost rather than taste: seconds mean a redraw every second for as long as the launcher is open."
        disabled={!pinned.has("clock")}
      >
        {#snippet control()}
          <Toggle
            bind:checked={prefs.widgets.seconds}
            onchange={() => commit()}
            label="Count the seconds"
          />
        {/snippet}
      </Row>
    {:else if widget.id === "worldclock"}
      <Row
        title="World clock cities"
        description="Type a city and press Enter. One that Windows does not list shows a dash rather than a guess."
        disabled={!pinned.has("worldclock")}
      >
        {#snippet children()}
          <TermList
            bind:terms={prefs.widgets.clocks}
            onchange={() => commit()}
            placeholder="A city, then Enter"
            ariaLabel="A city for the world clock"
            removeLabel={(city) => `Remove ${city}`}
          />
        {/snippet}
      </Row>
    {:else if widget.id === "weather"}
      <Row
        title="Where the weather is for"
        description={place.name
          ? `Currently ${place.name}${place.region ? `, ${place.region}` : ""}.`
          : "Nothing set yet, so the weather widget has nowhere to report on."}
        disabled={!pinned.has("weather")}
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

      <!--
        Editable, because a search box is a guess and coordinates are not.
        Somebody who lives between two towns, or wants the weather where they
        are going, or was handed the wrong Portland, needs to be able to say
        exactly where rather than keep retyping a name at a geocoder until it
        agrees with them.
      -->
      <!-- not a setting: one pinned widget, drawn again for every other one -->
      <Row
        title="Coordinates"
        description="Set directly if the search found the wrong place. Positive is north and east."
        disabled={!pinned.has("weather")}
      >
        {#snippet control()}
          <div class="pair">
            <TextField
              value={place.latitude ? String(place.latitude) : ""}
              oninput={(next) => setCoordinate("latitude", next)}
              placeholder="45.5235"
              ariaLabel="Latitude"
              mono
            />
            <TextField
              value={place.longitude ? String(place.longitude) : ""}
              oninput={(next) => setCoordinate("longitude", next)}
              placeholder="-122.6762"
              ariaLabel="Longitude"
              mono
            />
          </div>
        {/snippet}
      </Row>

      <!-- not a setting: one pinned widget, drawn again for every other one -->
      <Row
        title="Called"
        description="What the widget shows underneath the temperature."
        disabled={!pinned.has("weather")}
      >
        {#snippet control()}
          <TextField
            value={place.name}
            oninput={(next) => {
              prefs.widgets.place.name = next;
              commit();
            }}
            placeholder="Anywhere"
            ariaLabel="Called"
          />
        {/snippet}
      </Row>

      <!-- not a setting: one pinned widget, drawn again for every other one -->
      <Row title="Degrees" disabled={!pinned.has("weather")}>
        {#snippet control()}
          <Segmented
            label="Degrees"
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
    {/if}
  </Section>
{/each}
<style>
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-2);
  }

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
