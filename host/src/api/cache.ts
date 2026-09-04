/**
 * Raycast's `Cache`, which is synchronous, over storage that is not.
 *
 * ## Why this is not just `LocalStorage`
 *
 * `Cache` is a class an extension constructs and then reads from **inside a
 * render**: `cache.get(key)` returns a string or `undefined`, right now, with
 * no promise. Raycast can do that because its cache is a file read on the same
 * thread. Sill's storage lives in Rust behind an RPC, and nothing can await
 * inside a render.
 *
 * So the same bargain `utils/cache.ts` already makes for `useCachedState`: a
 * map in the worker, filled in from storage shortly after the extension
 * starts, written through synchronously and persisted in the background.
 *
 * **What that costs, said plainly:** the first read after launch can miss a
 * value that is on disk, and it arrives a moment later. For what a cache is
 * for, a stale miss is a re-fetch. It is not correct to use this for anything
 * where a miss is worse than a wait, and `LocalStorage` is the awaitable one
 * for that.
 *
 * ## Why it was worth building
 *
 * Measured across the twelve most-installed extensions, 124 commands: `Cache`
 * was the second most-wanted thing the host did not answer, and it stopped
 * **13 commands** dead at import. `obsidian` is twelve of them.
 *
 * ## Namespaces
 *
 * Raycast scopes a cache by `namespace`, and two caches with different ones do
 * not see each other. Keys are prefixed rather than kept in separate maps, so
 * `clear()` on one namespace cannot reach another's keys and there is one code
 * path for storage.
 */
import { LocalStorage } from "./runtime";

/** Keeps cache entries out of the namespace an extension's own keys live in. */
const PREFIX = "sill.Cache:";


/** Every entry, by its fully qualified key. Shared across instances. */
const held = new Map<string, string>();

/** Subscribers, by namespace, so one cache does not notify another's. */
const listeners = new Map<string, Set<(key: string, value?: string) => void>>();

let hydrating: Promise<void> | undefined;

/**
 * Reads what is already stored, once, in the background.
 *
 * Started when the first `Cache` is constructed rather than at launch, so an
 * extension that never uses one pays nothing. That is the same rule
 * `utils/cache.ts` follows.
 */
function hydrate(): void {
  hydrating ??= LocalStorage.allItems<Record<string, unknown>>()
    .then((items) => {
      for (const [key, value] of Object.entries(items ?? {})) {
        if (!key.startsWith(PREFIX)) continue;

        const bare = key.slice(PREFIX.length);

        // Anything set while this was in flight is newer than what was on disk
        // when it started, so it wins. Hydration must never walk over a value
        // somebody has already written.
        if (held.has(bare)) continue;
        if (typeof value === "string") held.set(bare, value);
      }
    })
    .catch(() => {
      // Storage being unreachable is an empty cache, which is a state every
      // caller already handles. It is not worth failing an extension over.
    });
}

function announce(namespace: string, key: string, value?: string): void {
  for (const listener of listeners.get(namespace) ?? []) {
    try {
      listener(key, value);
    } catch {
      // A subscriber that throws is its own problem and must not stop the
      // others being told, nor the write that caused it.
    }
  }
}

export interface CacheOptions {
  /** Scopes this cache. Two with different namespaces cannot see each other. */
  namespace?: string;
  /**
   * Accepted and not enforced.
   *
   * Raycast evicts by least-recent use past a byte budget. Sill's storage is a
   * database rather than a file it rewrites, so there is no size to defend
   * against here, and silently dropping entries an extension believes it wrote
   * would be a worse failure than holding a few more of them.
   */
  capacity?: number;
}

export class Cache {
  readonly namespace: string;
  readonly capacity: number;

  constructor(options: CacheOptions = {}) {
    this.namespace = options.namespace ?? "";
    this.capacity = options.capacity ?? 0;
    hydrate();
  }

  /**
   * Where every key of this namespace starts.
   *
   * Length-prefixed rather than separated by a character. Any printable
   * separator can appear inside a namespace, so `{namespace: "a", key:
   * "b:c"}` and `{namespace: "a:b", key: "c"}` would collide; the length says
   * where the namespace ends instead of hoping nothing looks like the marker.
   *
   * The obvious answer is a NUL, which cannot appear in either. It is also
   * the one thing `verify:source` refuses in a source file, and rightly: a
   * literal NUL compiles, passes its tests, and the only outward sign is grep
   * quietly deciding the file is binary. It caught exactly that here.
   */
  private get prefix(): string {
    return `${this.namespace.length}:${this.namespace}`;
  }

  /** The fully qualified key, so namespaces cannot collide. */
  private at(key: string): string {
    return `${this.prefix}${key}`;
  }

  get(key: string): string | undefined {
    return held.get(this.at(key));
  }

  has(key: string): boolean {
    return held.has(this.at(key));
  }

  get isEmpty(): boolean {
    const prefix = this.prefix;
    for (const key of held.keys()) if (key.startsWith(prefix)) return false;
    return true;
  }

  set(key: string, value: string): void {
    const at = this.at(key);
    held.set(at, value);
    announce(this.namespace, key, value);
    void LocalStorage.setItem(`${PREFIX}${at}`, value).catch(() => {});
  }

  remove(key: string): boolean {
    const at = this.at(key);
    const had = held.delete(at);

    if (had) {
      announce(this.namespace, key, undefined);
      void LocalStorage.removeItem(`${PREFIX}${at}`).catch(() => {});
    }

    return had;
  }

  /**
   * Empties this namespace, and only this one.
   *
   * `LocalStorage.clear()` would take the extension's own keys with it, which
   * is a different and much larger thing than clearing a cache.
   */
  clear(options: { notifySubscribers?: boolean } = {}): void {
    const prefix = this.prefix;

    for (const at of [...held.keys()]) {
      if (!at.startsWith(prefix)) continue;

      held.delete(at);
      void LocalStorage.removeItem(`${PREFIX}${at}`).catch(() => {});

      if (options.notifySubscribers !== false) {
        announce(this.namespace, at.slice(prefix.length), undefined);
      }
    }
  }

  /**
   * Subscribes to changes in this namespace. Returns the unsubscribe.
   *
   * A property holding an arrow, not a method, and that is the whole reason
   * this is written oddly. `@raycast/utils` does
   * `useSyncExternalStore(cache.subscribe, ...)`, which hands React the
   * function without the object it came from, so React calls it with no `this`
   * and a method reads `this.namespace` of undefined. Every extension using
   * `useCachedState` died on its first render because of it, and the error
   * arrived from inside React with nothing in the stack naming Sill.
   */
  subscribe = (subscriber: (key: string, value?: string) => void): (() => void) => {
    const set = listeners.get(this.namespace) ?? new Set();
    set.add(subscriber);
    listeners.set(this.namespace, set);

    return () => {
      set.delete(subscriber);
      if (set.size === 0) listeners.delete(this.namespace);
    };
  };
}
