<script lang="ts">
  /**
   * Browsing the extension store, and deciding whether to install one.
   *
   * Two screens in one component. The list is what somebody browses; the
   * confirmation is what they read before code they did not write arrives on
   * their machine. They are one component because the second is a step of the
   * first rather than a place of its own: there is no route to it, no way to
   * reach it except by asking to install something, and no state in it that
   * outlives the answer.
   *
   * Nothing here filters or ranks. A keystroke sends a query to Rust and draws
   * what comes back, which is the same division the root list already keeps.
   */
  import { onMount, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    ago,
    installs,
    progressFraction,
    progressLine,
    shortRevision,
    storeBrowse,
    storeDiscard,
    storeInstall,
    storePrepare,
    storeReady,
    weight,
    INSTALL_PROGRESS,
    type Browse,
    type InstallProgress,
    type Preparation,
    type StoreRow,
  } from "$lib/store";
  import StoreIcon from "./StoreIcon.svelte";
  import Chord from "./Chord.svelte";
  import ExtIcon from "./ExtIcon.svelte";
  import { storeGallery } from "$lib/store";
  import { GLYPHS } from "./marks";
  import Instead from "./Instead.svelte";
  import { couldNot, noMatch, standing } from "$lib/instead";
  import { LISTBOX, optionId } from "$lib/results";
  import { hint } from "$lib/hint";
  import type { Preferences } from "$lib/settings";
  import type { StoreFilterState } from "$lib/store";

  interface Props {
    /** The launcher's query field drives the search. */
    query: string;
    selected: number;
    onselect: (index: number) => void;
    oncount: (count: number) => void;
    /** Said in the launcher's status line, where every other view says things. */
    onstatus: (said: string) => void;
    /**
     * The listing under the cursor, so the launcher's action panel has one.
     *
     * The panel takes its actions from a row, and a store listing is not one
     * of the ranked results it usually reads: nothing outside this component
     * knows what is selected. Told rather than asked, so the answer arrives
     * with the selection instead of a frame later.
     */
    oncurrent: (row: StoreRow | null) => void;
    /** Refreshes the launcher's own list after an install or a removal. */
    onchanged: () => void;
    /**
     * The filter as it stands, for the control beside the search field.
     *
     * The field and its accessories belong to the launcher, so the store
     * reports what can be filtered and how it is filtered now, and the
     * launcher draws the control and hands a pick back to `pick`.
     */
    onfilter?: (filter: StoreFilterState) => void;
    prefs: Preferences | null;
    /** Opens straight onto what has an update, from the launcher's own row. */
    startOnUpdates?: boolean;
  }

  let {
    query,
    selected,
    onselect,
    oncount,
    onstatus,
    oncurrent,
    onchanged,
    onfilter,
    prefs,
    startOnUpdates = false,
  }: Props = $props();

  /** What the list is narrowed to. Derived scopes, not a hand-written set. */
  type Scope = "all" | "installed" | "updates";

  let browse = $state<Browse | null>(null);
  /*
   * Where the store opens, not what it stays on.
   *
   * The initial value is the whole meaning of the prop: "Update Extensions"
   * opens here on the updates scope and the person looking is then free to
   * change it. Reading it once is deliberate, and `untrack` says so rather
   * than leaving a warning that reads like an oversight.
   */
  let scope = $state<Scope>(untrack(() => startOnUpdates) ? "updates" : "all");
  let category = $state<string | null>(null);
  let loading = $state(true);
  let failed = $state<string | null>(null);

  /**
   * Whether this machine has the Node an extension needs.
   *
   * Browsing works without it and installing does not, so it is a line at the
   * top rather than a refusal to open the store. Asked once on the way in:
   * somebody can install Node while Sill is open, and the answer is only wrong
   * in the direction of saying so when it is no longer true.
   */
  let ready = $state(true);

  /** The install being decided, if one is. */
  let deciding = $state<Preparation | null>(null);

  /**
   * The listing somebody opened, drawn on its own screen.
   *
   * The shelf used to be two panes, the list and a detail column beside it,
   * so every row came with a description, a command list and a version before
   * anybody had asked. Now the list is the list, and opening a row is what
   * brings the rest: the screenshots the store shows, the commands, the
   * version, and the one key that installs it.
   */
  let opened = $state<StoreRow | null>(null);
  /** Its screenshots: not asked yet, or the answer, which may be empty. */
  let gallery = $state<string[] | null>(null);
  /** What is happening, so every surface can say so. */
  let working = $state<string | null>(null);
  /**
   * Which extension it is happening to.
   *
   * Held separately from `working` because the row has to say it too. The
   * first version put the word only on the confirmation screen's button,
   * which does not exist yet while the fetch that builds it is running, so
   * pressing Enter looked like pressing a dead key for several seconds.
   */
  let workingOn = $state<string | null>(null);
  /**
   * The latest thing the install said about itself.
   *
   * npm and esbuild are the whole of the wait and neither said anything until
   * it finished, so a large extension was one word and a spinner for a minute
   * and a half. Their own output is the content: npm names the package it is
   * fetching, esbuild names the file it is on.
   *
   * Kept beside `working` rather than folded into it, so the word stays put
   * and only the detail after it changes. A line that replaces itself entirely
   * several times a second reads as a fault.
   */
  let detail = $state("");

  /**
   * How far along the install is, or nothing before anything has said.
   *
   * Zero is a real position and means "starting", so this is null until the
   * first stage that counts something reports, and null again once the install
   * is over. A bar sitting at zero is a bar that looks stuck.
   */
  let howFar = $state<number | null>(null);

  const rows = $derived(browse?.rows ?? []);
  const current = $derived(rows[selected] ?? null);

  /**
   * Whether the compatibility switch is on.
   *
   * Read from preferences rather than held here, so the store and the settings
   * panel are one value and not two that drift. Rust owns it; this reflects it.
   */
  const windowsOnly = $derived(prefs?.store.windowsOnly ?? true);

  const showing = $derived(standing({ failed: failed !== null, loading, count: rows.length }));

  /**
   * What the list says when it has no rows, in the reader's terms.
   *
   * All three used to be one grey paragraph in one class, so a store that
   * could not be reached, a store still being read and a shelf with nothing on
   * it were the same picture and only the words told them apart. `failed` also
   * carried whatever Rust threw, which is the request that failed rather than
   * the errand somebody was on.
   */
  const saying = $derived(
    showing === "failed"
      ? couldNot("reach the extension store")
      : showing === "loading"
        ? "Reading the extension store"
        : query
          ? noMatch(query, "extensions")
          : "Nothing here",
  );

  /**
   * The second line, when there is one worth having.
   *
   * The hidden count is the reason the shelf looks emptier than it is, so it
   * belongs under the sentence that says the shelf is empty rather than beside
   * it. A failure gets the one thing a reader can actually do about it.
   */
  const alsoSaying = $derived.by(() => {
    if (showing === "failed") return "Check the connection and search again.";
    if (showing !== "empty") return "";

    const hidden = browse?.hidden ?? 0;
    return hidden && windowsOnly
      ? `${hidden} are hidden because they do not say they run on Windows.`
      : "";
  });

  /**
   * Which browse the rows on screen belong to.
   *
   * Two can be in flight at once and they can come back in either order, so
   * without this an older one lands last and puts results for a shorter query
   * under a longer one, which reads as the store ignoring what was typed. The
   * launcher's own search carries the same counter for the same reason.
   */
  let generation = 0;

  /**
   * Asks Rust for a screen.
   *
   * Every input to the query is read here, so changing the scope, the
   * category, the switch or the text all go through one path. A second "and
   * also re-run the search" call site is how two of these end up disagreeing
   * about what is on screen.
   */
  async function load(refresh = false) {
    const asked = ++generation;
    loading = true;

    try {
      const answer = await storeBrowse(
        {
          text: query,
          category,
          installedOnly: scope === "installed",
          updatesOnly: scope === "updates",
          hideBlocked: windowsOnly,
        },
        refresh,
      );

      if (asked !== generation) return;
      browse = answer;
      failed = null;
    } catch (err) {
      if (asked !== generation) return;
      // Kept as the reason, said to the reader as the errand. The console is
      // where the reason belongs: nobody browsing a store can act on a fetch
      // error, and the pane already offers the one thing they can do.
      console.error("[sill] could not reach the extension store", err);
      failed = `${err}`;
      browse = null;
    } finally {
      if (asked === generation) loading = false;
    }
  }

  /*
   * One effect for every input, rather than a handler per control.
   *
   * Reading them all here is what makes the list impossible to leave stale:
   * anything that changes what should be on screen re-runs the query by
   * existing, and there is no second place to remember to call.
   */
  $effect(() => {
    // Named so each dependency is deliberate rather than incidental.
    void query;
    void scope;
    void category;
    void windowsOnly;

    // Nothing to reload behind a screen nobody can see past. Reading this is
    // also what refreshes the list on the way back, which is how a row that
    // was just installed learns that it is.
    if (deciding) return;

    void load();
  });

  $effect(() => {
    oncount(rows.length);
  });

  /*
   * The listing under the cursor, said out loud.
   *
   * The launcher's action panel asks Rust what can be done to the selected
   * row, and it can only ask about a row it has been told about: a store
   * listing is not one of the ranked results the panel usually reads. Same
   * arrangement as the count beside it, and for the same reason.
   */
  $effect(() => {
    oncurrent(opened ?? current);
  });

  $effect(() => {
    onfilter?.({
      scope,
      category,
      categories: browse?.categories ?? [],
      updates: browse?.updates ?? 0,
    });
  });

  /**
   * A pick from the filter control: a scope, or a category, or any category.
   * The values are the ones `StoreFilter` reports, and nothing else is one.
   */
  export function pick(value: string) {
    if (value === "scope:all" || value === "scope:installed" || value === "scope:updates") {
      scope = value.slice(6) as Scope;
    } else if (value === "category:") {
      category = null;
    } else if (value.startsWith("category:")) {
      category = value.slice(9);
    } else {
      return;
    }
    onselect(0);
  }

  onMount(() => {
    void storeReady().then((answer) => (ready = answer));

    // Rust says how an install is going as it goes. Subscribed for the life of
    // the view rather than for the length of one install: attaching a listener
    // per install would race the first line, which arrives before `invoke`
    // has returned anything at all.
    const listening = listen<InstallProgress>(INSTALL_PROGRESS, (event) => {
      const line = progressLine(event.payload);
      if (line) detail = line;

      /*
       * Held rather than replaced by nothing.
       *
       * npm and esbuild report lines and no position, so a bar driven straight
       * off every message would vanish for the longest part of the wait and
       * come back near the end. Keeping the last position means it holds still
       * while npm works, which is the truth: nothing measurable is happening
       * to it.
       */
      const far = progressFraction(event.payload);
      if (far !== null) howFar = far;
    });

    return () => void listening.then((stop) => stop());
  });

  /**
   * Enter on the highlighted row.
   *
   * **Refuses while something is already running, and that is a correctness
   * guard rather than politeness.** Fetching wipes the staging directory
   * before it fills it, so a second Enter arriving during the first one
   * deleted what the first had staged and the install that followed reported
   * "Nothing is staged to install" about an extension it had just fetched.
   *
   * That is easy to do, because fetching takes seconds and the only sign it
   * had started used to be a line in the launcher's status bar.
   */
  export async function activate() {
    if (working) return;

    if (deciding) {
      await accept();
      return;
    }

    /*
     * There is nothing to fetch for a row the store does not carry.
     *
     * These rows are here because they are installed, not because the index
     * lists them, so `storePrepare` would go looking for a catalogue entry
     * that does not exist and come back with "the store has no extension
     * called my-notes" about something the person is looking at. Said as what
     * to do instead, because removing it is the one thing this screen can do
     * for it.
     */
    // The first Enter opens the row; the second does what its screen says.
    if (!opened) {
      if (current) open(current);
      return;
    }

    if (opened.native) {
      onstatus(
        `${opened.title} was built here rather than installed from the store, so there is nothing to fetch. Ctrl Shift X removes it.`,
      );
      return;
    }

    await decide(opened);
  }

  /** Shows one listing on its own screen and asks for its pictures. */
  function open(row: StoreRow) {
    opened = row;
    gallery = null;
    void storeGallery(row.name).then((urls) => {
      // Still the listing on screen: somebody can back out and open another
      // before the first answer is back.
      if (opened?.name === row.name) gallery = urls;
    });
  }

  /**
   * Escape.
   *
   * Answered here when there is something to back out of, so the launcher can
   * treat the store like every other view that owns its own Escape: the
   * confirmation goes back to the list, and the list goes back to the root.
   */
  export function back(): boolean {
    if (deciding) {
      cancel();
      return true;
    }
    if (opened) {
      opened = null;
      gallery = null;
      return true;
    }
    return false;
  }

  /** Fetches the catalogue again rather than reading the copy on disk. */
  export async function refresh() {
    onstatus("Fetching the extension store");
    await load(true);
    // The same sentence the pane shows, rather than the raw refusal. Two
    // surfaces reporting one failure in two vocabularies reads as two failures.
    onstatus(failed ? couldNot("reach the extension store") : `${browse?.total ?? 0} extensions`);
  }

  /** Moves through All, Installed and Updates. */
  export function cycleScope(by: number) {
    const order: Scope[] = ["all", "installed", "updates"];
    const at = order.indexOf(scope);
    scope = order[(at + by + order.length) % order.length];
    onselect(0);
  }

  /**
   * Reads the list again, without going back to the network.
   *
   * What the launcher calls after running an action on a row, because the row
   * it acted on is the one whose "Installed" badge has just stopped being
   * true. Removing used to live here and call Rust itself, which made it a
   * thing only this page could do; it is `sill.store.remove` in the registry
   * now and this is what puts the shelf back in agreement with the disk.
   */
  export async function reload() {
    await load();
    onchanged();
  }

  /** Whether the highlighted row has something to take back. */
  export function isInstalled(): boolean {
    return (opened ?? current)?.installed != null;
  }

  /** Step one. Fetches the source and reads it; installs nothing. */
  async function decide(row: StoreRow) {
    working = "Fetching";
    workingOn = row.name;
    onstatus(`Fetching ${row.title}`);

    try {
      deciding = await storePrepare(row.name);
      onstatus("");
    } catch (err) {
      onstatus(`${err}`);
    } finally {
      working = null;
      workingOn = null;
      howFar = null;
    }
  }

  /**
   * Step two.
   *
   * Recovers from a staging directory that is no longer there rather than
   * telling somebody to "fetch it again" on a screen with nothing on it to
   * fetch with. That state should not happen now that Enter is guarded, and
   * an error whose only remedy is an action the screen does not offer is a
   * dead end whether or not it is reachable.
   */
  async function accept(refetched = false) {
    const asked = deciding;
    if (!asked || working) return;

    working = "Installing";
    workingOn = asked.name;
    detail = "";

    try {
      const done = await storeInstall(asked.name);
      const refused = done.refused.length
        ? ` ${done.refused.length} command${done.refused.length === 1 ? "" : "s"} could not be installed.`
        : "";
      onstatus(
        `Installed ${done.title}. Find it by typing ${done.commands[0] ?? done.title}.${refused}`,
      );
      onchanged();
      // Clearing this is what puts the list back and reloads it, so the row
      // that was just installed says so. Reloading here as well would run the
      // same query twice for one install.
      deciding = null;
    } catch (err) {
      const said = `${err}`;

      if (!refetched && said.includes("staged")) {
        working = null;
        workingOn = null;
        howFar = null;
        onstatus(`Fetching ${asked.title} again`);
        // Silent, because the line below says it: a failed refetch falls
        // through to `onstatus(said)` with the real error, in the panel the
        // person is looking at. The status surface is for what has nowhere
        // else to appear.
        deciding = await storePrepare(asked.name).catch(() => null);
        if (deciding) return await accept(true);
        onstatus(said);
        return;
      }

      onstatus(said);
    } finally {
      working = null;
      workingOn = null;
      detail = "";
      howFar = null;
    }
  }

  /** Leaves the screen and throws away what was fetched for it. */
  function cancel() {
    deciding = null;
    void storeDiscard();
    onstatus("");
  }

  /** What the button on a row says, which is also what Enter will do. */
  function verb(row: StoreRow): string {
    if (row.installed?.outdated) return "Update";
    if (row.installed) return "Installed";
    return "Install";
  }

  /**
   * Whether Enter does anything to this row. An extension that is installed
   * and current has no verb, and a hint reading "Enter Installed" was the
   * hint strip saying a state as if it were a thing to do.
   */
  function acts(row: StoreRow | null): boolean {
    if (!row || row.native) return false;
    return !row.installed || Boolean(row.installed.outdated);
  }
