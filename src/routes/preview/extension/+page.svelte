<script lang="ts">
  /**
   * A development route. Reachable with `npm run dev`, never linked from the
   * app, and of no use to anyone running Sill rather than working on it.
   *
   * What an extension's list, grid and detail page look like, held still.
   *
   * The view gate proves that the right things are read out of the tree; it
   * cannot see whether an accessory sits on the baseline or whether a detail
   * pane beside a list leaves the rows enough room. Those are the questions
   * this answers, and answering them otherwise means installing a store
   * extension and finding a row that happens to use the part being judged.
   *
   * The trees below are built from ops rather than from markup, so what is on
   * screen went through the same `ViewTree` the window feeds from. A fixture
   * assembled any other way would be a picture of a component rather than of
   * the thing the launcher draws.
   */
  import "$lib/theme/theme.css";
  import DetailPane from "$lib/components/DetailPane.svelte";
  import Footer from "$lib/components/Footer.svelte";
  import FormView from "$lib/components/FormView.svelte";
  import GridView from "$lib/components/GridView.svelte";
  import ListView from "$lib/components/ListView.svelte";
  import { toastActions } from "$lib/exthost/actions";
  import { rowsOf } from "$lib/exthost/search";
  import { ROOT_ID, ViewTree, type Op } from "$lib/exthost/tree";

  interface Spec {
    tag: string;
    props?: Record<string, unknown>;
    children?: Spec[];
  }

  /** Grows a tree the way a commit would, so the ops are the real ops. */
  function grow(spec: Spec): ViewTree {
    const tree = new ViewTree();
    const ops: Op[] = [];
    let next = 1;

    const add = (node: Spec, parent: number) => {
      const id = next++;
      ops.push({ op: "create", id, $t: node.tag, props: node.props ?? {} });
      ops.push({ op: "append", parent, child: id });
      for (const child of node.children ?? []) add(child, id);
      return id;
    };

    add(spec, ROOT_ID);
    tree.apply(ops);
    return tree;
  }

  const metadata = (prefix: string): Spec => ({
    tag: "$slot",
    props: { name: "metadata" },
    children: [
      {
        tag: `${prefix}.Metadata`,
        children: [
          { tag: `${prefix}.Metadata.Label`, props: { title: "Kind", text: "Repository" } },
          {
            tag: `${prefix}.Metadata.Label`,
            props: { title: "Owner", text: "winters27", icon: "Person" },
          },
          { tag: `${prefix}.Metadata.Separator` },
          {
            tag: `${prefix}.Metadata.Link`,
            props: {
              title: "Home",
              text: "github.com/winters27/sill",
              target: "https://github.com/winters27/sill",
            },
          },
          {
            tag: `${prefix}.Metadata.TagList`,
            props: { title: "Topics" },
            children: [
              {
                tag: `${prefix}.Metadata.TagList.Item`,
                props: { text: "rust", color: "raycast-green" },
              },
              {
                tag: `${prefix}.Metadata.TagList.Item`,
                props: { text: "tauri", color: "raycast-blue" },
              },
              { tag: `${prefix}.Metadata.TagList.Item`, props: { text: "windows" } },
            ],
          },
        ],
      },
    ],
  });

  const MARKDOWN = `# Sill

A **launcher** for Windows, with a [site](https://example.invalid).

- Rust does the thinking
- The window presents it

\`\`\`sh
npm run dev
\`\`\``;

  const item = (
    title: string,
    icon: unknown,
    accessories: unknown[],
    subtitle?: string,
  ): Spec => ({
    tag: "List.Item",
    props: { title, icon, accessories, subtitle },
    children: [
      {
        tag: "$slot",
        props: { name: "detail" },
        children: [
          {
            tag: "List.Item.Detail",
            props: { markdown: MARKDOWN },
            children: [metadata("List.Item.Detail")],
          },
        ],
      },
    ],
  });

  const listTree = grow({
    tag: "List",
    props: { navigationTitle: "Repositories" },
    children: [
      {
        tag: "List.Section",
        props: { title: "Pinned", subtitle: "3" },
        children: [
          item("A named icon", "Star", [{ text: "12 items" }, { tag: "ready" }], "Icon.Star"),
          item("A tinted icon", { source: "CheckCircle", tintColor: "raycast-green" }, [
            { tag: { value: "passing", color: "raycast-green" } },
          ]),
        ],
      },
      item("An emoji icon", "🎉", [
        { text: "2 days ago" },
        { tag: { value: "beta", color: "raycast-yellow" } },
      ]),
      item("A name with no mark drawn for it", "Fingerprint", [
        { text: "falls back to a letter" },
      ]),
      item("A picture from the network", { source: "https://example.invalid/a.png" }, []),
    ],
  });

  const detailTree = grow({
    tag: "Detail",
    props: { markdown: MARKDOWN },
    children: [metadata("Detail")],
  });

  const gridTree = grow({
    tag: "Grid",
    props: { columns: 5 },
    children: [
      { tag: "Grid.Section", props: { title: "Round" } },
      { tag: "Grid.Item", props: { title: "Circle", subtitle: "round", content: "●" } },
      { tag: "Grid.Item", props: { title: "Ring", content: "○" } },
      { tag: "Grid.Item", props: { title: "Square", content: "■" } },
    ],
  });

  /** A list with nothing in it but its own words for being empty. */
  const emptyTree = grow({
    tag: "List",
    children: [
      {
        tag: "List.EmptyView",
        props: {
          title: "No repositories",
          description: "Sign in to see the ones you can reach.",
        },
      },
    ],
  });

  /** Every form control, including the three that were drawn as nothing. */
  const formTree = grow({
    tag: "Form",
    children: [
      { tag: "Form.TextField", props: { id: "name", title: "Name", placeholder: "A short name" } },
      { tag: "Form.TextArea", props: { id: "notes", title: "Notes" } },
      {
        tag: "Form.Dropdown",
        props: { id: "kind", title: "Kind" },
        children: [
          { tag: "Form.Dropdown.Item", props: { value: "a", title: "Repository" } },
          { tag: "Form.Dropdown.Item", props: { value: "b", title: "Gist" } },
        ],
      },
      {
        tag: "Form.TagPicker",
        props: { id: "topics", title: "Topics", defaultValue: ["rust"] },
        children: [
          { tag: "Form.TagPicker.Item", props: { value: "rust", title: "rust" } },
          { tag: "Form.TagPicker.Item", props: { value: "tauri", title: "tauri" } },
          { tag: "Form.TagPicker.Item", props: { value: "svelte", title: "svelte" } },
        ],
      },
      { tag: "Form.DatePicker", props: { id: "due", title: "Due", type: "date" } },
      { tag: "Form.DatePicker", props: { id: "at", title: "At" } },
      // Both states of a picker, because they are the two different rows: one
      // offering to choose and one holding what was chosen. No session is
      // passed to the form below, so the buttons open nothing here; what this
      // is for is whether the names sit on the baseline beside them.
      { tag: "Form.FilePicker", props: { id: "empty", title: "Attachment" } },
      {
        tag: "Form.FilePicker",
        props: {
          id: "sources",
          title: "Sources",
          defaultValue: ["C:\\Users\\someone\\Documents\\notes.md", "C:\\Windows\\System32"],
        },
      },
      { tag: "Form.Separator" },
      { tag: "Form.Checkbox", props: { id: "pin", title: "Pin", label: "Keep at the top" } },
    ],
  });

  /** A list showing its detail pane, and the same list without one. */
  let showing = $state(true);
  let selected = $state(0);

  const listNode = $derived.by(() => {
    const top = listTree.top();
    if (top) top.props.isShowingDetail = showing;
    return top;
  });

  const rows = $derived(listNode ? rowsOf(listTree, listNode, "") : []);
  const emptyNode = emptyTree.top();
  const formNode = formTree.top();
  const gridNode = gridTree.top();
  const detailNode = detailTree.top();
