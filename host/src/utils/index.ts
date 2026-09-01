/**
 * `@raycast/utils`, as far as it can honestly go on Windows.
 *
 * The audit's reason for building this: `usePromise`, `useCachedPromise` and
 * `useFetch` are near universal in the Raycast store, and resolving the whole
 * package to a thrown error excluded every extension touching one of them.
 * That was a large slice of the catalogue closed off by a single line.
 *
 * ## What is missing throws by name
 *
 * Anything this cannot do throws saying which function and why, rather than
 * being absent. A missing export otherwise reads as `undefined is not a
 * function` from inside a bundle, which looks like the extension being broken
 * rather than like Sill not covering something.
 *
 * ## The permission gate does its own work here
 *
 * `runPowerShellScript` requires `child_process` **inside the function**, not
 * at the top of this file. The require is what the module gate refuses, so an
 * extension without permission to start programs gets the permission message
 * at the moment it tries, and every other extension importing this module is
 * unaffected.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { LocalStorage, showToast, Toast } from "../api";
import * as cache from "./cache";

void cache.hydrate();

export interface PromiseOptions<T> {
  execute?: boolean;
  keepPreviousData?: boolean;
  abortable?: { current?: AbortController | null };
  onError?: (error: Error) => void;
  onData?: (data: T) => void;
  onWillExecute?: (args: unknown[]) => void;
}

export interface MutateOptions<T> {
  optimisticUpdate?: (data: T | undefined) => T;
  rollbackOnError?: boolean | ((data: T | undefined) => T);
  shouldRevalidateAfter?: boolean;
}

export interface PromiseResult<T> {
  isLoading: boolean;
  data: T | undefined;
  error: Error | undefined;
  revalidate: () => void;
  mutate: (update?: Promise<unknown>, options?: MutateOptions<T>) => Promise<unknown>;
}

/** Stable enough to compare renders by, without demanding serialisable args. */
function keyOf(args: readonly unknown[]): string {
  try {
    return JSON.stringify(args) ?? "";
  } catch {
    return `unserialisable:${args.length}`;
  }
}

function asError(thrown: unknown): Error {
  return thrown instanceof Error ? thrown : new Error(String(thrown));
}

