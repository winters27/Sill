/**
 * Applies the extension host's op stream to a local view tree.
 *
 * The host sends patches, not snapshots, so this is the only place that knows
 * how to rebuild the tree. Keeping it a plain function over a plain store
 * makes it testable without mounting anything.
 */

export type NodeId = number;

/** Root is a fixed id so the first append has somewhere to land. */
export const ROOT_ID: NodeId = 0;

export interface ElementNode {
  kind: "element";
  id: NodeId;
  tag: string;
  props: Record<string, unknown>;
  children: NodeId[];
}

export interface TextNode {
  kind: "text";
  id: NodeId;
  text: string;
}

export type TreeNode = ElementNode | TextNode;

export type Op =
  | { op: "create"; id: NodeId; $t: string; props: Record<string, unknown> }
  | { op: "createText"; id: NodeId; text: string }
  | { op: "updateProps"; id: NodeId; props: Record<string, unknown> }
  | { op: "updateText"; id: NodeId; text: string }
  | { op: "append"; parent: NodeId; child: NodeId }
  | { op: "insertBefore"; parent: NodeId; child: NodeId; before: NodeId }
  | { op: "remove"; parent: NodeId; child: NodeId }
  | { op: "clear"; id: NodeId };

/** A handler reference as it arrives in props. */
export interface HandlerRef {
  $handler: string;
}

export function isHandlerRef(value: unknown): value is HandlerRef {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as HandlerRef).$handler === "string"
  );
}

export class ViewTree {
  private nodes = new Map<NodeId, TreeNode>();

  /**
   * Who each node currently hangs off, which is how a dropped one is spotted.
   *
   * The map of nodes is keyed by an id that only ever goes up, so anything
   * left in it that nothing points at is memory nobody can reach. Without a
   * parent to ask about, working that out means walking the whole tree on
   * every commit; with one it costs a lookup per structural op, which is a
   * cost the op was already paying.
   */
  private parents = new Map<NodeId, NodeId>();

  /** Nodes this batch took off the tree, judged once the batch has landed. */
  private detached = new Set<NodeId>();

  constructor() {
    this.reset();
  }

  reset(): void {
    this.nodes.clear();
    this.parents.clear();
    this.detached.clear();
    this.nodes.set(ROOT_ID, {
      kind: "element",
      id: ROOT_ID,
      tag: "$root",
      props: {},
      children: [],
    });
  }

  /** How many nodes are held, so a test can watch it come back down. */
  get size(): number {
    return this.nodes.size;
  }

  get(id: NodeId): TreeNode | undefined {
    return this.nodes.get(id);
  }

  root(): ElementNode {
    return this.nodes.get(ROOT_ID) as ElementNode;
  }

  /** The single node the UI should draw, or undefined before first render. */
  top(): ElementNode | undefined {
    const child = this.root().children[0];
    if (child === undefined) return undefined;
    const node = this.nodes.get(child);
    return node?.kind === "element" ? node : undefined;
  }

  children(node: ElementNode): TreeNode[] {
    const out: TreeNode[] = [];
    for (const id of node.children) {
      const child = this.nodes.get(id);
      if (child) out.push(child);
    }
    return out;
  }

  /**
   * Returns the subtree passed as a named element prop.
   *
   * Element-valued props are lifted into `$slot` children by the API layer,
   * because React elements cannot be serialized and the reconciler never
   * descends into props. Reading one back means looking through children.
   */
  slot(node: ElementNode, name: string): ElementNode | undefined {
    for (const child of this.children(node)) {
      if (child.kind !== "element" || child.tag !== "$slot") continue;
      if (child.props.name !== name) continue;
      const inner = this.children(child).find((c) => c.kind === "element");
      return inner as ElementNode | undefined;
    }
    return undefined;
  }

  /** Element children, skipping slots and whitespace-only text. */
  elementChildren(node: ElementNode): ElementNode[] {
    return this.children(node).filter(
      (child): child is ElementNode => child.kind === "element" && child.tag !== "$slot",
    );
  }

  /** Concatenated text of a node's direct text children. */
  text(node: ElementNode): string {
    return this.children(node)
      .filter((c): c is TextNode => c.kind === "text")
      .map((c) => c.text)
      .join("");
  }

