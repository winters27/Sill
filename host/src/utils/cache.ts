/**
 * A synchronous cache over an asynchronous store.
 *
 * Raycast's `Cache` reads a file on the same thread, so `useCachedState`
 * returns yesterday's value on the very first render. Sill's storage lives in
 * Rust behind an RPC, and no hook can await inside a render, so the honest
 * shape is a map in the worker that fills in from storage shortly after the
 * extension starts.
 *
 * What that costs, said plainly: the first render after launch can show the
 * initial value where Raycast would have shown the cached one, and the cached
 * one arrives a moment later as a re-render. For the lists and preferences
 * these hooks are used for that is a flicker; for anything where it matters,
 * `usePromise` is the honest hook to reach for anyway.
 *
 * Hydration starts when this module is first required, which is when an
 * extension imports `@raycast/utils`. An extension that does not import it
 * pays nothing, which is why this is not done at launch for everybody.
 */
import { LocalStorage } from "../api";

/** Keeps cached state out of the same namespace as an extension's own keys. */
const PREFIX = "sill.cache:";

const held = new Map<string, unknown>();
const listeners = new Map<string, Set<() => void>>();

let hydrating: Promise<void> | undefined;

/** Everything already stored, read once, in the background. */
export function hydrate(): Promise<void> {
  hydrating ??= LocalStorage.allItems()
    .then((items) => {
      for (const [key, value] of Object.entries(items ?? {})) {
        if (!key.startsWith(PREFIX)) continue;

        // A key already written by a hook is newer than what was on disk when
        // this started, so it wins. Hydration must never walk over a value
        // somebody set while it was in flight.
        const bare = key.slice(PREFIX.length);
        if (held.has(bare)) continue;

        held.set(bare, value);
        announce(bare);
      }
    })
    .catch(() => {
      // Storage being unavailable is not worth failing an extension over. The
      // hooks fall back to their initial values, which is what an empty cache
      // means anyway.
    });

  return hydrating;
}

function announce(key: string): void {
  for (const listener of listeners.get(key) ?? []) listener();
}

export function read<T>(key: string, fallback: T): T {
  return held.has(key) ? (held.get(key) as T) : fallback;
}

export function write(key: string, value: unknown): void {
  held.set(key, value);
  announce(key);
  void LocalStorage.setItem(`${PREFIX}${key}`, value).catch(() => {});
}

/** Subscribes to one key, and returns the unsubscribe. */
export function watch(key: string, listener: () => void): () => void {
  const set = listeners.get(key) ?? new Set();
  set.add(listener);
  listeners.set(key, set);

  return () => {
    set.delete(listener);
    if (set.size === 0) listeners.delete(key);
  };
}
