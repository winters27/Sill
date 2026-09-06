/**
 * Runs one extension command inside a worker thread.
 *
 * Two logical channels share the single postMessage link to the manager:
 * lifecycle control, and the extension's own API traffic carried as an opaque
 * string. That mirrors the split on the stdio side, so the manager never has
 * to understand what an extension is saying, only who it is saying it to.
 */

import { parentPort, workerData } from "node:worker_threads";
import { getHeapStatistics } from "node:v8";
import { createElement, type ReactElement } from "react";
import { RpcPeer, type RpcParams } from "../proto/rpc";
import { createRenderer } from "../render/renderer";
import { getBridge, setBridge, type Environment, type SillObject } from "../api/bridge";
import { gateGlobals, Held, patchRequire } from "./patch-require";
import { makeAsker } from "./ask";

export interface LaunchData {
  entrypoint: string;
  extensionName: string;
  commandName: string;
  mode: "view" | "no-view";
  assetsPath: string;
  supportPath: string;
  preferences: Record<string, unknown>;
  launchArguments: Record<string, unknown>;
  launchContext?: unknown;
  fallbackText?: string;
  isDevelopment: boolean;
  launchType: "userInitiated" | "background";
  /**
   * What this extension has been allowed to reach, as Rust spells it.
   *
   * Absent means nothing was granted, which is the safe reading: an older host
   * or a caller that forgot the field gets an extension that cannot touch the
   * disk, rather than one that can.
   */
  capabilities?: string[];
  /**
   * Whether there is somebody to ask when a gate refuses.
   *
   * True when Sill started this command: a permission the extension does not
   * hold goes on a card, and the worker waits for the answer. Absent for a
   * script or a test driving the host, which has nobody to put a card in
   * front of; there a gate refuses at once, as every gate did before asking
   * existed, rather than waiting on an answer that cannot come.
   */
  asks?: boolean;
  /**
   * The thing this command was run on, when it was run as an action.
   *
   * Absent for an ordinary launch, and absent is what `@sill/api` reports:
   * a command picked off the root list was run on nothing.
   */
  sillObject?: SillObject;
}

