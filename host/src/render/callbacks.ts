/**
 * Handler registry.
 *
 * Props can hold functions (onAction, onChange, onSearchTextChange) and those
 * cannot cross the wire. Each one is swapped for an opaque id; the UI sends
 * that id back when the user activates it and the function runs here.
 *
 * Removal is deferred by one commit. React frequently detaches a handler and
 * immediately reattaches an equivalent one during the same update, and an
 * in-flight activation from the UI can arrive just after the detach. Dropping
 * ids immediately turns that race into a dead button.
 */

export type Callback = (...args: unknown[]) => unknown;

export class CallbackManager {
  private nextId = 1;
  private readonly callbacks = new Map<string, Callback>();
  private pendingRemoval = new Set<string>();

  register(fn: Callback): string {
    const id = `h${this.nextId++}`;
    this.callbacks.set(id, fn);
    return id;
  }

  /** Keeps an existing id alive rather than minting a new one on re-render. */
  rebind(id: string, fn: Callback): void {
    this.callbacks.set(id, fn);
    this.pendingRemoval.delete(id);
  }

  deferRemoval(id: string): void {
    this.pendingRemoval.add(id);
  }

  /** Called once per commit, so anything still unclaimed is genuinely gone. */
  flushDeferredRemovals(): void {
    for (const id of this.pendingRemoval) this.callbacks.delete(id);
    this.pendingRemoval.clear();
  }

  invoke(id: string, args: unknown[]): unknown {
    const fn = this.callbacks.get(id);
    if (!fn) {
      throw new Error(`no handler registered for "${id}"`);
    }
    return fn(...args);
  }

  has(id: string): boolean {
    return this.callbacks.has(id);
  }
}
