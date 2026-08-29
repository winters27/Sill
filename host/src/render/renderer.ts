import Reconciler from "react-reconciler";
import { ConcurrentRoot } from "react-reconciler/constants";
import type { ReactElement } from "react";
import { createRendererState, type RendererHooks } from "./hostConfig";
import { snapshot, type Op } from "./nodes";
import type { CallbackManager } from "./callbacks";

export interface Renderer {
  render(element: ReactElement): void;
  /** Full tree, for a UI attaching to an already-running command. */
  snapshot(): unknown;
  callbacks: CallbackManager;
  unmount(): void;
}

export function createRenderer(hooks: RendererHooks): Renderer {
  const state = createRendererState(hooks);
  const reconciler = Reconciler(state.hostConfig);

  const onRecoverableError = (error: Error) => {
    // Recoverable in React's sense, still worth surfacing: it usually means an
    // extension rendered something invalid and got silently patched over.
    process.stderr.write(`[sill] recoverable react error: ${error.stack ?? error.message}\n`);
  };

  const root = reconciler.createContainer(
    state.container,
    ConcurrentRoot,
    null,
    false,
    null,
    "",
    onRecoverableError,
    null,
  );

  /**
   * React 19 removed legacy mode, so a bare updateContainer only schedules
   * work and the commit lands whenever the scheduler gets to it. The host has
   * to have the ops before it can reply, so renders go through the sync path.
   *
   * @types/react-reconciler 0.32.1 still describes the older `flushSync`,
   * which does not exist on the 0.32.0 runtime. The real API is
   * `updateContainerSync` followed by `flushSyncWork`, so it is reached
   * through a narrow cast rather than by trusting the declarations.
   */
  const sync = reconciler as unknown as {
    updateContainerSync(element: ReactElement | null, root: unknown, parent: null, cb: null): void;
    flushSyncWork(): void;
  };

  const renderSync = (element: ReactElement | null) => {
    sync.updateContainerSync(element, root, null, null);
    sync.flushSyncWork();
  };

  return {
    render(element: ReactElement) {
      renderSync(element);
    },

    snapshot() {
      return state.container.children.map(snapshot);
    },

    callbacks: state.callbacks,

    unmount() {
      renderSync(null);
    },
  };
}

export type { Op, RendererHooks };
