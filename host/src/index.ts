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
import { StringDecoder } from "node:string_decoder";
import type { Readable } from "node:stream";
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

/**
 * How busy a worker has to be before it counts as running away.
 *
 * Event loop utilisation, not processor time: a thread pinned at 0.95 is one
 * that never yields, which in a launcher extension means a loop rather than
 * work. Real extensions are almost entirely idle, waking to render and to
 * answer a request.
 */
const RUNAWAY_UTILISATION = 0.95;

/**
 * How long it has to stay there before it is stopped, and how often that is
 * looked at.
 *
 * Thirty seconds is deliberately far past anything legitimate. The cost of
 * being wrong is somebody's working extension killed under them, so the bar is
 * set where a false positive is close to impossible rather than where a
 * runaway is caught soonest.
 *
 * Both are overridable so a test can use a budget it can actually wait for.
 * The alternative is a thirty second test, which is a test nobody runs.
 */
const RUNAWAY_MS = Number(process.env.SILL_RUNAWAY_MS ?? 30_000);
const RUNAWAY_CHECK_MS = Number(process.env.SILL_RUNAWAY_CHECK_MS ?? 5_000);

/**
 * How much one command's console output is worth carrying.
 *
 * An author reading back their own `console.log` is what this exists for, and
 * nobody writes sixty-four kilobytes of that. An extension in a loop writes it
 * in about a second, and every line of it ends in Sill's log on a machine that
 * is not being debugged. So the budget sits where the first case never reaches
 * it and the second stops being paid for.
 */
const CONSOLE_BUDGET = 64 * 1024;

/**
 * The longest line carried whole.
 *
 * A line has no bound of its own: an extension logging a document it fetched
 * writes one of a megabyte. Cut rather than dropped, because the beginning is
 * where the answer usually is.
 */
const CONSOLE_LINE = 2000;

/**
 * Carries what an extension prints to somewhere a person can read it.
 *
 * Workers are created with `stdout` and `stderr` set, and that does not mean
 * "let it through": it means Node hands this process two streams instead of
 * forwarding the output. Nothing read them, so **every `console.log` an
 * extension wrote went into a buffer nobody would ever drain**. The author saw
 * nothing wherever they looked, and the text was held in memory for as long as
 * the worker lived.
 *
 * Letting Node forward it instead is not available. This process's stdout
 * carries framed protocol traffic, and an extension printing into the middle
 * of a frame would corrupt it, which is why the streams were diverted in the
 * first place. So everything goes to stderr, which the Rust side reads and
 * writes into Sill's log.
 *
 * One of these per worker, so the budget below is per command rather than
 * shared, and so a quiet extension pays nothing for a noisy one.
 */
class ExtensionOutput {
  /** Named when a command is loaded, which is after the worker is spawned. */
  private who = "an extension";

  private left = CONSOLE_BUDGET;

  /** Says whose output this is, as soon as there is an answer. */
  belongsTo(extension: string, command: string): void {
    this.who = `${extension}/${command}`;
  }

  /**
   * Reads one of the worker's streams for as long as it lasts.
   *
   * A decoder rather than `String(chunk)`, because a chunk boundary can fall
   * inside a character and the two halves decoded separately are two
   * replacement characters rather than the letter somebody wrote.
   */
  carry(stream: Readable | null): void {
    if (!stream) return;

    const decoder = new StringDecoder("utf8");
    let held = "";

    stream.on("data", (chunk: Buffer) => {
      held += decoder.write(chunk);

      for (let end = held.indexOf("\n"); end !== -1; end = held.indexOf("\n")) {
        this.say(held.slice(0, end));
        held = held.slice(end + 1);
      }

      // Output with no newline in it is still output, and holding it would put
      // back the unbounded buffer this whole class exists to drain.
      if (held.length >= CONSOLE_LINE) {
        this.say(held);
        held = "";
      }
    });
  }

  private say(line: string): void {
    if (this.left <= 0) return;

    const text = line.trimEnd();
    // A bare `console.log()` writes one newline. It says nothing, and a log
    // full of blank lines is harder to read than one without them.
    if (text === "") return;

    const shown = text.length > CONSOLE_LINE ? `${text.slice(0, CONSOLE_LINE)} (cut)` : text;
    this.left -= shown.length;

    process.stderr.write(`${this.who}: ${shown}\n`);

    // Said once, and only by the worker that spent the budget, so the reason
    // its output stopped is in the same place its output was.
    if (this.left <= 0) {
      process.stderr.write(
        `${this.who}: silenced after ${CONSOLE_BUDGET / 1024}KB of output. ` +
          "An extension writing that much is logging in a loop.\n",
      );
    }
  }
}

/** A worker, with the relay carrying whatever it prints. */
interface Warm {
  worker: Worker;
  output: ExtensionOutput;
}

