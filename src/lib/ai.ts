/**
 * Asking a model, mirrored from `src-tauri/src/ai/`.
 *
 * Rust is the authority: it decides who answers, where a key may be sent and
 * what a request looks like. This side only shows and collects.
 */
import { invoke } from "@tauri-apps/api/core";
import { silently } from "$lib/status";

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

/** What Windows Hello can do on this machine. */
export interface AiHelloHere {
  /** Whether a face, a fingerprint or a Hello PIN can be asked for. */
  ready: boolean;
  /** Why not, when not. Absent when it can. */
  why?: string;
}

/**
 * Asks Windows whether the Hello gate can actually run here.
 *
 * For the settings row, so a switch shown as on can say when the thing it
 * switches on is not available on this machine. Answering "cannot" when the
 * call itself fails is the honest direction: the row would rather understate
 * what is protecting somebody than overstate it.
 */
export function aiHello(): Promise<AiHelloHere> {
  return invoke<AiHelloHere>("ai_hello").catch(
    silently({
      ready: false,
      why: "Windows could not be asked about Windows Hello",
    }),
  );
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

/**
 * What each of these models is called, in order.
 *
 * One call for the whole list. Working it out in the window instead would put
 * the rule for what a model is called in two places, and the chip in the
 * launcher and the cards in here would drift apart the first time either
 * changed.
 */
export function aiNamed(providers: AiProvider[]): Promise<string[]> {
  return invoke<string[]>("ai_named", { providers });
}