  apply(ops: Op[]): void {
    for (const op of ops) {
      switch (op.op) {
        case "create":
          this.nodes.set(op.id, {
            kind: "element",
            id: op.id,
            tag: op.$t,
            props: op.props ?? {},
            children: [],
          });
          break;

        case "createText":
          this.nodes.set(op.id, { kind: "text", id: op.id, text: op.text });
          break;

        case "updateProps": {
          const node = this.nodes.get(op.id);
          // Props are sent whole rather than as a delta, so replacing is
          // correct; merging would resurrect props the extension removed.
          if (node?.kind === "element") node.props = op.props ?? {};
          break;
        }

        case "updateText": {
          const node = this.nodes.get(op.id);
          if (node?.kind === "text") node.text = op.text;
          break;
        }

        case "append": {
          const parent = this.nodes.get(op.parent);
          if (parent?.kind !== "element") break;
          // React can move an existing child by appending it again.
          const existing = parent.children.indexOf(op.child);
          if (existing !== -1) parent.children.splice(existing, 1);
          parent.children.push(op.child);
          this.adopt(op.parent, op.child);
          break;
        }

        case "insertBefore": {
          const parent = this.nodes.get(op.parent);
          if (parent?.kind !== "element") break;
          const existing = parent.children.indexOf(op.child);
          if (existing !== -1) parent.children.splice(existing, 1);
          const at = parent.children.indexOf(op.before);
          if (at === -1) parent.children.push(op.child);
          else parent.children.splice(at, 0, op.child);
          this.adopt(op.parent, op.child);
          break;
        }

        case "remove": {
          const parent = this.nodes.get(op.parent);
          if (parent?.kind !== "element") break;
          const at = parent.children.indexOf(op.child);
          if (at !== -1) parent.children.splice(at, 1);
          // The node itself is kept until the end of the batch: React reuses
          // detached nodes when moving them, and dropping it here would lose
          // one mid-reorder.
          this.disown(op.parent, op.child);
          break;
        }

        case "clear": {
          const node = this.nodes.get(op.id);
          if (node?.kind !== "element") break;
          for (const child of node.children) this.disown(op.id, child);
          node.children = [];
          break;
        }
      }
    }

    this.sweep();
  }

  private adopt(parent: NodeId, child: NodeId): void {
    this.parents.set(child, parent);
    // Taken off one parent and put on another inside the same batch, which is
    // a move rather than a deletion.
    this.detached.delete(child);
  }

  private disown(parent: NodeId, child: NodeId): void {
    // Only if it still belongs to the parent letting go of it. A node already
    // moved elsewhere earlier in the batch keeps its new home.
    if (this.parents.get(child) === parent) this.parents.delete(child);
    this.detached.add(child);
  }

  /**
   * Forgets whatever this batch took off the tree and did not put back.
   *
   * ## Why the tree has to be able to shrink
   *
   * Ids are minted by the reconciler and never reused, so a node the UI keeps
   * after React has thrown it away is memory nothing can ever reach again. A
   * list that goes from two thousand emoji to six hundred leaves fourteen
   * hundred behind; a view pushed and popped ten times leaves ten whole
   * screens behind. Neither ever comes back, and nothing was collecting them.
   *
   * A batch is one React commit, which is the unit that matters: inside one,
   * a remove followed by an append is a reorder and the node must survive it.
   * Once the commit has landed, anything still parentless is genuinely gone.
   *
   * Nothing at all happens when a batch removed nothing, which is most of
   * them, so the common case pays for none of this.
   */
  private sweep(): void {
    if (this.detached.size === 0) return;

    for (const id of this.detached) {
      if (this.parents.has(id)) continue;
      this.drop(id);
    }

    this.detached.clear();
  }

  private drop(id: NodeId): void {
    const node = this.nodes.get(id);
    this.nodes.delete(id);
    if (node?.kind !== "element") return;

    for (const child of node.children) {
      // React sends no `remove` for a grandchild, so its parent entry still
      // names the node being dropped. One that says something else was moved
      // out and is somebody else's now.
      if (this.parents.get(child) !== id) continue;
      this.parents.delete(child);
      this.drop(child);
    }
  }
}
