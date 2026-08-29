/**
 * Extension host entry point.
 *
 * Spawned by the Rust side with piped stdio. Serves the Manager layer and
 * relays each extension's API traffic to and from its worker thread.
 *
 * The same bundled file is also the worker body, selected by isMainThread, so
 * it has to stay a single artifact.
 */

import { isMainThread, Worker } from "node:worker_threads";
import { randomUUID } from "node:crypto";
import { encodeFrame, FrameDecoder } from "./proto/framing";
import { RpcPeer, type RpcParams } from "./proto/rpc";
import { workerMain, type LaunchData } from "./worker/worker";

/**
 * Renders anything throwable as readable text. Rejections arriving over RPC
 * are plain error objects rather than Error instances, and String() on those
 * yields "[object Object]", which hides exactly the detail worth having.
 */
function describeError(err: unknown): string {
  if (err instanceof Error) return err.stack ?? err.message;
  if (typeof err === "string") return err;
  if (err && typeof err === "object") {
    const e = err as { message?: unknown; data?: unknown };
    if (typeof e.message === "string") {
      return typeof e.data === "string" ? `${e.message}\n${e.data}` : e.message;
    }
    try {
      return JSON.stringify(err);
    } catch {
      return Object.prototype.toString.call(err);
    }
  }
  return String(err);
}

/** Generous per-worker cap so one extension cannot exhaust system memory. */
const WORKER_MAX_HEAP_MB = 1000;

/** How long a worker gets to unmount cleanly before it is terminated. */
const SHUTDOWN_GRACE_MS = 5000;

interface Session {
  id: string;
  worker: Worker;
  control: RpcPeer;
  ready: boolean;
  /** Buffered until Rust says it is ready, so nothing is dropped on startup. */
  queued: string[];
}

class ExtensionHost {
  private readonly rpc: RpcPeer;
  private readonly decoder = new FrameDecoder();
  private readonly sessions = new Map<string, Session>();

  /**
   * One idle worker is kept spun up. Thread creation plus module evaluation is
   * the dominant cost of opening a command, and paying it before the user asks
   * is what keeps a launch feeling instant.
   */
  private spare: Worker | undefined;

  constructor() {
    this.rpc = new RpcPeer((data) => process.stdout.write(encodeFrame(data)));

    this.rpc.handle("Manager/load", (p) => this.load(p));
    this.rpc.handle("Manager/ready", (p) => this.markReady(String(p.session_id)));
    this.rpc.handle("Manager/unload", (p) => this.unload(String(p.session_id)));
    this.rpc.handle("Manager/messageExtension", (p) => this.toExtension(p));

    process.stdin.on("data", (chunk: Buffer) => {
      for (const frame of this.decoder.push(chunk)) {
        this.rpc.receive(frame);
      }
    });

    process.stdin.on("end", () => this.shutdownAll());

    this.spare = this.spawnWorker();
  }

  private spawnWorker(): Worker {
    return new Worker(__filename, {
      resourceLimits: { maxOldGenerationSizeMb: WORKER_MAX_HEAP_MB },
      stdout: true,
      stderr: true,
    });
  }

  /** Takes the warm worker and immediately starts warming the next one. */
  private acquireWorker(): Worker {
    const worker = this.spare ?? this.spawnWorker();
    this.spare = this.spawnWorker();
    return worker;
  }

  private async load(params: RpcParams): Promise<{ session_id: string }> {
    const opts = (params.opts ?? {}) as Record<string, unknown>;
    const sessionId = randomUUID();
    const worker = this.acquireWorker();

    const control = new RpcPeer((data) => worker.postMessage(data));
    const session: Session = { id: sessionId, worker, control, ready: false, queued: [] };
    this.sessions.set(sessionId, session);

    worker.on("message", (data: string) => control.receive(data));

    worker.on("error", (err: Error) => {
      this.rpc.emit("Manager/extensionCrash", {
        session_id: sessionId,
        reason: describeError(err),
      });
    });

    worker.on("exit", (code) => {
      if (code !== 0 && this.sessions.has(sessionId)) {
        this.rpc.emit("Manager/extensionCrash", {
          session_id: sessionId,
          reason: `worker exited with code ${code}`,
        });
      }
      this.sessions.delete(sessionId);
    });

    // Everything the extension says is relayed up, or held until ready.
    control.on("Lifecycle/message", (p: RpcParams) => {
      const payload = String(p.payload);
      if (!session.ready) {
        session.queued.push(payload);
        return;
      }
      this.rpc.emit("Manager/extensionMessage", { session_id: sessionId, payload });
    });

    control.on("Lifecycle/unloadRequested", () => {
      void this.unload(sessionId);
    });

    const data: LaunchData = {
      entrypoint: String(opts.entrypoint ?? ""),
      extensionName: String(opts.extension_name ?? ""),
      commandName: String(opts.command_name ?? ""),
      mode: opts.mode === "NoView" ? "no-view" : "view",
      assetsPath: String(opts.assets_path ?? ""),
      supportPath: String(opts.support_path ?? ""),
      preferences: (opts.preferences ?? {}) as Record<string, unknown>,
      launchArguments: (opts.arguments ?? {}) as Record<string, unknown>,
      launchContext: opts.launch_context,
      fallbackText: typeof opts.fallbackText === "string" ? opts.fallbackText : undefined,
      isDevelopment: opts.env === "Development",
      launchType: opts.launch_type === "Background" ? "background" : "userInitiated",
    };

    // Not awaited: the extension's first render must be free to arrive while
    // Rust is still storing the session id. That is what the queue is for.
    void control.request("Lifecycle/launch", { data: data as unknown as RpcParams }).catch(
      (err: unknown) => {
        this.rpc.emit("Manager/extensionCrash", {
          session_id: sessionId,
          reason: describeError(err),
        });
      },
    );

    return { session_id: sessionId };
  }

  /** Rust has stored the id, so buffered messages can be released. */
  private markReady(sessionId: string): boolean {
    const session = this.sessions.get(sessionId);
    if (!session) return false;

    session.ready = true;
    for (const payload of session.queued) {
      this.rpc.emit("Manager/extensionMessage", { session_id: sessionId, payload });
    }
    session.queued = [];
    return true;
  }

  private toExtension(params: RpcParams): boolean {
    const session = this.sessions.get(String(params.session_id));
    if (!session) return false;
    session.control.emit("Lifecycle/message", { payload: String(params.payload) });
    return true;
  }

  private async unload(sessionId: string): Promise<boolean> {
    const session = this.sessions.get(sessionId);
    if (!session) return false;

    this.sessions.delete(sessionId);

    try {
      await Promise.race([
        session.control.request("Lifecycle/shutdown"),
        new Promise((resolve) => setTimeout(resolve, SHUTDOWN_GRACE_MS)),
      ]);
    } catch {
      // A worker that cannot answer is one that gets terminated below.
    }

    await session.worker.terminate();
    return true;
  }

  private shutdownAll(): void {
    for (const id of [...this.sessions.keys()]) void this.unload(id);
    void this.spare?.terminate();
  }
}

function main(): void {
  if (!isMainThread) {
    workerMain();
    return;
  }

  // stdout carries framed protocol traffic. A TTY means someone ran this by
  // hand and is about to see binary garbage, so refuse instead.
  if (process.stdout.isTTY) {
    process.stderr.write("sill-host: not runnable from a TTY; it is spawned with piped stdio.\n");
    process.exit(1);
  }

  new ExtensionHost();
}

main();