export function usePromise<T>(
  fn: (...args: never[]) => Promise<T>,
  args: readonly unknown[] = [],
  options: PromiseOptions<T> = {},
): PromiseResult<T> {
  const { execute = true } = options;

  const [data, setData] = useState<T | undefined>(undefined);
  const [error, setError] = useState<Error | undefined>(undefined);
  const [isLoading, setIsLoading] = useState<boolean>(execute);

  /*
   * Every run is numbered and only the newest may write.
   *
   * Two runs overlap whenever arguments change while one is in flight, and
   * without this the slower one lands last and the list shows results for a
   * query nobody is looking at any more. The same generation guard the
   * launcher's own search needed, for the same reason.
   */
  const generation = useRef(0);
  const alive = useRef(true);

  const latest = useRef({ fn, options });
  latest.current = { fn, options };

  useEffect(
    () => () => {
      alive.current = false;
    },
    [],
  );

  const run = useCallback(async (callArgs: readonly unknown[]): Promise<void> => {
    const id = ++generation.current;
    const { fn: current, options: opts } = latest.current;

    opts.onWillExecute?.([...callArgs]);

    if (opts.abortable) {
      opts.abortable.current?.abort();
      opts.abortable.current = new AbortController();
    }

    setIsLoading(true);
    if (!opts.keepPreviousData) setData(undefined);
    setError(undefined);

    try {
      const value = await (current as (...a: unknown[]) => Promise<T>)(...callArgs);

      if (!alive.current || id !== generation.current) return;

      setData(value);
      setIsLoading(false);
      opts.onData?.(value);
    } catch (thrown) {
      if (!alive.current || id !== generation.current) return;

      // An abort is this hook's own doing, not a failure to report.
      if (thrown instanceof Error && thrown.name === "AbortError") return;

      const failed = asError(thrown);
      setError(failed);
      setIsLoading(false);
      opts.onError?.(failed);
    }
  }, []);

  const argsKey = keyOf(args);

  useEffect(() => {
    if (!execute) {
      setIsLoading(false);
      return;
    }
    void run(args);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [argsKey, execute, run]);

  const revalidate = useCallback(() => {
    void run(args);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [argsKey, run]);

  const mutate = useCallback(
    async (update?: Promise<unknown>, mutateOptions: MutateOptions<T> = {}): Promise<unknown> => {
      const { optimisticUpdate, rollbackOnError = true, shouldRevalidateAfter = true } =
        mutateOptions;

      let before: T | undefined;

      if (optimisticUpdate) {
        setData((held) => {
          before = held;
          return optimisticUpdate(held);
        });
      }

      try {
        const result = await update;
        if (shouldRevalidateAfter) void run(args);
        return result;
      } catch (thrown) {
        if (optimisticUpdate && rollbackOnError !== false) {
          setData(typeof rollbackOnError === "function" ? rollbackOnError(before) : before);
        }
        throw thrown;
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [argsKey, run],
  );

  return { isLoading, data, error, revalidate, mutate };
}

export function useCachedState<T>(
  key: string,
  initialValue: T,
  config?: { cacheNamespace?: string },
): [T, (value: T | ((previous: T) => T)) => void] {
  const full = config?.cacheNamespace ? `${config.cacheNamespace}:${key}` : key;

  const [value, setLocal] = useState<T>(() => cache.read(full, initialValue));

  // Re-renders when hydration lands, or when another hook writes the same key.
  useEffect(
    () => cache.watch(full, () => setLocal(cache.read(full, initialValue))),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [full],
  );

  const set = useCallback(
    (next: T | ((previous: T) => T)) => {
      const resolved =
        typeof next === "function"
          ? (next as (previous: T) => T)(cache.read(full, initialValue))
          : next;

      cache.write(full, resolved);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [full],
  );

  return [value, set];
}

export function useCachedPromise<T>(
  fn: (...args: never[]) => Promise<T>,
  args: readonly unknown[] = [],
  options: PromiseOptions<T> & { initialData?: T } = {},
): PromiseResult<T> {
  const key = `promise:${fn.name || "anonymous"}:${keyOf(args)}`;
  const [cached, setCached] = useCachedState<T | undefined>(key, options.initialData);

  const result = usePromise(fn, args, {
    ...options,
    onData: (value) => {
      setCached(value);
      options.onData?.(value);
    },
  });

  // The cached value stands in until the live one lands, which is the whole
  // point of the hook: a list that was there a second ago beats a spinner.
  return { ...result, data: result.data ?? cached };
}

export interface FetchOptions<T> extends PromiseOptions<T> {
  method?: string;
  headers?: Record<string, string>;
  body?: unknown;
  initialData?: T;
  parseResponse?: (response: Response) => Promise<T>;
  mapResult?: (raw: unknown) => { data: T };
}

/** JSON unless told otherwise, which is what nearly every caller wants. */
async function defaultParse(response: Response): Promise<unknown> {
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`.trim());
  }

  const type = response.headers.get("content-type") ?? "";
  return type.includes("json") ? response.json() : response.text();
}

export function useFetch<T>(url: string, options: FetchOptions<T> = {}): PromiseResult<T> {
  const { method, headers, body, parseResponse, mapResult, ...rest } = options;
  const shape = keyOf([headers, body]);

  const fetcher = useCallback(
    async (): Promise<T> => {
      const response = await fetch(url, {
        method,
        headers,
        // Cast loosely: the worker has no DOM lib, so `BodyInit` does not
        // exist here even though the runtime accepts exactly those shapes.
        body: body === undefined ? undefined : (body as never),
        signal: rest.abortable?.current?.signal,
      });

      const parsed = parseResponse
        ? await parseResponse(response)
        : ((await defaultParse(response)) as T);

      return mapResult ? mapResult(parsed).data : (parsed as T);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [url, method, shape],
  );

  return useCachedPromise(fetcher as never, [url, method, shape], rest);
}

export function useLocalStorage<T>(
  key: string,
  initialValue?: T,
): {
  value: T | undefined;
  setValue: (value: T) => Promise<void>;
  removeValue: () => Promise<void>;
  isLoading: boolean;
} {
  const [value, setLocal] = useState<T | undefined>(initialValue);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    let alive = true;

    void LocalStorage.getItem<T>(key)
      .then((held) => {
        if (alive && held !== undefined) setLocal(held as T);
      })
      .finally(() => {
        if (alive) setIsLoading(false);
      });

    return () => {
      alive = false;
    };
  }, [key]);

  const setValue = useCallback(
    async (next: T) => {
      setLocal(next);
      await LocalStorage.setItem(key, next);
    },
    [key],
  );

  const removeValue = useCallback(async () => {
    setLocal(undefined);
    await LocalStorage.removeItem(key);
  }, [key]);

  return { value, setValue, removeValue, isLoading };
}

export type Validator<V> = ((value: V | undefined) => string | undefined | void) | "required";

export const FormValidation = { Required: "required" as const };

export function useForm<T extends Record<string, unknown>>(props: {
  onSubmit: (values: T) => void | boolean | Promise<void | boolean>;
  initialValues?: Partial<T>;
  validation?: { [K in keyof T]?: Validator<T[K]> };
}) {
  const { onSubmit, initialValues = {}, validation = {} } = props;

  const [values, setValues] = useState<T>({ ...initialValues } as T);
  const [errors, setErrors] = useState<Partial<Record<keyof T, string>>>({});

  const checkOne = useCallback(
    (field: keyof T, value: unknown): string | undefined => {
      const rule = (validation as Record<string, unknown>)[field as string];
      if (!rule) return undefined;

      if (rule === "required") {
        const empty =
          value === undefined ||
          value === null ||
          value === "" ||
          (Array.isArray(value) && value.length === 0);

        return empty ? "The item is required" : undefined;
      }

      return (rule as (v: unknown) => string | undefined | void)(value) ?? undefined;
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  const setValue = useCallback(
    <K extends keyof T>(field: K, value: T[K]) => {
      setValues((held) => ({ ...held, [field]: value }));
      setErrors((held) => ({ ...held, [field]: checkOne(field, value) }));
    },
    [checkOne],
  );

  const handleSubmit = useCallback(
    (submitted: T) => {
      const found: Partial<Record<keyof T, string>> = {};

      for (const field of Object.keys(validation) as (keyof T)[]) {
        const failed = checkOne(field, submitted?.[field]);
        if (failed) found[field] = failed;
      }

      setErrors(found);
      if (Object.keys(found).length > 0) return;

      void onSubmit(submitted);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [checkOne, onSubmit],
  );

  const itemProps = new Proxy({} as Record<string, unknown>, {
    get: (_t, field: string) => ({
      id: field,
      value: values?.[field as keyof T],
      error: errors[field as keyof T],
      onChange: (value: unknown) => setValue(field as keyof T, value as T[keyof T]),
      onBlur: () =>
        setErrors((held) => ({
          ...held,
          [field]: checkOne(field as keyof T, values?.[field as keyof T]),
        })),
    }),
  });

  return {
    handleSubmit,
    itemProps,
    values,
    setValue,
    setValidationError: (field: keyof T, error: string | undefined) =>
      setErrors((held) => ({ ...held, [field]: error })),
    reset: (next?: Partial<T>) => {
      setValues({ ...initialValues, ...next } as T);
      setErrors({});
    },
    // Focus belongs to the window and the host cannot reach a rendered field,
    // so this does nothing rather than throwing. An extension calling it still
    // works; it simply does not move the cursor.
    focus: () => {},
  };
}

export function useFrecencySorting<T extends { id?: string }>(
  data: T[] = [],
  options?: {
    key?: (item: T) => string;
    namespace?: string;
    sortUnvisited?: (a: T, b: T) => number;
  },
): { data: T[]; visitItem: (item: T) => Promise<void>; resetRanking: (item: T) => Promise<void> } {
  const keyOfItem = options?.key ?? ((item: T) => String(item.id ?? ""));
  const [scores, setScores] = useCachedState<Record<string, number>>(
    options?.namespace ?? "frecency",
    {},
  );

  const sorted = [...data].sort((a, b) => {
    const left = scores[keyOfItem(a)] ?? 0;
    const right = scores[keyOfItem(b)] ?? 0;

    if (left !== right) return right - left;
    return options?.sortUnvisited?.(a, b) ?? 0;
  });

  return {
    data: sorted,
    visitItem: async (item) => {
      const id = keyOfItem(item);
      setScores({ ...scores, [id]: (scores[id] ?? 0) + 1 });
    },
    resetRanking: async (item) => {
      const next = { ...scores };
      delete next[keyOfItem(item)];
      setScores(next);
    },
  };
}

export async function showFailureToast(
  error: unknown,
  options?: { title?: string },
): Promise<unknown> {
  return showToast({
    style: Toast.Style.Failure,
    title: options?.title ?? "Something went wrong",
    message: error instanceof Error ? error.message : String(error),
  });
}

function asDataUri(svg: string): { source: string } {
  return { source: `data:image/svg+xml;base64,${Buffer.from(svg, "utf8").toString("base64")}` };
}

/** A coloured circle with initials, self-contained rather than fetched. */
export function getAvatarIcon(
  name: string,
  options?: { background?: string },
): { source: string } {
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => word[0]?.toUpperCase() ?? "")
    .join("");

  // Derived from the name, so one person keeps one colour instead of getting a
  // new one on every render.
  let hash = 0;
  for (const character of name) hash = (hash * 31 + character.charCodeAt(0)) | 0;

  const background = options?.background ?? `hsl(${Math.abs(hash) % 360} 55% 45%)`;

  return asDataUri(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">` +
      `<circle cx="50" cy="50" r="50" fill="${background}"/>` +
      `<text x="50" y="50" font-family="sans-serif" font-size="40" fill="#fff" ` +
      `text-anchor="middle" dominant-baseline="central">${initials}</text></svg>`,
  );
}

/** A ring filled to `progress`, between 0 and 1. */
export function getProgressIcon(
  progress: number,
  color = "#0ea5e9",
  options?: { background?: string; backgroundOpacity?: number },
): { source: string } {
  const clamped = Math.max(0, Math.min(1, progress));
  const circumference = 2 * Math.PI * 40;

  return asDataUri(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">` +
      `<circle cx="50" cy="50" r="40" fill="none" stroke="${options?.background ?? color}" ` +
      `stroke-opacity="${options?.backgroundOpacity ?? 0.2}" stroke-width="12"/>` +
      `<circle cx="50" cy="50" r="40" fill="none" stroke="${color}" stroke-width="12" ` +
      `stroke-dasharray="${circumference}" stroke-dashoffset="${circumference * (1 - clamped)}" ` +
      `transform="rotate(-90 50 50)"/></svg>`,
  );
}

export function getFavicon(
  url: string | URL,
  options?: { fallback?: unknown; size?: number },
): unknown {
  try {
    const host = new URL(String(url)).hostname;
    return { source: `https://www.google.com/s2/favicons?sz=${options?.size ?? 64}&domain=${host}` };
  } catch {
    return options?.fallback ?? { source: "Globe" };
  }
}

export function createDeeplink(options: {
  command?: string;
  extensionName?: string;
  ownerOrAuthorName?: string;
  arguments?: Record<string, unknown>;
}): string {
  const query = options.arguments
    ? `?arguments=${encodeURIComponent(JSON.stringify(options.arguments))}`
    : "";

  return (
    `raycast://extensions/${options.ownerOrAuthorName ?? ""}/` +
    `${options.extensionName ?? ""}/${options.command ?? ""}${query}`
  );
}

/** Remembers what a function returned, for as long as the extension runs. */
export function withCache<A extends unknown[], R>(
  fn: (...args: A) => Promise<R>,
  options?: { maxAge?: number },
): ((...args: A) => Promise<R>) & { clearCache: () => void } {
  const held = new Map<string, { at: number; value: Promise<R> }>();
  const maxAge = options?.maxAge ?? Infinity;

  const wrapped = (...args: A): Promise<R> => {
    const key = keyOf(args);
    const found = held.get(key);

    if (found && Date.now() - found.at < maxAge) return found.value;

    const value = fn(...args);
    held.set(key, { at: Date.now(), value });

    // A rejection must not be remembered, or one failed call poisons every
    // later one for the life of the extension.
    void value.catch(() => held.delete(key));

    return value;
  };

  wrapped.clearCache = () => held.clear();
  return wrapped;
}

/**
 * Runs a PowerShell script.
 *
 * `child_process` is required here rather than at the top of the file, so the
 * module gate refuses it at the moment it is used by an extension that was not
 * granted permission to start programs. Every other extension importing this
 * module is unaffected by that refusal.
 */
export async function runPowerShellScript(
  script: string,
  options?: { timeout?: number },
): Promise<string> {
  const { execFile } = require("child_process") as typeof import("child_process");

  return new Promise((resolve, reject) => {
    execFile(
      "powershell.exe",
      ["-NoProfile", "-NonInteractive", "-Command", script],
      { timeout: options?.timeout ?? 10000 },
      (error, stdout, stderr) => {
        if (error) reject(new Error(String(stderr ?? "").trim() || error.message));
        else resolve(String(stdout));
      },
    );
  });
}

function notHere(name: string, why: string): never {
  throw new Error(`sill: "${name}" is not available. ${why}`);
}

export function runAppleScript(): never {
  notHere(
    "runAppleScript",
    "AppleScript is macOS only. An extension built around it cannot work here without a different path for whatever it was doing.",
  );
}

export function useExec(): never {
  notHere(
    "useExec",
    "Running arbitrary commands is not offered to extensions. runPowerShellScript is, and needs permission to start programs.",
  );
}

export function useSQL(): never {
  notHere("useSQL", "Sill does not open SQLite databases on an extension's behalf.");
}

export function executeSQL(): never {
  notHere("executeSQL", "Sill does not open SQLite databases on an extension's behalf.");
}

export function useStreamJSON(): never {
  notHere("useStreamJSON", "Streaming a file from disk is not implemented here yet.");
}

export function useAI(): never {
  notHere("useAI", "Sill's AI is reached through its own commands, not from inside an extension.");
}
