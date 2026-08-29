/**
 * The view tree and the mutation ops that describe changes to it.
 *
 * The host does not ship whole trees on every render. React's reconciler
 * already tells us exactly what changed, so those mutations are forwarded as
 * an op stream and the UI applies them to its own copy of the tree.
 *
 * This is the finer of the two approaches in this space: Vicinae re-serialises
 * an entire view whenever anything inside it changes, while an op stream costs
 * only what actually moved.
 */

/** Every instance gets a stable id so ops can address it without paths. */
export type NodeId = number;

export const CONTAINER_ID: NodeId = 0;

/** A serialisable prop value. Functions are replaced by handler ids first. */
export type PropValue = unknown;

export interface SerializedProps {
  [key: string]: PropValue;
}

/**
 * A host instance. `$t` is the component tag, e.g. "List" or "Action".
 * `children` is kept so a full snapshot can be produced on demand, which the
 * UI needs when it attaches to an already-running command.
 */
export interface Instance {
  id: NodeId;
  $t: string;
  props: SerializedProps;
  children: Array<Instance | TextInstance>;
}

export interface TextInstance {
  id: NodeId;
  $t: "#text";
  text: string;
  children: [];
}

export type AnyInstance = Instance | TextInstance;

export function isTextInstance(node: AnyInstance): node is TextInstance {
  return node.$t === "#text";
}

export type Op =
  | { op: "create"; id: NodeId; $t: string; props: SerializedProps }
  | { op: "createText"; id: NodeId; text: string }
  | { op: "updateProps"; id: NodeId; props: SerializedProps }
  | { op: "updateText"; id: NodeId; text: string }
  | { op: "append"; parent: NodeId; child: NodeId }
  | { op: "insertBefore"; parent: NodeId; child: NodeId; before: NodeId }
  | { op: "remove"; parent: NodeId; child: NodeId }
  | { op: "clear"; id: NodeId };

/** One commit's worth of ops, in the order the reconciler produced them. */
export interface Frame {
  ops: Op[];
}

/**
 * Structural equality over serialised prop bags.
 *
 * Extensions overwhelmingly pass inline arrow functions, so React sees new
 * props on every render even when nothing meaningful changed. Once functions
 * have been normalised to stable handler ids, that noise becomes detectable:
 * the two bags compare equal and the update op can be dropped entirely.
 */
export function propsEqual(a: SerializedProps, b: SerializedProps): boolean {
  const aKeys = Object.keys(a);
  if (aKeys.length !== Object.keys(b).length) return false;
  for (const key of aKeys) {
    if (!Object.prototype.hasOwnProperty.call(b, key)) return false;
    if (!valueEqual(a[key], b[key])) return false;
  }
  return true;
}

function valueEqual(a: PropValue, b: PropValue): boolean {
  if (a === b) return true;
  if (a === null || b === null) return false;
  if (typeof a !== "object" || typeof b !== "object") return false;

  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((item, i) => valueEqual(item, b[i]));
  }

  return propsEqual(a as SerializedProps, b as SerializedProps);
}

/** Rebuilds a plain nested tree, for a snapshot rather than a patch. */
export function snapshot(node: AnyInstance): unknown {
  if (isTextInstance(node)) {
    return { $t: "#text", id: node.id, text: node.text };
  }
  return {
    $t: node.$t,
    id: node.id,
    ...node.props,
    children: node.children.map(snapshot),
  };
}
