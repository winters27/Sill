<script lang="ts">
  /**
   * One note, in one window.
   *
   * The whole of the prototype's interface. There is no list, no sidebar and
   * no second note on screen, because which note is open is a question the
   * launcher already answers: `note` and whatever you remember of it puts the
   * one you want under the cursor, and Enter opens it here.
   *
   * The window is opened on a note by an event Rust sends, and it asks as well
   * on mount. Both, for the reason `Welcome` does: the window is built and the
   * event is sent in that order, so the page may still be loading when the
   * event arrives and would otherwise show an empty note that saving then
   * writes over.
   */
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { forgetNote, readNote, saver, writeNote, type Standing } from "$lib/notes";
  import { applyAppearance, getPreferences } from "$lib/settings";
  import "$lib/theme/theme.css";

  let id = $state("");
  let text = $state("");
  let standing = $state<Standing>("saved");
  let missing = $state(false);
  let area = $state<HTMLTextAreaElement>();
  let unlisten: UnlistenFn[] = [];

  const saving = saver({
    write: async (what) => {
      const saved = await writeNote(id, what);
      // A note made from the New Note row arrives with no id and comes back
      // with one. Without keeping it, the next write would make a second note.
      id = saved.id;
      return saved;
    },
  });

  const says: Record<Standing, string> = {
    saved: "Saved",
    saving: "Saving",
    unsaved: "Not saved yet",
    failed: "Could not save. It will try again.",
  };

  async function open(which: string): Promise<void> {
    id = which;
    const note = await readNote(which);

    // Gone since the launcher offered it, which is not an error worth a
    // dialog: it is one sentence, and the window stays open so nothing typed
    // into it is lost.
    missing = note === null;
    text = note?.text ?? "";
    saving.opened(text);
    standing = saving.standing;

    area?.focus();
  }

  function typed(): void {
    saving.changed(text);
    standing = saving.standing;
  }

  async function settle(): Promise<void> {
    await saving.flush();
    standing = saving.standing;
  }

  /**
   * Removes the note and leaves an empty one in its place.
   *
   * The window stays open rather than closing itself. Closing would need the
   * window permissions this route deliberately does not have, and it is also
   * the wrong answer: somebody who deletes the wrong note wants somewhere to
   * start typing it again, not a window that vanishes.
   */
  async function remove(): Promise<void> {
    saving.stop();

    if (id) await forgetNote(id);

    id = "";
    text = "";
    missing = false;
    saving.opened("");
    standing = saving.standing;
    area?.focus();
  }

  onMount(() => {
    void (async () => {
      applyAppearance(await getPreferences());

      unlisten.push(
        await listen<string>("sill://note", ({ payload }) => {
          // Whatever is on screen goes to disk before the window becomes a
          // different note. Opening a second note is the one moment the
          // debounce would otherwise lose the first one.
          void settle().then(() => open(payload));
        }),
      );
    })();

    /*
     * The window going away, from the title bar or from anywhere else.
     *
     * `beforeunload` rather than `onDestroy`, because a webview that is being
     * torn down does not always run a component's teardown, and the last
     * paragraph somebody typed is exactly what is outstanding at that moment.
     */
    const leaving = () => void settle();
    window.addEventListener("beforeunload", leaving);

    return () => window.removeEventListener("beforeunload", leaving);
  });

  onDestroy(() => {
    void settle();
    for (const off of unlisten) off();
    unlisten = [];
  });
</script>

<main>
  <textarea
    bind:this={area}
    bind:value={text}
    oninput={typed}
    onblur={settle}
    spellcheck="true"
    placeholder="Write something."
    aria-label="Note"
  ></textarea>

  <footer>
    <span class="standing" class:failed={standing === "failed"}>
      {missing ? "That note is gone. What you type here will be a new one." : says[standing]}
    </span>

    <button type="button" onclick={remove} disabled={!id}>Delete</button>
  </footer>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--core-background);
    color: var(--text-1);
  }

  textarea {
    flex: 1;
    resize: none;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text-1);
    font-family: var(--font);
    font-size: var(--text-body);
    line-height: 1.6;
    padding: var(--space-5);
  }

  textarea::placeholder {
    color: var(--text-4);
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-5);
    border-top: 1px solid var(--hairline);
  }

  .standing {
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .standing.failed {
    color: var(--danger);
  }

  button {
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-2);
    cursor: pointer;
    font-family: var(--font);
    font-size: var(--text-meta);
    padding: var(--space-1) var(--space-2);
  }

  button:hover:not(:disabled) {
    background: var(--fill-2);
    color: var(--danger);
  }

  button:disabled {
    cursor: default;
    opacity: var(--opacity-disabled);
  }
</style>
