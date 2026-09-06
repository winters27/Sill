import type { HostConfig } from "react-reconciler";
import { DefaultEventPriority } from "react-reconciler/constants";
import { CallbackManager, type Callback } from "./callbacks";
import {
  CONTAINER_ID,
  type AnyInstance,
  type Instance,
  type NodeId,
  propsEqual,
  type Op,
  type SerializedProps,
  type TextInstance,
} from "./nodes";

/** The container is a node like any other, so ops can address it by id. */
export interface Container {
  id: NodeId;
  $t: "#root";
  props: SerializedProps;
  children: AnyInstance[];
}

type Type = string;
type Props = Record<string, unknown>;
type HostContext = Record<string, never>;
type TimeoutHandle = ReturnType<typeof setTimeout>;

export interface RendererHooks {
  /** Receives each commit's ops. Empty commits are not reported. */
  onCommit(ops: Op[]): void;
}

export interface RendererState {
  container: Container;
  callbacks: CallbackManager;
  hostConfig: HostConfig<
    Type,
    Props,
    Container,
    Instance,
    TextInstance,
    never, // SuspenseInstance
    never, // HydratableInstance
    never, // FormInstance
    AnyInstance, // PublicInstance
    HostContext,
    never, // ChildSet, mutation mode only
    TimeoutHandle,
    number, // NoTimeout
    null // TransitionStatus
  >;
}

/** React props that never belong on the wire. */
const SKIPPED_PROPS = new Set(["children", "key", "ref"]);