interface Session {
  id: string;
  worker: Worker;
  control: RpcPeer;
  ready: boolean;
  /** Buffered until Rust says it is ready, so nothing is dropped on startup. */
  queued: string[];
  /** The last utilisation reading, to take the next one against. */
  elu?: ReturnType<Worker["performance"]["eventLoopUtilization"]>;
  /** How long it has been pinned. Reset by a single quiet sample. */
  hotMs: number;
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
  private spare: Warm | undefined;

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

  private spawnWorker(): Warm {
    const worker = new Worker(__filename, {
      resourceLimits: { maxOldGenerationSizeMb: WORKER_MAX_HEAP_MB },
      stdout: true,
      stderr: true,
    });

    // Read from the moment the worker exists rather than from the moment a
    // command is loaded into it. A spare sits here warm, and anything it
    // printed before it was claimed would otherwise be the buffer again.
    const output = new ExtensionOutput();
    output.carry(worker.stdout);
    output.carry(worker.stderr);

    return { worker, output };
  }

  /** Takes the warm worker and immediately starts warming the next one. */
  private acquireWorker(): Warm {
    const warm = this.spare ?? this.spawnWorker();
    this.spare = this.spawnWorker();
    return warm;
  }

  /** Ticks only while something is loaded. */
  private runaway?: NodeJS.Timeout;

  /**
   * Starts watching, if it is not already and there is anything to watch.
   *
   * Started and stopped with the sessions rather than left running, because a
   * timer waking every few seconds to look at an empty map is exactly the idle
   * cost Sill refuses to pay elsewhere. `unref` on top of that, so it can
   * never be the reason the host process stays alive.
   */
  private watchForRunaways(): void {
    if (this.runaway || this.sessions.size === 0) return;

    this.runaway = setInterval(() => this.stopRunaways(), RUNAWAY_CHECK_MS);
    this.runaway.unref();
  }

  /** Stops watching once the last session has gone. */
  private stopWatchingRunaways(): void {
    if (this.sessions.size > 0 || !this.runaway) return;

    clearInterval(this.runaway);
    this.runaway = undefined;
  }

  /**
   * Ends a worker that has done nothing but spin.
   *
   * An extension in a loop pins a core for as long as Sill runs and nothing
   * notices: it never crashes, never finishes, and the launcher it is
   * attached to is a program whose whole claim is that it costs nothing while
   * idle. This is the one thing in the extension host that stops that.
   *
   * The first sample of a session is a baseline and never a verdict, and one
   * quiet sample clears the count, so a burst of real work has to be sustained
   * past the whole budget to be treated as a runaway.
   */
  private stopRunaways(): void {
    for (const session of [...this.sessions.values()]) {
      const previous = session.elu;
      const current = session.worker.performance.eventLoopUtilization();
      session.elu = current;

      if (!previous) continue;

      const since = session.worker.performance.eventLoopUtilization(current, previous);

      session.hotMs = since.utilization >= RUNAWAY_UTILISATION ? session.hotMs + RUNAWAY_CHECK_MS : 0;

      if (session.hotMs < RUNAWAY_MS) continue;

      // Removed first, so the `exit` handler's "did anyone still want this?"
      // check stays false and the crash is reported once, with the reason that
      // is actually true, rather than twice with the second saying only that a
      // worker exited.
      this.sessions.delete(session.id);
      void session.worker.terminate();

      this.rpc.emit("Manager/extensionCrash", {
        session_id: session.id,
        reason:
          `stopped after using a whole processor core for ${Math.round(RUNAWAY_MS / 1000)}s ` +
          `without pausing. An extension that does this is looping rather than working.`,
      });
    }

    this.stopWatchingRunaways();
  }

  private async load(params: RpcParams): Promise<{ session_id: string }> {
    const opts = (params.opts ?? {}) as Record<string, unknown>;
    const sessionId = randomUUID();
    const warm = this.acquireWorker();
    const worker = warm.worker;

    const control = new RpcPeer((data) => worker.postMessage(data));
    const session: Session = {
      id: sessionId,
      worker,
      control,
      ready: false,
      queued: [],
      hotMs: 0,
    };
    this.sessions.set(sessionId, session);
    this.watchForRunaways();

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
      this.stopWatchingRunaways();
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
      // Forwarded, and it has to be. The struct this replaced was carried all
      // the way here in Rust and then never read on this side, which is how a
      // permission model can exist in the types and enforce nothing.
      capabilities: Array.isArray(opts.capabilities)
        ? (opts.capabilities as string[])
        : [],
    };

    // Before the launch, which is the first moment the extension can print
    // anything: its module body runs inside that call.
    warm.output.belongsTo(data.extensionName, data.commandName);

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
    this.stopWatchingRunaways();

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
    void this.spare?.worker.terminate();
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
