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

/**
 * The most memory one command may hold before V8 stops it.
 *
 * A backstop, not a budget. It is the answer to "one extension has a leak and
 * the machine is swapping", and nothing else: crossing it is not a policy
 * decision Sill makes, it is V8 refusing to grow the heap and the thread
 * ending where it stands. **Whatever the person was doing in that command is
 * lost**, which is why this sits so far above anything real rather than
 * anywhere near it.
 *
 * Measured before it was chosen, with `run-extension.mjs --measure` against
 * the five real extensions the view gate draws. The heaviest is Emoji Search,
 * which builds a list of 1,898 rows out of a bundled dataset and settles at
 * 63 MB. Kill Process, which reads the whole process table, is 31 MB. Hacker
 * News is 16 MB and the two generators are 11 MB each, which is close to what
 * an empty worker costs.
 *
 * 512 MB is eight times the worst of those, so an extension reaching it has
 * stopped doing arithmetic on the size of its own input.
 *
 * It was 1000 MB, which is a number nobody had measured against anything. Two
 * commands at that were more memory than the whole launcher is allowed to be.
 *
 * Nothing warns before this. A cap that killed at one number and nagged at a
 * lower one would be two policies, and the lower one would be the one people
 * saw, on extensions that were working. What the panel does instead is say
 * what each extension is using, and leave the reading to the reader.
 *
 * Overridable for the same reason the runaway budget is: proving that crossing
 * it produces a sentence somebody can read should not mean allocating half a
 * gigabyte on the machine running the tests. A test that needs that is a test
 * nobody runs.
 */
const WORKER_MAX_HEAP_MB = Number(process.env.SILL_WORKER_HEAP_MB ?? 512);

/** Whether V8 ended this worker because it would not fit. */
function outOfMemory(err: unknown): boolean {
  return (err as { code?: unknown } | null)?.code === "ERR_WORKER_OUT_OF_MEMORY";
}

/**
 * What somebody is told when a command is stopped for using too much memory.
 *
 * Read as the second half of a sentence, because the launcher puts the
 * command's own title in front of it: "Search Emoji stopped: it used more
 * than...". Saying the name here as well would say it twice.
 *
 * No stack, no error code, no mention of V8 or of old generation sizes. The
 * person reading this did not choose the runtime and cannot act on any of it.
 * What they can act on is which extension it was and whether to keep it, so
 * that is what it says.
 *
 * It is also written to this process's stderr, which Rust reads into Sill's
 * log, because a `no-view` command has no screen for the launcher to put this
 * on: a background command dying of memory would otherwise leave nothing
 * anywhere. That is the host's own stderr and not a worker's, so it is the
 * stream that is already drained rather than one of the two that are not.
 */
function ranOutOfMemory(session: Session): string {
  process.stderr.write(
    `${session.extension}/${session.command}: stopped after using more than ` +
      `${WORKER_MAX_HEAP_MB} MB of memory\n`,
  );

  return (
    `it used more than ${WORKER_MAX_HEAP_MB} MB of memory, so Sill stopped it. ` +
    "That is far more than an extension needs, so it has most likely leaked " +
    "rather than done something big. Nothing else in Sill was affected."
  );
}

/** How long a worker gets to unmount cleanly before it is terminated. */
const SHUTDOWN_GRACE_MS = 5000;

/**
 * How long a worker gets to say how much memory it is using.
 *
 * Short, because the answer is a property read on a thread that is nearly
 * always asleep and comes back in under a millisecond. Anything approaching
 * this deadline is a worker that is busy, and "busy" is the reading rather
 * than something to wait out: a person who opened the panel is looking at it
 * now, and a screen that stalls for a second per extension is worse than one
 * that says an extension did not answer.
 */
const HEAP_ANSWER_MS = 250;

/**
 * How long the first reading of a session watches before it says anything.
 *
 * A share of a core is a rate and needs a window. Long enough that a worker
 * doing real work registers, short enough that a settings panel does not
 * visibly hesitate on its way in, and paid once per session rather than once
 * per reading.
 */
const CORE_WINDOW_MS = 200;

/** A delay that can never be the reason this process stays alive. */
function pause(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms).unref());
}

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

/** What a worker says about its own memory, in bytes. */
interface Heap {
  used: number;
  total: number;
}

