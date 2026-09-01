<script lang="ts">
  /**
   * What each extension can reach, and how to take it back.
   *
   * The audit's second leg: every launcher in this category runs extension
   * code with the user's full privileges and none of them will tell you what
   * that code can touch. This is the screen that answers it.
   *
   * The wording of a permission comes from Rust, not from a table here, so the
   * line somebody reads when revoking is the same line the card asked them to
   * agree to. Two spellings of one permission is how a person ends up thinking
   * they turned off something they did not.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Section from "./Section.svelte";
  import Button from "./Button.svelte";

  type Permission = { capability: string; plainly: string };
  type GrantedTo = { extension: string; permissions: Permission[] };

  let granted = $state<GrantedTo[]>([]);
  let said = $state("");

  async function refresh() {
    try {
      granted = await invoke<GrantedTo[]>("extension_grants");
      said = "";
    } catch (err) {
      said = `${err}`;
    }
  }

  async function revoke(extension: string, capability: string) {
    try {
      await invoke("revoke_extension_grant", { extension, capability });
    } catch (err) {
      said = `${err}`;
    } finally {
      await refresh();
    }
  }

  onMount(refresh);

  /** Extensions that hold at least one permission, which is all this lists. */
  const holding = $derived(granted.filter((one) => one.permissions.length > 0));
</script>

<Section
  label="Permissions"
  description="An extension is asked the first time it tries to reach something outside its own window, and the answer is kept. Taking one back means it is asked again next time, not that it is refused for good."
>
  {#if said}
    <p class="said">{said}</p>
  {/if}

  {#if holding.length === 0}
    <p class="none">
      Nothing has been granted yet. Extensions that only draw their own view never ask.
    </p>
  {:else}
    <ul class="list">
      {#each holding as one (one.extension)}
        <li class="one">
          <p class="name">{one.extension}</p>
          <ul class="permissions">
            {#each one.permissions as permission (permission.capability)}
              <li class="permission">
                <span class="what">Can {permission.plainly}</span>
                <Button
                  label="Revoke"
                  tone="danger"
                  onclick={() => revoke(one.extension, permission.capability)}
                />
              </li>
            {/each}
          </ul>
        </li>
      {/each}
    </ul>
  {/if}
</Section>

<style>
  .list,
  .permissions {
    display: flex;
    flex-direction: column;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .list {
    gap: var(--space-4);
  }

  .permissions {
    gap: var(--space-2);
  }

  .one {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .name {
    margin: 0;
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: 600;
  }

  .permission {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .what {
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .none,
  .said {
    margin: 0;
    color: var(--text-3);
    font-size: var(--text-meta);
  }
</style>
