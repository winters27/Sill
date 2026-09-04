/**
 * The imperative half of the Raycast API: everything that is a function call
 * rather than a component. These all reach the Rust host over the bridge.
 */

import { getBridge } from "./bridge";

let toastSeq = 0;

export const Toast = {
  Style: {
    Success: "success",
    Failure: "failure",
    Animated: "animated",
  } as const,
};

/** A button on a toast: what it says, what runs it, and its chord. */
export interface ToastAction {
  title?: string;
  shortcut?: unknown;
  onAction?: (toast: ToastHandle) => void;
}

export interface ToastOptions {
  title: string;
  message?: string;
  style?: string;
  primaryAction?: ToastAction;
  secondaryAction?: ToastAction;
}

/** One button as the window receives it: a title, a chord, and a handler id. */
interface OnTheWire {
  title: string;
  shortcut?: unknown;
  handler: string;
}

/**
 * Raycast hands back a live handle whose properties can be reassigned to
 * update the toast in place, so this is a class rather than a plain object.
 */
export class ToastHandle {
  readonly id: string;
  private _title: string;
  private _message: string | undefined;
  private _style: string;
  private readonly _primary: ToastAction | undefined;
  private readonly _secondary: ToastAction | undefined;

  /**
   * The handler ids the window is currently holding for this toast's buttons.
   *
   * Kept so they can be given back. These are registered in the same registry
   * every other callback uses, which is what makes a toast button an ordinary
   * activation rather than a channel of its own, but nothing in a tree owns
   * them, so the reconciler's deferred removal never reaches them. Releasing
   * them here is the only thing that does.
   */
  private minted: string[] = [];

  constructor(options: ToastOptions) {
    this.id = `toast-${++toastSeq}`;
    this._title = options.title;
    this._message = options.message;
    this._style = options.style ?? Toast.Style.Success;
    this._primary = options.primaryAction;
    this._secondary = options.secondaryAction;
  }

  get title(): string {
    return this._title;
  }
  set title(value: string) {
    this._title = value;
    this.push();
  }

  get message(): string | undefined {
    return this._message;
  }
  set message(value: string | undefined) {
    this._message = value;
    this.push();
  }

  get style(): string {
    return this._style;
  }
  set style(value: string) {
    this._style = value;
    this.push();
  }

  async show(): Promise<void> {
    await getBridge().request("UI/showToast", {
      id: this.id,
      title: this._title,
      message: this._message ?? "",
      style: this._style,
      actions: this.buttons(),
    });
  }

  async hide(): Promise<void> {
    this.releaseButtons();
    await getBridge().request("UI/hideToast", { id: this.id });
  }

  private push(): void {
    getBridge().emit("UI/updateToast", {
      id: this.id,
      title: this._title,
      message: this._message ?? "",
      style: this._style,
      actions: this.buttons(),
    });
  }

  /**
   * The buttons, as ids the window can activate.
   *
   * Minted afresh on every send and the previous set given back first, because
   * an update is the same toast redrawn: the window is about to forget the ids
   * it was holding, and keeping them alive so it could not use them is a leak
   * with nothing on the other end of it.
   *
   * Raycast calls the handler with the toast itself, which is what lets a Retry
   * button set `toast.style = Failure` and change the message it is sitting on.
   * That is done here rather than by the window, which has no toast object and
   * should not learn about one.
   */
  private buttons(): OnTheWire[] {
    this.releaseButtons();

    const out: OnTheWire[] = [];

    for (const [action, fallback] of [
      [this._primary, "Retry"],
      [this._secondary, "Cancel"],
    ] as const) {
      if (!action?.onAction) continue;

      const handler = getBridge().renderer.callbacks.register(() => {
        action.onAction?.(this);
        return null;
      });

      this.minted.push(handler);
      out.push({ title: action.title ?? fallback, shortcut: action.shortcut, handler });
    }

    return out;
  }

  private releaseButtons(): void {
    for (const handler of this.minted) getBridge().renderer.callbacks.release(handler);
    this.minted = [];
  }
}