</script>

<main class="sill-scrolls">
  <h1>Extension views</h1>

  <label class="toggle">
    <input type="checkbox" bind:checked={showing} />
    isShowingDetail
  </label>

  <section class="window">
    {#if listNode}
      <ListView
        tree={listTree}
        node={listNode}
        version={showing ? 1 : 0}
        {rows}
        query=""
        loading={false}
        {selected}
        onselect={(i) => (selected = i)}
        onrun={() => {}}
      />
    {/if}
  </section>

  <h2>An empty list, in the extension's own words</h2>
  <section class="window short">
    {#if emptyNode}
      <ListView
        tree={emptyTree}
        node={emptyNode}
        version={0}
        rows={[]}
        query=""
        loading={false}
        selected={0}
        onselect={() => {}}
        onrun={() => {}}
      />
    {/if}
  </section>

  <h2>Detail, as a whole view</h2>
  <section class="window">
    {#if detailNode}
      <DetailPane tree={detailTree} node={detailNode} version={0} />
    {/if}
  </section>

  <h2>Form</h2>
  <section class="window">
    {#if formNode}
      <FormView tree={formTree} node={formNode} version={0} onsubmit={() => {}} />
    {/if}
  </section>

  <!--
    A toast is the one thing a command can put in front of somebody that leaves
    no mark on the tree, so it is drawn here in the chin it actually lives in.
    Both shapes: a message on its own, and a failure with the two buttons an
    extension is allowed to offer.
  -->
  <h2>A toast, and a toast with the extension's own buttons</h2>
  <section class="chin">
    <Footer
      mode="root"
      toast={{ title: "Copied to clipboard", style: "success", actions: [] }}
      status=""
      prefs={null}
      viewTag="List"
      hasActions={true}
      onbuiltin={() => {}}
      onrun={() => {}}
      onactions={() => {}}
      ontoastaction={() => {}}
    />
  </section>
  <section class="chin">
    <Footer
      mode="root"
      toast={{
        title: "Could not reach the server",
        style: "failure",
        actions: toastActions([
          { title: "Try Again", handler: "h1", shortcut: { modifiers: ["cmd"], key: "r" } },
          { title: "Give Up", handler: "h2" },
        ]),
      }}
      status=""
      prefs={null}
      viewTag="List"
      hasActions={true}
      onbuiltin={() => {}}
      onrun={() => {}}
      onactions={() => {}}
      ontoastaction={() => {}}
    />
  </section>

  <h2>Grid</h2>
  <section class="window short">
    {#if gridNode}
      <GridView
        tree={gridTree}
        node={gridNode}
        cells={rowsOf(gridTree, gridNode, "")}
        version={0}
        query=""
        loading={false}
        selected={0}
        onselect={() => {}}
        onrun={() => {}}
      />
    {/if}
  </section>
</main>

<style>
  /*
   * The theme hides the document's overflow, because the launcher is a window
   * that never scrolls. A harness that is a page of examples has to, so this
   * scrolls itself rather than asking the theme to change for it.
   */
  main {
    height: 100vh;
    overflow-y: auto;
    padding: var(--space-6);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    background-color: var(--core-background);
    min-height: 100vh;
  }

  h1,
  h2 {
    margin: 0;
    color: var(--text-1);
    font-size: var(--text-heading);
    font-weight: var(--weight-medium);
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  /* A stand-in for the launcher window, so widths and hairlines are judged at
     the size they are actually drawn at. */
  /* `flex: none` because these are stand-ins for a window and a window has a
     size; as ordinary flex children they shrank to share the page. */
  .window {
    display: flex;
    flex: none;
    width: 750px;
    height: 420px;
    border: 1px solid var(--hairline);
    border-radius: var(--radius-lg);
    background-color: var(--surface-base);
    overflow: hidden;
  }

  /* The chin alone, at the width it has in the launcher, so the buttons are
     judged against the space they actually get. */
  .chin {
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    flex: none;
    width: 750px;
    border: 1px solid var(--hairline);
    border-radius: var(--radius-lg);
    background-color: var(--surface-base);
    overflow: hidden;
  }

  .window.short {
    height: 240px;
  }
</style>