/**
 * What unloading a command answers with.
 *
 * `ok` is what it always was: whether there was anything there to unload. The
 * memory figure rides along because this is the last moment anybody can ask
 * for it, and it is the one reading that lets two extensions be compared
 * after the fact.
 *
 * `null` for a worker that did not answer, which is the same meaning it has
 * everywhere else here rather than a zero somebody would quote.
 */
interface Unloaded {
  ok: boolean;
  heap_bytes: number | null;
}

/** One worker's readings, as they go back to Rust. */
interface Reading {
  session_id: string;
  /** Bytes in use, or null when the worker did not answer in time. */
  heap_bytes: number | null;
  heap_limit_bytes: number | null;
  /** How much of one processor core it has used since the last reading. */
  core_percent: number;
  answering: boolean;
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
  /**
   * The same reading again, for whoever is asking rather than for the runaway
   * watch.
   *
   * Two baselines rather than one, because they are read on different clocks
   * and sharing would corrupt both. The watch adds a fixed `RUNAWAY_CHECK_MS`
   * for every sample that comes back hot, so a reader moving the baseline
   * between two of its ticks would have it credit five seconds of pinning to a
   * window that was half a second long. Two objects cost two pointers and
   * nothing else.
   */
  eluSeen?: ReturnType<Worker["performance"]["eventLoopUtilization"]>;
  /** Who this is, so a message about it can say so in the person's words. */
  extension: string;
  command: string;
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
    this.rpc.handle("Manager/setCapabilities", (p) => this.setCapabilities(p));
    this.rpc.handle("Manager/diagnostics", () => this.diagnostics());

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

  /**
   * What every loaded command is costing, right now.
   *
   * Asked, never watched. Everything a person would want here could be sampled
   * on a timer into a tidy little history, and that timer would be a wakeup on
   * a machine where nothing is happening, which is the one thing this whole
   * project refuses to spend. Somebody opening the Extensions panel is a
   * reason to look; nothing else is. Between two of those readings this costs
   * exactly nothing.
   *
   * Nothing here starts a host either: Rust only asks a host that is already
   * up, so opening the panel on a machine that has not run an extension today
   * does not spawn Node to be told there is nothing to report.
   */
  private async diagnostics(): Promise<{ workers: Reading[] }> {
    const workers = await Promise.all(
      [...this.sessions.values()].map((session) => this.readingFor(session)),
    );

    return { workers };
  }

  /** One worker's reading: its share of a core from here, its heap from it. */
  private async readingFor(session: Session): Promise<Reading> {
    /*
     * A share of a core is a rate, so it needs two readings and a gap.
     *
     * The first time anybody asks about a session there is no earlier reading
     * to measure against, and the figure the platform hands back instead is
     * cumulative since the worker started. **For a command somebody has just
     * opened that is close to 100%**, because starting is the busiest thing a
     * worker ever does, and a panel saying an extension is using a whole
     * processor core the moment it appears would be wrong about every
     * extension there is.
     *
     * So the first reading opens a window and waits it out. Every session is
     * read at once, so this is one wait for the whole panel rather than one
     * per extension, and it is paid only the first time each is looked at.
     */
    const opening = session.eluSeen === undefined;

    if (opening) {
      session.eluSeen = session.worker.performance.eventLoopUtilization();
    }

    const [heap] = await Promise.all([
      this.askHeap(session),
      opening ? pause(CORE_WINDOW_MS) : Promise.resolve(),
    ]);

    const current = session.worker.performance.eventLoopUtilization();
    const since = session.worker.performance.eventLoopUtilization(current, session.eluSeen);
    session.eluSeen = current;

    return {
      session_id: session.id,
      heap_bytes: heap ? heap.used : null,
      // What Sill asked for rather than what V8 reports back, which is the old
      // generation plus the semi spaces and so reads about 40% higher. The cap
      // is the number a worker is stopped at, and it is the only one worth
      // putting next to the usage.
      heap_limit_bytes: WORKER_MAX_HEAP_MB * 1024 * 1024,
      // One decimal place. The difference between 0.4% and 0.7% of a core is
      // not a difference anybody acts on, and the digits after it are noise.
      core_percent: Math.round(since.utilization * 1000) / 10,
      answering: heap !== null,
    };
  }

