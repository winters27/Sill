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

export interface ToastOptions {
  title: string;
  message?: string;
  style?: string;
  primaryAction?: unknown;
  secondaryAction?: unknown;
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

  constructor(options: ToastOptions) {
    this.id = `toast-${++toastSeq}`;
    this._title = options.title;
    this._message = options.message;
    this._style = options.style ?? Toast.Style.Success;
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
    });
  }

  async hide(): Promise<void> {
    await getBridge().request("UI/hideToast", { id: this.id });
  }

  private push(): void {
    getBridge().emit("UI/updateToast", {
      id: this.id,
      title: this._title,
      message: this._message ?? "",
      style: this._style,
    });
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