export function workerMain(): void {
  const port = parentPort;
  if (!port) throw new Error("sill: worker started without a parent port");

  // Lifecycle channel to the manager.
  const control = new RpcPeer((data) => port.postMessage(data));

  // The extension's API channel, tunnelled through the control channel.
  const api = new RpcPeer((data) => control.emit("Lifecycle/message", { payload: data }));

  port.on("message", (data: string) => control.receive(data));

  control.on("Lifecycle/message", (params: RpcParams) => {
    // A reply or event from the host, addressed to the extension.
    api.receive(String(params.payload));
  });

  /**
   * What this command is allowed to reach, for as long as it runs.
   *
   * Made before the launch rather than inside it, because the answer can
   * change while the command is on screen and the gates have to be looking at
   * one object rather than at a copy each.
   */
  const held = new Held();

  /*
   * Somebody changed their mind, and this worker is already running.
   *
   * Rust owns what an extension holds and sends the whole list whenever it
   * changes, so this replaces rather than merges: a revoke is a shorter list,
   * and a merge could only ever add. It arrives as an event because nothing
   * here has an answer to give, and the next gated call is the first one that
   * sees it.
   *
   * Before `Lifecycle/launch` is registered, so a change that arrives during a
   * slow module evaluation is not dropped on the floor.
   */
  control.on("Lifecycle/capabilities", (params: RpcParams) => {
    held.replace(Array.isArray(params.capabilities) ? (params.capabilities as string[]) : []);
  });

  /**
   * The navigation stack, which is a stack of elements and not of workers.
   *
   * One command is one worker holding one React root. Pushing renders a
   * different element into that same root, so React mounts the new screen and
   * unmounts the old one, running its cleanups and releasing its handler ids
   * on the same commit. Popping does it the other way. Nothing is created to
   * be torn down, which is why a screen opened and closed a hundred times
   * costs what one costs.
   */
  interface Screen {
    element: ReactElement;
    /** Raycast tells the pusher when its screen is left again. */
    onPop?: () => void;
  }

  let stack: Screen[] = [];
  let renderer: ReturnType<typeof createRenderer> | undefined;

  /**
   * The id the UI activates to pop, minted once and never released.
   *
   * The UI cannot call into the worker except by activating a handler, and
   * inventing a second channel for one button would be a second protocol.
   * Registered through the ordinary registry so it is an ordinary id, and
   * named to the UI in every navigation event rather than agreed in advance,
   * because a constant written on both sides of a wire is two constants.
   */
  let popHandler = "";

  const renderTop = () => {
    const top = stack[stack.length - 1];
    if (!renderer || !top) return;
    renderer.render(top.element);
  };

  /**
   * Tells the UI how deep the stack is, so Escape knows what it means.
   *
   * After the render rather than before it, so the depth never describes a
   * screen that has not been drawn yet.
   */
  const announce = () => {
    api.emit("UI/navigation", { depth: stack.length, pop: popHandler });
  };

  control.handle("Lifecycle/launch", async (params: RpcParams) => {
    const data = params.data as unknown as LaunchData;

    held.replace(data.capabilities ?? []);

    /*
     * The way to ask, given only when both halves are there: a launch that
     * says somebody will answer, and the shared memory the manager made for
     * the answer to arrive in. The question itself goes up the control
     * channel like any other lifecycle event; only the answer takes the
     * shared route, because the thread that asked is holding `require`.
     */
    const shared = (workerData as { ask?: unknown } | null)?.ask;
    if (data.asks === true && shared instanceof SharedArrayBuffer) {
      held.askWith(
        makeAsker(shared, (needs, plainly) => control.emit("Lifecycle/ask", { needs, plainly })),
      );
    }

    patchRequire(held);
    gateGlobals(held);

    renderer = createRenderer({
      onCommit: (ops) => {
        // Ops go straight out; the UI applies them to its own copy of the tree.
        api.emit("UI/render", { ops: ops as unknown as RpcParams[keyof RpcParams] });
      },
    });

    const environment: Environment = {
      extensionName: data.extensionName,
      commandName: data.commandName,
      commandMode: data.mode,
      assetsPath: data.assetsPath,
      supportPath: data.supportPath,
      isDevelopment: data.isDevelopment,
      raycastVersion: "1.104.0",
      textSize: "medium",
      launchType: data.launchType,
    };

    setBridge({
      request: (method, p) => api.request(method, p),
      emit: (method, p) => api.emit(method, p),
      renderer,
      navigation: {
        push: (view, onPop) => {
          stack.push({ element: view as ReactElement, onPop });
          renderTop();
          announce();
        },
        pop: () => {
          // The bottom of the stack is the command itself, and leaving that is
          // the launcher's decision rather than the extension's.
          if (stack.length <= 1) return;

          const left = stack.pop();
          renderTop();
          announce();
          // After the screen underneath is back, because this is where an
          // extension re-reads what the screen it pushed may have changed.
          left?.onPop?.();
        },
      },
      environment,
      preferences: data.preferences,
      launchArguments: data.launchArguments,
      // The thing this command was run on, when it was reached through an
      // action panel rather than off the root list.
      on: data.sillObject,
      // A getter rather than a value, because `held` is replaced while the
      // command is on screen: somebody revoking a permission in Settings
      // reaches a running worker, and an extension asking what it holds has to
      // get the answer the module gate would give rather than the one that was
      // true at launch.
      get capabilities(): readonly string[] {
        return held.all();
      },
    });

    /*
     * The pop button, as a handler like any other.
     *
     * Minted before the first render so it is the lowest id there is, and
     * never handed to the reconciler, so nothing ever defers its removal and
     * it survives every commit for the life of the worker. One entry in the
     * registry, per command, for as long as the command is running.
     */
    popHandler = renderer.callbacks.register(() => {
      getBridge().navigation.pop();
      return null;
    });

    // The extension bundle is CommonJS, produced by `ray build`.
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const mod = require(data.entrypoint) as { default?: unknown };
    const entry = mod.default ?? mod;

    if (typeof entry !== "function") {
      throw new Error(
        `sill: ${data.extensionName}/${data.commandName} has no default export to run`,
      );
    }

    /*
     * Raycast hands every command a LaunchProps object, and extensions
     * destructure it immediately: `const { x } = props.arguments`. Passing
     * nothing makes `props` undefined and the command throws before it
     * renders, so `arguments` in particular must always be an object.
     */
    const launchProps = {
      arguments: data.launchArguments ?? {},
      launchContext: data.launchContext ?? undefined,
      fallbackText: data.fallbackText ?? undefined,
      launchType: data.launchType,
      draftValues: {},
    };

    if (data.mode === "no-view") {
      await (entry as (props: unknown) => unknown | Promise<unknown>)(launchProps);
      control.emit("Lifecycle/unloadRequested", {});
      return true;
    }

    stack = [{ element: createElement(entry as never, launchProps as never) }];
    renderTop();
    // No announcement here. Depth one with nothing to pop is what the UI
    // already assumes about a command it has just opened, and saying so would
    // be one message per launch that changes nothing.
    return true;
  });

  // The UI activating a handler is the one inbound call the renderer serves.
  api.handle("EventCore/handlerActivated", (params: RpcParams) => {
    const id = String(params.id);
    const args = Array.isArray(params.args) ? (params.args as unknown[]) : [];
    if (!renderer) throw new Error("sill: handler activated before the view existed");
    return renderer.callbacks.invoke(id, args) ?? null;
  });

  /**
   * How much memory this command is using, asked rather than watched.
   *
   * There is no way to read one worker's heap from the thread that created it,
   * so the only honest answer comes from inside. It arrives on the control
   * channel, which is the manager talking to the worker about itself, and
   * **not on a stream**. A worker's stdout and stderr are already diverted to
   * be drained by hand, and giving this its own thing to write into them would
   * put back the unbounded buffer that cost this project a release.
   *
   * `getHeapStatistics` rather than `process.memoryUsage()`, because a worker
   * is a thread and `rss` there is the whole Node process, every other
   * extension included. These three numbers belong to this isolate alone,
   * which is what "what does this extension cost" means.
   *
   * `heap_size_limit` is deliberately not sent. It is not the number that
   * stops a worker: V8 reports it as the old generation plus the semi spaces,
   * so a cap of 512 MB reads back as 704, and a panel saying "11 MB of 704"
   * beside a message saying it will be stopped at 512 would be two answers to
   * one question. The manager knows what it asked for and says that instead.
   *
   * Registered before the launch, so a command whose module body is still
   * evaluating is a worker that has not answered yet rather than one that
   * cannot. It still will not answer while that body runs, because evaluating
   * it holds the event loop, and that is the truth worth reporting.
   */
  control.handle("Lifecycle/heap", () => {
    const heap = getHeapStatistics();
    return { used: heap.used_heap_size, total: heap.total_heap_size };
  });

  control.handle("Lifecycle/shutdown", () => {
    renderer?.unmount();
    return true;
  });
}
