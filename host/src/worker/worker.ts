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
import { setBridge, type Environment } from "../api/bridge";
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

  let stack: ReactElement[] = [];
  let renderer: ReturnType<typeof createRenderer> | undefined;

  const renderTop = () => {
    const top = stack[stack.length - 1];
    if (!renderer || !top) return;
    renderer.render(top);
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
        push: (view) => {
          stack.push(view as ReactElement);
          renderTop();
        },
        pop: () => {
          if (stack.length > 1) stack.pop();
          renderTop();
        },
      },
      environment,
      preferences: data.preferences,
      launchArguments: data.launchArguments,
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

    stack = [createElement(entry as never, launchProps as never)];
    renderTop();
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
