/**
 * Runs one extension command inside a worker thread.
 *
 * Two logical channels share the single postMessage link to the manager:
 * lifecycle control, and the extension's own API traffic carried as an opaque
 * string. That mirrors the split on the stdio side, so the manager never has
 * to understand what an extension is saying, only who it is saying it to.
 */

import { parentPort } from "node:worker_threads";
import { createElement, type ReactElement } from "react";
import { RpcPeer, type RpcParams } from "../proto/rpc";
import { createRenderer } from "../render/renderer";
import { getBridge, setBridge, type Environment } from "../api/bridge";
import { gateGlobals, patchRequire } from "./patch-require";

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

    patchRequire(data.capabilities ?? []);
    gateGlobals(data.capabilities ?? []);

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

  control.handle("Lifecycle/shutdown", () => {
    renderer?.unmount();
    return true;
  });
}