</script>

<div class="store">
  {#if deciding}
    <!--
      What it will be able to do, before it can do any of it.

      Deliberately not styled as a permission dialog. Nothing here grants or
      withholds anything, and a screen that looks like it does would earn a
      trust that is not on offer. It is a description, and the sentence at the
      bottom says exactly that.
    -->
    <div class="decide sill-scrolls">
      <header class="head">
        <StoreIcon src={deciding.icon} label={deciding.title} size={40} />
        <div>
        <h2>{deciding.title}</h2>
        <p class="sub">
          {deciding.files} files, {weight(deciding.bytes)}, at
          <code>{shortRevision(deciding.revision)}</code>
        </p>
        </div>
      </header>

      <section>
        <h3>What this code appears to be able to do</h3>
        {#if deciding.capabilities.length}
          <ul class="reaches">
            {#each deciding.capabilities as reach (reach.id)}
              <li>
                <span class="what">
                  {reach.title}
                  {#if !reach.mediated}
                    <span class="unseen" use:hint={"Sill is not in the way of this one"}
                      >Sill never sees this</span
                    >
                  {/if}
                </span>
                <span class="why">{reach.detail}</span>
                <span class="where">{reach.seenIn.join(", ")}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="quiet">Nothing in its own source reaches outside itself.</p>
        {/if}
      </section>

      {#if deciding.secrets.length}
        <section>
          <h3>It will ask you for</h3>
          <p class="quiet">{deciding.secrets.join(", ")}</p>
        </section>
      {/if}

      {#if deciding.packages.length}
        <section>
          <h3>It will fetch {deciding.packages.length} packages from npm</h3>
          <p class="quiet">{deciding.packages.join(", ")}</p>
        </section>
      {/if}

      <section>
        <h3>Commands it adds</h3>
        <ul class="commands">
          {#each deciding.commands as command (command.name)}
            <li class:unrunnable={!command.runnable}>
              <span>{command.title}</span>
              {#if !command.runnable}
                <span class="tag">{command.mode}, which Sill cannot run</span>
              {/if}
            </li>
          {/each}
        </ul>
        <!--
          Said before the install rather than found afterwards. An extension
          whose menu bar command is dropped installs three of its four, and
          without this the missing one reads as the install half working.
        -->
        {#if deciding.refused.length}
          <p class="quiet">
            {deciding.refused.length}
            {deciding.refused.length === 1 ? "command is" : "commands are"} not installed at all.
          </p>
        {/if}
      </section>

      {#if deciding.apiWarning}
        <p class="warning">{deciding.apiWarning}</p>
      {/if}

      <p class="warning">{deciding.notEnforced}</p>

      {#if deciding.sourceUrl}
        <p class="quiet">
          Source: <code>{deciding.sourceUrl}</code>
        </p>
      {/if}

      <div class="decide-actions">
        <button class="primary" onclick={() => accept()} disabled={working !== null}>
          {working ?? "Install"}
        </button>
        <button onclick={cancel} disabled={working !== null}>Cancel</button>
      </div>

    </div>

    <!--
      Pinned, because this panel scrolls and its buttons are below the fold on
      an ordinary extension. An install that reports into the scrolling part
      reports somewhere nobody is looking: the first attempt at this drew the
      line under the Install button and it was off the bottom of the window
      for the whole minute the install took.
    -->
    {#if working}
      <p class="progress pinned">
        <span class="busy">{detail || `${working} ${deciding.title}…`}</span>
        {#if howFar !== null}
          <span
            class="bar wide"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(howFar * 100)}
            aria-label="{working} {deciding.title}"
          >
            <span class="far" style="width: {Math.round(howFar * 100)}%"></span>
          </span>
        {/if}
      </p>
    {/if}
  {:else if opened}
    <!--
      One extension, opened.

      The store's own page for it, near enough: the pictures Raycast shows,
      then what it does and what it is. This is the only place the pictures
      are fetched, one extension at a time, when somebody asks to see it.
    -->
    <div class="opened sill-scrolls">
      <div class="head">
        <StoreIcon src={opened.icon} label={opened.title} size={40} />
        <div class="named">
          <h2>{opened.title}</h2>
          <p class="sub">by {opened.author}</p>
        </div>
        {#if opened.installed?.outdated}
          <span class="state update">Update</span>
        {:else if opened.installed}
          <span class="state have">
            <svg viewBox="0 0 12 12" aria-hidden="true">
              <path d="M2.5 6.4 5 8.8l4.6-5.4" />
            </svg>Installed</span>
        {/if}
      </div>
      <p class="desc">{opened.description}</p>

      {#if gallery?.length}
        <div class="gallery sill-scrolls" aria-label="Screenshots">
          {#each gallery as shot, at (shot)}
            <img src={shot} alt="Screenshot {at + 1} of {opened.title}" loading="lazy" />
          {/each}
        </div>
      {/if}

      <h3>Commands</h3>
      <ul class="commands">
        {#each opened.commands as command (command.name)}
          <li class:unrunnable={!command.runnable}>
            <span>{command.title}</span>
            {#if !command.runnable}
              <span class="tag">{command.mode}</span>
            {/if}
          </li>
        {/each}
      </ul>

      {#if opened.categories.length}
        <h3>Categories</h3>
        <p class="quiet">{opened.categories.join(", ")}</p>
      {/if}

      <!-- A folder install has no published revision to be behind. -->
      {#if opened.revision}
        <h3>Version</h3>
        <p class="quiet">
          <code>{shortRevision(opened.revision)}</code>
          {#if opened.installed}
            {opened.installed.outdated
              ? ` published, you have ${shortRevision(opened.installed.revision) || "an unrecorded version"}`
              : " published, which is what you have"}
          {/if}
        </p>
      {/if}

      {#if opened.native}
        <p class="cta">
          Built here rather than installed from the store, so there is nothing to fetch.
          <Chord chord="Ctrl+Shift+X" /> removes it.
        </p>
      {:else if acts(opened)}
        <p class="cta act"><kbd class="sill-key">↵</kbd> {verb(opened)}</p>
      {:else}
        <p class="cta"><Chord chord="Ctrl+Shift+X" /> removes it.</p>
      {/if}
    </div>

    <div class="foot">
      <span class="keys">
        {#if working}
          <span class="busy">{detail || `${working} ${opened.title}…`}</span>
          {#if howFar !== null}
            <span
              class="bar wide"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(howFar * 100)}
              aria-label="{working} {opened.title}"
            >
              <span class="far" style="width: {Math.round(howFar * 100)}%"></span>
            </span>
          {/if}
        {:else}
          {#if acts(opened)}
            <span class="key"><Chord chord="Enter" /> {verb(opened)}</span>
          {/if}
          {#if opened.installed}
            <span class="key"><Chord chord="Ctrl+Shift+X" /> Remove</span>
          {/if}
          <span class="key"><Chord chord="Escape" /> Back</span>
        {/if}
      </span>
      <span class="counts">
        {#if opened.native}Built here{:else}{installs(opened.downloads)} installs{/if}
      </span>
    </div>
  {:else}
    {#if !ready}
      <!-- Said here rather than at the moment an install fails. Browsing works
           without Node and installing does not, and finding that out after
           choosing something is finding it out too late. -->
      <p class="missing">
        Extensions need Node.js, which is not installed, so nothing here can be
        installed yet. Get it from nodejs.org, or run: winget install OpenJS.NodeJS.LTS
      </p>
    {/if}

    <div class="pane">
      <!--
        A list of extensions is a list, and it was not one.

        This was `role="presentation"` over rows with no role, no id and no key
        handler, with two a11y warnings silenced to keep it that way. So the
        launcher's field announced itself and then said nothing while somebody
        arrowed down the store, and Enter worked only because the window was
        catching the key on its way past.
      -->
      <div
        id={LISTBOX}
        class="list sill-scrolls"
        role="listbox"
        tabindex="-1"
        aria-label="Extensions"
      >
        <Instead tone={showing} inline headline={saying} hint={alsoSaying} />

        {#each rows as row, index (row.name)}
          <div
            id={optionId(index)}
            class="row"
            class:selected={index === selected}
            role="option"
            aria-selected={index === selected}
            tabindex="-1"
            onmouseenter={() => onselect(index)}
            onclick={() => {
              onselect(index);
              void activate();
            }}
            onkeydown={(e) => {
              if (e.key !== "Enter") return;
              onselect(index);
              void activate();
            }}
          >
            <StoreIcon src={row.icon} label={row.title} size={32} />

            <div class="body">
              <div class="line">
                <span class="title">{row.title}</span>
              </div>
              <div class="line">
                <span class="desc">{row.description}</span>
              </div>
              <div class="line meta">
                {#if workingOn === row.name}
                  <!-- The row says what is happening to it. Anything slower
                       than a keystroke has to be visible where the eye
                       already is, not only in the status bar. -->
                  <span class="busy">{detail || `${working}…`}</span>
                  <!--
                    A position, where there is one to give. Fetching counts
                    files and building counts commands; npm reports lines and
                    no position at all, so the bar holds rather than guessing.
                  -->
                  {#if howFar !== null}
                    <span
                      class="bar"
                      role="progressbar"
                      aria-valuemin={0}
                      aria-valuemax={100}
                      aria-valuenow={Math.round(howFar * 100)}
                      aria-label="{working} {row.title}"
                    >
                      <span class="far" style="width: {Math.round(howFar * 100)}%"></span>
                    </span>
                  {/if}
                {:else}
                  {#if row.native}
                    <!-- A row the store does not carry has no count and no
                         author; it says where it came from instead. -->
                    <span>Built here</span>
                  {/if}
                  {#if row.blocked}
                    <span class="blocked">{row.blocked}</span>
                  {/if}
                {/if}
              </div>
            </div>

            <!--
              What the store knows about the listing that is not the listing,
              on the right where the eye goes after the title, as marks rather
              than sentences: its category, how many have it, whether it is
              here, and who made it as their picture with their name on hover.
            -->
            <div class="aside">
              {#if row.mark && row.categories[0]}
                <span class="category" use:hint={row.categories[0]}>
                  <ExtIcon icon={{ kind: "mark", name: row.mark }} small />
                </span>
              {/if}
              {#if !row.native}
                <span class="installs" use:hint={`${installs(row.downloads)} installs`}>
                  <svg viewBox="0 0 256 256" fill="currentColor" aria-hidden="true">
                    <path d={GLYPHS["arrow-circle-down"]} />
                  </svg>
                  {installs(row.downloads)}
                </span>
              {/if}
              {#if row.installed?.outdated}
                <span class="state update">Update</span>
              {:else if row.installed}
                <span class="state have">
                  <svg viewBox="0 0 12 12" aria-hidden="true">
                    <path d="M2.5 6.4 5 8.8l4.6-5.4" />
                  </svg>Installed</span>
              {/if}
              {#if row.author}
                <span class="who" use:hint={row.authorName || row.author}>
                  {#if row.authorAvatar}
                    <img src={row.authorAvatar} alt={row.authorName || row.author} loading="lazy" />
                  {:else}
                    <span class="initial" aria-label={row.authorName || row.author}>
                      {(row.authorName || row.author).trim()[0]?.toUpperCase() ?? "?"}
                    </span>
                  {/if}
                </span>
              {/if}
            </div>
          </div>
        {/each}
      </div>

    </div>

    <!--
      What you can press.

      Written out rather than left to be discovered, because none of it is:
      the scopes are chips you can see, and removing an extension, refreshing
      the catalogue and cycling the scope were three chords with nothing on
      screen naming them. A capability nobody can find is a capability nobody
      has, which is the same rule the launcher's own rows are held to.
    -->
    <div class="foot">
      <span class="keys">
        {#if working}
          <span class="busy">{detail || `${working} ${current?.title ?? ""}…`}</span>
          {#if howFar !== null}
            <span
              class="bar wide"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(howFar * 100)}
              aria-label="{working} {current?.title ?? ""}"
            >
              <span class="far" style="width: {Math.round(howFar * 100)}%"></span>
            </span>
          {/if}
        {:else}
          <span class="key"><Chord chord="Enter" /> Open</span>
          {#if current?.installed}
            <span class="key"><Chord chord="Ctrl+Shift+X" /> Remove</span>
          {/if}
          <span class="key">
            <Chord chord="Ctrl+T" />
            {scope === "all" ? "Installed" : scope === "installed" ? "Updates" : "All"}
          </span>
          <span class="key"><Chord chord="Ctrl+R" /> Refresh</span>
        {/if}
      </span>

      <span class="counts">
        {browse ? `${browse.matched} of ${browse.total}` : ""}
        {#if browse?.hidden && windowsOnly}
          · {browse.hidden} hidden
        {/if}
        {#if browse}
          · fetched {ago(browse.fetchedAt)}
        {/if}
      </span>
    </div>
  {/if}
</div>

<style>
  .store {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }

  /* Scopes and categories on one scrolling strip. There are sixteen
     categories and the window is 750px, so they scroll sideways rather than
     wrapping into a block that eats the list. */
  .missing {
    flex: none;
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--hairline);
    color: var(--text-2);
    font-size: var(--text-meta);
    line-height: var(--line-meta);
  }

  .pane {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .list {
    flex: 1;
    min-width: 0;
    padding: var(--space-1);
    overflow-y: auto;
  }

  /* One extension, opened: the store's page for it, at reading width. */
  .opened {
    flex: 1;
    min-height: 0;
    padding: var(--space-4) var(--space-5) var(--space-5);
    overflow-y: auto;
  }

  .opened .head {
    gap: var(--space-3);
    margin-bottom: var(--space-3);
  }

  .named {
    flex: 1;
    min-width: 0;
  }

  /*
   * The screenshots, as the store shows them: one strip, scrolled sideways,
   * each at a fixed height so a portrait and a landscape shot sit on one
   * line. Loaded lazily, so an extension with eight pictures fetches the
   * two on screen and the rest as they are scrolled to.
   */
  .gallery {
    display: flex;
    gap: var(--space-2);
    margin: var(--space-3) 0 var(--space-1);
    padding-bottom: var(--space-1);
    overflow-x: auto;
  }

  .gallery img {
    flex: none;
    height: 190px;
    border-radius: var(--radius-md);
    box-shadow: var(--ring-outside);
    background: var(--fill-1);
  }

  /* Three lines rather than the launcher's one, so this cannot use
     `--row-height`: a store row carries a title, a description and its
     numbers, and squeezing that into 40px is what makes a store unreadable. */
  .row {
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    cursor: default;
    transition: background-color var(--motion-travel) var(--ease);
  }

  .row:hover:not(.selected) {
    background-color: var(--fill-1);
  }

  .row.selected {
    background-color: var(--accent-fill);
    box-shadow: var(--catch);
  }

  /* The icon keeps its width; everything else shares what is left, and
     `min-width: 0` is what lets the title and description truncate rather
     than pushing the row wider than the column. */
  .body {
    flex: 1;
    min-width: 0;
  }

  .line {
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
    min-width: 0;
  }

  .title {
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
  }

  .desc,
  .meta {
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .desc,
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The right-hand side of a row: category, count, pill and face, centred
     on the row rather than tied to its first line, so they sit level with
     the middle of the icon whether the row is two lines or three. */
  .aside {
    display: flex;
    flex: none;
    gap: var(--space-3);
    align-items: center;
    align-self: center;
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .installs {
    display: inline-flex;
    gap: var(--space-1);
    align-items: center;
    white-space: nowrap;
  }

  /* Drawn at the small icon size rather than at the text's: a 256-unit
     outline at fourteen pixels aliased into a smudge. */
  .installs svg {
    width: var(--icon-tile-sm);
    height: var(--icon-tile-sm);
  }

  /* Closer to the count than the rest of the row is to each other: the
     category and the count are both facts about the listing, and read as a
     pair. */
  .category {
    display: grid;
    flex: none;
    place-items: center;
    margin-right: calc(var(--space-1) - var(--space-3));
    color: var(--text-3);
  }

  .who {
    display: grid;
    flex: none;
    place-items: center;
    width: var(--icon-tile-sm);
    height: var(--icon-tile-sm);
    border-radius: var(--radius-pill);
    overflow: hidden;
    background: var(--fill-2);
    box-shadow: var(--ring);
  }

  .who img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .initial {
    color: var(--text-2);
    font-size: var(--text-label);
    font-weight: var(--weight-medium);
    line-height: 1;
  }

  .meta {
    gap: var(--space-3);
    margin-top: var(--space-half);
  }

  /*
   * The row's mark: a pill, not a word.
   *
   * "Installed" in the meta colour was the one fact on the row somebody most
   * wants at a glance, drawn at the size of the download count. The accent is
   * for affirmative state, and an extension that is on this machine is
   * exactly that; "Update" wears the accent as a line rather than a fill,
   * because it is a thing to do rather than a thing that is done.
   */
  .state {
    display: inline-flex;
    flex: none;
    gap: var(--space-1);
    align-items: center;
    height: 18px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    font-size: var(--text-label);
    font-weight: var(--weight-medium);
    line-height: 1;
  }

  .state svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .have {
    background: var(--accent-fill-strong);
    box-shadow: var(--ring-accent-faint);
    color: var(--accent-bright);
  }

  .update {
    box-shadow: var(--ring-accent-faint);
    color: var(--accent);
  }

  /* Says why a row cannot be installed here, which is a fact somebody has to
     read before they stop trying. --text-4 is declared decorative-only and
     this is not decoration. */
  .blocked {
    color: var(--text-3);
  }

  .head {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    margin-bottom: var(--space-3);
  }

  h2 {
    margin: 0;
    color: var(--text-1);
    font-size: var(--text-heading);
    font-weight: var(--weight-medium);
  }

  h3 {
    margin: var(--space-4) 0 var(--space-1);
    color: var(--text-3);
    font-size: var(--text-label);
    font-weight: var(--weight-medium);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .sub {
    margin: var(--space-half) 0 0;
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .opened .desc {
    display: block;
    white-space: normal;
    line-height: var(--line-body);
  }

  .quiet {
    margin: 0;
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: var(--line-meta);
  }

  .cta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-2);
    align-items: center;
    margin: var(--space-4) 0 0;
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  /* The one thing Enter does here, drawn as the key and the verb. */
  .cta.act {
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .commands li {
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
    padding: var(--space-half) 0;
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .commands li.unrunnable {
    color: var(--text-3);
  }

  .tag {
    color: var(--text-3);
    font-size: var(--text-micro);
  }

  code {
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: var(--text-micro);
  }

  .foot {
    display: flex;
    flex: none;
    gap: var(--space-3);
    justify-content: space-between;
    align-items: center;
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--hairline);
    /* The keys this screen answers to. They were the quietest text in the
       window, one size below the description of a row, and a capability
       nobody notices is a capability nobody has. Keycaps and the meta size,
       the same as the launcher's own chin. */
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .keys {
    display: flex;
    gap: var(--space-4);
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }

  .key {
    display: inline-flex;
    gap: var(--space-1);
    align-items: center;
  }

  .counts {
    flex: none;
    color: var(--text-3);
    font-size: var(--text-micro);
  }

  /* The accent means "this one is chosen" everywhere else in Sill; here it is
     the one thing on screen that is actually happening. */
  .busy {
    color: var(--accent);
  }

  /*
   * How far along, beside the words saying what is happening.
   *
   * A hairline rather than a block: this sits inside a row of an ordinary list
   * and a solid bar would be the loudest thing on a screen whose subject is
   * the extension above it. The accent is allowed here for the same reason it
   * is allowed on `.busy`, which it sits next to: this is the one thing on
   * screen that is actually happening.
   */
  /*
   * What the install is doing, on the screen the install runs on.
   *
   * The only thing that said so was the word on a disabled button, and that
   * button is below the fold on any extension with more than a few
   * capabilities. npm and esbuild take most of a minute between them, and a
   * screen that says nothing for that long is one that has stopped.
   */
  .progress {
    display: flex;
    align-items: center;
    margin: var(--space-2) 0 0;
    font-size: var(--text-meta);
  }

  /* Above the chin, out of the part that scrolls. */
  .pinned {
    flex: none;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--hairline);
  }

  .bar {
    display: inline-block;
    width: 5rem;
    height: 2px;
    margin-left: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    vertical-align: middle;
    overflow: hidden;
  }

  .bar.wide {
    width: 8rem;
  }

  .far {
    display: block;
    height: 100%;
    background: var(--accent);
    /* Files land several at a time, so the position arrives in jumps. */
    transition: width var(--motion-state) linear;
  }

  /* ------------------------------------------------------------- deciding */

  .decide {
    flex: 1;
    min-height: 0;
    padding: var(--space-4);
    overflow-y: auto;
  }

  .decide header {
    margin-bottom: var(--space-2);
  }

  .decide section {
    margin-bottom: var(--space-2);
  }

  .reaches li {
    display: grid;
    gap: var(--space-hair);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--hairline);
  }

  .reaches li:last-child {
    border-bottom: 0;
  }

  .what {
    color: var(--text-1);
    font-size: var(--text-body);
  }

  .why {
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .where {
    color: var(--text-3);
    font-size: var(--text-micro);
  }

  /* The one thing on this screen that is a fact about Sill rather than about
     the extension: whether Sill is in the way of it at all. */
  .unseen {
    margin-left: var(--space-2);
    color: var(--text-3);
    font-size: var(--text-micro);
  }

  .warning {
    margin: var(--space-3) 0;
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--fill-1);
    color: var(--text-2);
    font-size: var(--text-meta);
    line-height: var(--line-meta);
  }

  .decide-actions {
    display: flex;
    gap: var(--space-2);
    margin-top: var(--space-4);
  }

  .decide-actions button {
    padding: var(--space-2) var(--space-4);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--fill-2);
    color: var(--text-1);
    font-size: var(--text-meta);
    cursor: pointer;
  }

  .decide-actions button:hover:not(:disabled) {
    background: var(--fill-3);
  }

  .decide-actions button.primary {
    background: var(--accent-fill-strong);
  }

  /* Disabled is a state with a token of its own. Fading the label to the
     decorative step said the same thing by accident and said it differently
     from every other disabled control in the application. */
  .decide-actions button:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }
</style>