export function createRendererState(hooks: RendererHooks): RendererState {
  const callbacks = new CallbackManager();

  const container: Container = {
    id: CONTAINER_ID,
    $t: "#root",
    props: {},
    children: [],
  };

  let nextId: NodeId = 1;
  let ops: Op[] = [];

  /** Maps a live instance to the handler ids currently minted from its props. */
  const handlerIds = new WeakMap<object, Map<string, string>>();

  const emit = (op: Op) => ops.push(op);

  /**
   * Replaces function props with handler ids. Ids are reused across renders for
   * the same instance and prop name, so a re-render does not invalidate a
   * handler the UI is about to activate.
   */
  const serializeProps = (target: object, props: Props): SerializedProps => {
    const out: SerializedProps = {};
    let ids = handlerIds.get(target);
    const seen = new Set<string>();

    /**
     * Whether this is an object whose insides are worth walking.
     *
     * A plain one only. A `Date` is an object and its insides are not props;
     * anything with a prototype of its own is a value the extension built
     * rather than a bag it filled in, and rebuilding it here would hand the
     * window a copy that has lost whatever made it that kind of thing.
     */
    const plain = (value: unknown): value is Record<string, unknown> => {
      if (!value || typeof value !== "object") return false;
      const proto = Object.getPrototypeOf(value);
      return proto === Object.prototype || proto === null;
    };

    /**
     * One prop, with any callback inside it replaced by an id.
     *
     * **Nesting is the whole point.** This used to look only at the top of the
     * prop bag, so `onAction` became an id and `pagination.onLoadMore` did
     * not: it fell through as a function, JSON dropped it on the way out, and
     * the window received a list that said there was more and named nobody to
     * ask for it. Nothing failed. The list simply ended, one page in, exactly
     * as a list with no more pages does.
     *
     * The path is the id's key, so `pagination.onLoadMore` keeps its own id
     * across renders for the same instance, which is what stops a re-render
     * invalidating a handler the window is about to activate.
     */
    const serialize = (path: string, value: unknown): unknown => {
      if (typeof value === "function") {
        if (!ids) {
          ids = new Map();
          handlerIds.set(target, ids);
        }
        const existing = ids.get(path);
        if (existing) {
          callbacks.rebind(existing, value as Callback);
          seen.add(existing);
          return { $handler: existing };
        }
        const id = callbacks.register(value as Callback);
        ids.set(path, id);
        seen.add(id);
        return { $handler: id };
      }

      if (Array.isArray(value)) {
        return value.map((entry, at) => serialize(`${path}.${at}`, entry));
      }

      if (plain(value)) {
        return Object.fromEntries(
          Object.entries(value).map(([key, inner]) => [key, serialize(`${path}.${key}`, inner)]),
        );
      }

      return value;
    };

    for (const [key, value] of Object.entries(props)) {
      if (SKIPPED_PROPS.has(key)) continue;
      out[key] = serialize(key, value) as SerializedProps[string];
    }

    // Handlers that disappeared from this render are released next commit.
    if (ids) {
      for (const [key, id] of ids) {
        if (!seen.has(id)) {
          callbacks.deferRemoval(id);
          ids.delete(key);
        }
      }
    }

    return out;
  };

  const detach = (node: AnyInstance) => {
    const ids = handlerIds.get(node);
    if (ids) for (const [, id] of ids) callbacks.deferRemoval(id);
    for (const child of node.children) detach(child);
  };

  const hostConfig: RendererState["hostConfig"] = {
    supportsMutation: true,
    supportsPersistence: false,
    supportsHydration: false,
    isPrimaryRenderer: true,

    getPublicInstance: (instance) => instance,
    getRootHostContext: () => ({}),
    getChildHostContext: (parentContext) => parentContext,

    prepareForCommit: () => null,

    resetAfterCommit: () => {
      if (ops.length > 0) {
        const batch = ops;
        ops = [];
        hooks.onCommit(batch);
      }
      callbacks.flushDeferredRemovals();
    },

    createInstance: (type, props) => {
      const instance: Instance = {
        id: nextId++,
        $t: type,
        props: {},
        children: [],
      };
      instance.props = serializeProps(instance, props);
      emit({ op: "create", id: instance.id, $t: type, props: instance.props });
      return instance;
    },

    createTextInstance: (text) => {
      const instance: TextInstance = {
        id: nextId++,
        $t: "#text",
        text,
        children: [],
      };
      emit({ op: "createText", id: instance.id, text });
      return instance;
    },

    appendInitialChild: (parent, child) => {
      parent.children.push(child);
      emit({ op: "append", parent: parent.id, child: child.id });
    },

    finalizeInitialChildren: () => false,

    shouldSetTextContent: () => false,

    appendChild: (parent, child) => {
      parent.children.push(child);
      emit({ op: "append", parent: parent.id, child: child.id });
    },

    appendChildToContainer: (parent, child) => {
      parent.children.push(child);
      emit({ op: "append", parent: parent.id, child: child.id });
    },

    insertBefore: (parent, child, before) => {
      const at = parent.children.indexOf(before);
      if (at >= 0) parent.children.splice(at, 0, child);
      else parent.children.push(child);
      emit({ op: "insertBefore", parent: parent.id, child: child.id, before: before.id });
    },

    insertInContainerBefore: (parent, child, before) => {
      const at = parent.children.indexOf(before);
      if (at >= 0) parent.children.splice(at, 0, child);
      else parent.children.push(child);
      emit({ op: "insertBefore", parent: parent.id, child: child.id, before: before.id });
    },

    removeChild: (parent, child) => {
      const at = parent.children.indexOf(child);
      if (at >= 0) parent.children.splice(at, 1);
      detach(child);
      emit({ op: "remove", parent: parent.id, child: child.id });
    },

    removeChildFromContainer: (parent, child) => {
      const at = parent.children.indexOf(child);
      if (at >= 0) parent.children.splice(at, 1);
      detach(child);
      emit({ op: "remove", parent: parent.id, child: child.id });
    },

    commitUpdate: (instance, _type, _prevProps, nextProps) => {
      const serialized = serializeProps(instance, nextProps);

      // Nothing observable changed, so nothing needs to cross the wire. This
      // is what makes re-renders driven by fresh closures free.
      if (propsEqual(instance.props, serialized)) return;

      instance.props = serialized;
      emit({ op: "updateProps", id: instance.id, props: serialized });
    },

    commitTextUpdate: (instance, _oldText, newText) => {
      instance.text = newText;
      emit({ op: "updateText", id: instance.id, text: newText });
    },

    clearContainer: (target) => {
      for (const child of target.children) detach(child);
      target.children = [];
      emit({ op: "clear", id: target.id });
    },

    detachDeletedInstance: () => {},

    preparePortalMount: () => {},

    scheduleTimeout: setTimeout,
    cancelTimeout: clearTimeout,
    noTimeout: -1,

    getCurrentUpdatePriority: () => DefaultEventPriority,
    resolveUpdatePriority: () => DefaultEventPriority,
    setCurrentUpdatePriority: () => {},

    getInstanceFromNode: () => null,
    getInstanceFromScope: () => null,
    beforeActiveInstanceBlur: () => {},
    afterActiveInstanceBlur: () => {},
    prepareScopeUpdate: () => {},

    // Suspense and transition hooks. Nothing here suspends on the host, so
    // these are the trivial implementations that let commits proceed.
    maySuspendCommit: () => false,
    startSuspendingCommit: () => {},
    suspendInstance: () => {},
    waitForCommitToBeReady: () => null,
    preloadInstance: () => true,
    requestPostPaintCallback: () => {},
    shouldAttemptEagerTransition: () => false,
    trackSchedulerEvent: () => {},
    resolveEventType: () => null,
    resolveEventTimeStamp: () => -1.1,
    resetFormInstance: () => {},
    NotPendingTransition: null,
    HostTransitionContext: {
      $$typeof: Symbol.for("react.context"),
      Provider: null,
      Consumer: null,
      _currentValue: null,
      _currentValue2: null,
      _threadCount: 0,
    } as never,
  };

  return { container, callbacks, hostConfig };
}