  /**
   * Asks one worker how much memory it is using, and gives up quickly.
   *
   * The deadline is the point rather than a precaution. **The worker most
   * worth asking is the one that will not answer**: an extension stuck in a
   * loop holds its own event loop, so the request sits in a queue that is
   * never drained. Waiting on that would hang the panel on exactly the
   * extension it exists to name.
   *
   * Not answering is itself a reading, and it is reported as one. The share
   * of a core beside it comes from this thread and always arrives, so a
   * worker that says nothing while pinning a core describes itself perfectly.
   *
   * A worker killed mid-question is the same answer by another route: the
   * exit handler fails everything in flight, and that rejection lands here.
   */
  private askHeap(session: Session): Promise<Heap | null> {
    let timer: NodeJS.Timeout | undefined;

    const gaveUp = new Promise<null>((resolve) => {
      timer = setTimeout(() => resolve(null), HEAP_ANSWER_MS);
      // So a pending question can never be the reason the host stays alive.
      timer.unref();
    });

    return Promise.race([
      session.control.request<Heap>("Lifecycle/heap").catch(() => null),
      gaveUp,
    ]).finally(() => clearTimeout(timer));
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
      extension: String(opts.extension_name ?? opts.extension_id ?? "an extension"),
      command: String(opts.command_name ?? ""),
    };
    this.sessions.set(sessionId, session);
    this.watchForRunaways();

    worker.on("message", (data: string) => control.receive(data));

    worker.on("error", (err: Error) => {
      this.rpc.emit("Manager/extensionCrash", {
        session_id: sessionId,
        reason: outOfMemory(err) ? ranOutOfMemory(session) : describeError(err),
      });
    });

    worker.on("exit", (code) => {
      if (code !== 0 && this.sessions.has(sessionId)) {
        this.rpc.emit("Manager/extensionCrash", {
          session_id: sessionId,
          reason: `worker exited with code ${code}`,
        });
      }

      /*
       * Nothing is waiting on this worker any more, so nothing should be left
       * waiting for it.
       *
       * A control request whose worker was terminated resolves never: the
       * thread that was going to answer it does not exist. Held promises are
       * a leak on their own, and they are worse than that here, because the
       * caller most likely to be holding one is a diagnostics read on the
       * extension that had just been killed for misbehaving.
       */
      session.control.rejectAllPending("the extension was stopped");

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
      // The thing this command was run on, when it was run as an action. Rust
      // leaves the field out entirely for an ordinary launch, and absent is
      // what `@sill/api`'s `actionTarget` answers with.
      sillObject: (opts.on ?? undefined) as LaunchData["sillObject"],
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

  /**
   * Tells a running command what it is allowed to reach now.
   *
   * Capabilities used to arrive once, in `Lifecycle/launch`, and never again.
   * That made revoking a half-measure: Settings wrote the file, the next
   * launch honoured it, and the worker the person was looking at went on using
   * the permission they had just taken away until something unloaded it.
   *
   * On the control channel rather than the extension's own, because this is
   * the manager telling the worker about itself and no extension code is
   * entitled to see it, let alone answer it.
   *
   * `false` for a session that is gone, like every other call here. A revoke
   * arriving a moment after a command closed is not a failure; there is simply
   * nothing left to tell.
   */
  private setCapabilities(params: RpcParams): boolean {
    const session = this.sessions.get(String(params.session_id));
    if (!session) return false;

    session.control.emit("Lifecycle/capabilities", {
      capabilities: Array.isArray(params.capabilities) ? params.capabilities : [],
    });
    return true;
  }

  private async unload(sessionId: string): Promise<Unloaded> {
    const session = this.sessions.get(sessionId);
    if (!session) return { ok: false, heap_bytes: null };

    /*
     * What it was holding, asked on the way out.
     *
     * The last chance there is. Once this returns, the worker is gone and its
     * memory is a number nobody can ever recover, and closing a command is by
     * far the most common way one ends.
     *
     * This is the answer to a problem the live reading cannot solve. Only one
     * command is usually loaded at a time, so a panel showing memory for what
     * is running is a panel showing one figure, and one figure is not a
     * comparison. Somebody who opens four extensions and then goes looking for
     * the expensive one has closed three of them by the time they look.
     *
     * Not a timer, and not a sample: it rides a round trip that was already
     * being made, on a path a person triggers by pressing Escape.
     */
    const heap = await this.askHeap(session);

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
    return { ok: true, heap_bytes: heap ? heap.used : null };
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