/**
 * Raycast kept a positional overload from its older API and real extensions
 * still use it: `showToast(Toast.Style.Failure, "title", "message")` appears
 * alongside the object form in the same file. Both have to work.
 */
export async function showToast(
  options: ToastOptions | string,
  title?: string,
  message?: string,
): Promise<ToastHandle> {
  let opts: ToastOptions;

  if (typeof options === "string" && title !== undefined) {
    // Positional form: the first argument is the style.
    opts = { style: options, title, message };
  } else if (typeof options === "string") {
    opts = { title: options };
  } else {
    opts = options;
  }

  const toast = new ToastHandle(opts);
  await toast.show();
  return toast;
}

export interface HudOptions {
  clearRootSearch?: boolean;
  popToRootType?: string;
}

export async function showHUD(text: string, options?: HudOptions): Promise<void> {
  await getBridge().request("UI/showHud", {
    text,
    clearRootSearch: options?.clearRootSearch ?? false,
    popToRootType: options?.popToRootType ?? "default",
  });
}

export async function popToRoot(options?: { clearSearchBar?: boolean }): Promise<void> {
  await getBridge().request("UI/popToRoot", {
    clearSearchBar: options?.clearSearchBar ?? true,
  });
}

export async function closeMainWindow(options?: { clearRootSearch?: boolean }): Promise<void> {
  await getBridge().request("UI/closeMainWindow", {
    clearRootSearch: options?.clearRootSearch ?? false,
  });
}

export async function open(target: string, application?: string): Promise<void> {
  await getBridge().request("Application/open", { target, appId: application });
}

export async function getSelectedText(): Promise<string> {
  return getBridge().request<string>("UI/getSelectedText");
}

export async function getApplications(target?: string): Promise<unknown[]> {
  return getBridge().request<unknown[]>("Application/list", { target });
}

export async function getDefaultApplication(target: string): Promise<unknown> {
  return getBridge().request("Application/getDefault", { target });
}

export function getPreferenceValues<T = Record<string, unknown>>(): T {
  return getBridge().preferences as T;
}

export const environment = new Proxy(
  {},
  {
    // Read lazily: the bridge is installed after this module is evaluated.
    get: (_target, prop: string) => getBridge().environment[prop as never],
  },
) as Record<string, unknown>;

export const Clipboard = {
  async copy(content: unknown, options?: { concealed?: boolean }): Promise<void> {
    await getBridge().request("Clipboard/copy", {
      content: normalizeClipboard(content),
      options: { concealed: options?.concealed ?? false },
    });
  },
  async paste(content: unknown): Promise<void> {
    await getBridge().request("Clipboard/paste", { content: normalizeClipboard(content) });
  },
  async clear(): Promise<void> {
    await getBridge().request("Clipboard/clear");
  },
  async read(): Promise<{ text?: string; html?: string }> {
    return getBridge().request("Clipboard/readContent");
  },
  async readText(): Promise<string | undefined> {
    const content = await getBridge().request<{ text?: string }>("Clipboard/readContent");
    return content.text;
  },
};

function normalizeClipboard(content: unknown): Record<string, unknown> {
  if (typeof content === "string") return { text: content };
  if (typeof content === "number") return { text: String(content) };
  return (content ?? {}) as Record<string, unknown>;
}

export const LocalStorage = {
  async getItem<T = unknown>(key: string): Promise<T | undefined> {
    return getBridge().request<T | undefined>("Storage/get", { key });
  },
  async setItem(key: string, value: unknown): Promise<void> {
    await getBridge().request("Storage/set", { key, value });
  },
  async removeItem(key: string): Promise<void> {
    await getBridge().request("Storage/remove", { key });
  },
  async clear(): Promise<void> {
    await getBridge().request("Storage/clear");
  },
  async allItems<T = Record<string, unknown>>(): Promise<T> {
    return getBridge().request<T>("Storage/list");
  },
};
