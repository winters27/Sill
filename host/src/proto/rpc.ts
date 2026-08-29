/**
 * JSON-RPC 2.0 peer.
 *
 * A peer both serves methods and calls them, because the host is a server on
 * the stdio channel (Rust drives the manager) and a client toward its workers.
 *
 * Method names are "Service/method", e.g. "Manager/load" or "UI/render".
 * A message carrying an `id` is a request or its response; a message with a
 * `method` and no `id` is a fire-and-forget event.
 */

export type RpcParams = Record<string, unknown>;

export interface RpcError {
  code: number;
  message: string;
  data?: unknown;
}

interface RpcMessage {
  jsonrpc: "2.0";
  id?: number;
  method?: string;
  params?: RpcParams;
  result?: unknown;
  error?: RpcError;
}

/** JSON-RPC reserved codes, plus one of ours for a handler that threw. */
export const RPC_METHOD_NOT_FOUND = -32601;
export const RPC_INTERNAL_ERROR = -32603;
export const RPC_HANDLER_FAILED = -32000;

export type RequestHandler = (params: RpcParams) => unknown | Promise<unknown>;
export type EventHandler = (params: RpcParams) => void;

export interface Subscription {
  unsubscribe(): void;
}

export class RpcPeer {
  private nextId = 1;
  private readonly pending = new Map<
    number,
    { resolve: (v: unknown) => void; reject: (e: unknown) => void }
  >();
  private readonly handlers = new Map<string, RequestHandler>();
  private readonly listeners = new Map<string, EventHandler[]>();

  constructor(private readonly send: (data: string) => void) {}

  /** Registers the implementation of a method this peer serves. */
  handle(method: string, handler: RequestHandler): void {
    this.handlers.set(method, handler);
  }

  /** Subscribes to an inbound event. Several listeners per method are allowed. */
  on(method: string, handler: EventHandler): Subscription {
    const existing = this.listeners.get(method);
    if (existing) existing.push(handler);
    else this.listeners.set(method, [handler]);

    return {
      unsubscribe: () => {
        const list = this.listeners.get(method);
        if (!list) return;
        const at = list.indexOf(handler);
        if (at >= 0) list.splice(at, 1);
      },
    };
  }

  /** Calls a method on the other side and waits for its response. */
  request<T = unknown>(method: string, params: RpcParams = {}): Promise<T> {
    const id = this.nextId++;
    const promise = new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: (v) => resolve(v as T), reject });
    });
    this.write({ jsonrpc: "2.0", id, method, params });
    return promise;
  }

  /** Fires an event. No response is expected and none is waited for. */
  emit(method: string, params: RpcParams = {}): void {
    this.write({ jsonrpc: "2.0", method, params });
  }

  /** Feeds one inbound frame in. Never throws; protocol errors go back as responses. */
  receive(data: string): void {
    let msg: RpcMessage;

    try {
      msg = JSON.parse(data) as RpcMessage;
    } catch (err) {
      // A malformed frame means the stream is suspect, but there is no id to
      // answer, so the only useful thing is to surface it.
      throw new Error(`malformed JSON-RPC frame: ${String(err)}`);
    }

    // A response to something we sent.
    if (msg.id !== undefined && msg.method === undefined) {
      const waiter = this.pending.get(msg.id);
      if (!waiter) return;
      this.pending.delete(msg.id);
      if (msg.error) waiter.reject(msg.error);
      else waiter.resolve(msg.result);
      return;
    }

    // A request we are expected to answer.
    if (msg.id !== undefined && msg.method !== undefined) {
      void this.dispatchRequest(msg.id, msg.method, msg.params ?? {});
      return;
    }

    // An event.
    if (msg.method !== undefined) {
      const listeners = this.listeners.get(msg.method);

      // A notification for a method that is actually a request is a wiring
      // mistake, and the default behaviour (drop it) makes that invisible.
      // Nothing can be answered without an id, but it can at least be said.
      if (!listeners?.length && this.handlers.has(msg.method)) {
        process.stderr.write(
          `[sill] "${msg.method}" arrived as a notification but is a request method; ` +
            `it was dropped. Send it with an "id".\n`,
        );
        return;
      }

      for (const listener of listeners ?? []) {
        listener(msg.params ?? {});
      }
    }
  }

  /** Fails every in-flight request, for when the far side goes away. */
  rejectAllPending(reason: unknown): void {
    for (const [, waiter] of this.pending) waiter.reject(reason);
    this.pending.clear();
  }

  private async dispatchRequest(id: number, method: string, params: RpcParams): Promise<void> {
    const handler = this.handlers.get(method);

    // Unknown methods fail loudly and name themselves. Silence here turns a
    // missing implementation into a mysterious hang on the other side.
    if (!handler) {
      this.write({
        jsonrpc: "2.0",
        id,
        error: { code: RPC_METHOD_NOT_FOUND, message: `no handler for "${method}"` },
      });
      return;
    }

    try {
      const result = await handler(params);
      this.write({ jsonrpc: "2.0", id, result: result ?? null });
    } catch (err) {
      this.write({
        jsonrpc: "2.0",
        id,
        error: {
          code: RPC_HANDLER_FAILED,
          message: err instanceof Error ? err.message : String(err),
          data: err instanceof Error ? err.stack : undefined,
        },
      });
    }
  }

  private write(msg: RpcMessage): void {
    this.send(JSON.stringify(msg));
  }
}
