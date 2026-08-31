/**
 * Asking a model, mirrored from `src-tauri/src/ai/`.
 *
 * Rust is the authority: it decides who answers, where a key may be sent and
 * what a request looks like. This side only shows and collects.
 */
import { invoke } from "@tauri-apps/api/core";

/** The shape a service expects, or the CLI that is not a shape at all. */
export type AiWire = "openAi" | "anthropic" | "claudeCode";

/** One service, as configured. */
export interface AiProvider {
  /** Stable across renames, because the chosen one is stored by it. */
  id: string;
  name: string;
  wire: AiWire;
  /** Empty for the one that runs a binary rather than making a request. */
  baseUrl: string;
  /** Sealed before it is written to disk. Empty for anything needing no key. */
  apiKey: string;
  model: string;
  /** Only on the ones offered in Add: one line about setting it up. */
  note?: string;
}

/** One model somebody can choose. */
export interface AiModel {
  /** What goes into the request. */
  id: string;
  /** What the settings window shows. */
  label: string;
}

/** The services Sill knows how to reach, ready to be added. */
export function aiKnown(): Promise<AiProvider[]> {
  return invoke<AiProvider[]>("ai_known");
}

/**
 * Which models a provider offers.
 *
 * An empty list is not a failure: the settings window offers a text field
 * instead of a picker, which still works.
 */
export function aiModels(provider: AiProvider): Promise<AiModel[]> {
  return invoke<AiModel[]>("ai_models", { provider });
}
