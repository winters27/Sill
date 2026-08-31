/**
 * Dictation, mirrored from `src-tauri/src/dictation/`.
 *
 * Rust is the authority: it fills in any field this side omits, so a stale
 * copy here loses a setting rather than corrupting one.
 */
import { invoke } from "@tauri-apps/api/core";

/** Where a finished transcript goes. */
export type OutputMode = "paste" | "clipboard" | "none";

export interface TranscriptionProvider {
  enabled: boolean;
  name: string | null;
  providerType: string | null;
  apiKey: string | null;
  baseUrl: string | null;
  lastModelId: string | null;
}

export interface DictationSettings {
  enabled: boolean;
  /** The trigger, as the recorder produces it: "Control+Alt" and "D". */
  shortcutModifier: string;
  shortcutKey: string;
  /** Which backend transcribes: "local", "openai", "groq" and so on. */
  providerId: string;
  /** Which whisper model the local server runs. */
  modelId: string;
  provider: TranscriptionProvider;
  /** cpal device id. null follows the system default microphone. */
  deviceId: string | null;
  /** ISO language code. null lets the model detect it. */
  language: string | null;
  outputMode: OutputMode;
  soundEnabled: boolean;
  /** Names and jargon the model should get right, sent as the prompt. */
  vocabulary: string;
  /** Follow whatever Windows Sound settings call the default input. */
  useSystemMicrophone: boolean;
  /** Preferred devices, best first, used when the above is off. */
  devicePriority: string[];
  muteWhileRecording: boolean;
  finishKey: string;
  cancelKey: string;
  confirmCancel: boolean;
  /** Standing guidance sent at the head of every transcription prompt. */
  customInstructions: string;
  appContext: boolean;
  keepHistory: boolean;
  /** Days a transcript is kept. Zero keeps everything, which is the default. */
  retainDays: number;
}

/** One finished dictation. */
export interface HistoryEntry {
  /** Unix seconds. */
  at: number;
  text: string;
  words: number;
  spokenMs: number;
  transcribeMs: number;
  provider: string;
  model: string;
  app: string | null;
}

export type StatsRange = "today" | "week" | "month" | "allTime";

export interface DictationStats {
  dictations: number;
  totalWords: number;
  spokenSeconds: number;
  wordsPerMinute: number;
  /** Against a 40 words per minute typing baseline. */
  secondsSaved: number;
}

export interface AudioInputDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface WhisperModel {
  id: string;
  label: string;
  sizeBytes: number;
  installed: boolean;
}

/** What a running server is doing right now. */
export interface ServerSnapshot {
  port: number;
  modelId: string;
  pid: number;
  uptimeSeconds: number;
  idleSeconds: number;
  idleTimeoutSeconds: number;
  /** Working set, which is what Task Manager's Memory column shows. */
  memoryBytes: number;
}

export interface LocalSetupStatus {
  engineInstalled: boolean;
  modelInstalled: boolean;
  modelId: string;
  /** Bytes still to fetch, so the button can say what it will cost. */
  downloadBytes: number;
  serverRunning: boolean;
  /** Live details while it is running, otherwise null. */
  server: ServerSnapshot | null;
  engineVersion: string;
  modelLabel: string;
  /** Roughly what the selected model holds once resident. */
  modelMemoryBytes: number;
}

/**
 * A stage of the local setup, as `dictation:setup` delivers it.
 *
 * Serialised by serde as an externally tagged enum, so a payload is either a
 * bare string for the unit stages or a one-key object for the rest.
 */
export type SetupProgress =
  | "engine"
  | "verifying"
  | "starting"
  | "ready"
  | { engineDownload: { bytesDownloaded: number; totalBytes: number } }
  | { model: { bytesDownloaded: number; totalBytes: number } }
  | { failed: { error: string } };

export function listAudioInputDevices(): Promise<AudioInputDevice[]> {
  return invoke<AudioInputDevice[]>("list_audio_input_devices");
}

export function listWhisperModels(): Promise<WhisperModel[]> {
  return invoke<WhisperModel[]>("list_whisper_models");
}

export function getLocalDictationStatus(): Promise<LocalSetupStatus> {
  return invoke<LocalSetupStatus>("get_local_dictation_status");
}

/** Downloads the engine and the model, then starts the server. */
export function installLocalDictation(modelId: string): Promise<void> {
  return invoke("install_local_dictation", { modelId });
}

export function removeWhisperModel(modelId: string): Promise<boolean> {
  return invoke<boolean>("remove_whisper_model", { modelId });
}

/** Stops the local server, releasing the model's memory. */
export function stopWhisperServer(): Promise<void> {
  return invoke("stop_whisper_server");
}

export function dictationHistory(): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>("dictation_history");
}

export function dictationStats(range: StatsRange): Promise<DictationStats> {
  return invoke<DictationStats>("dictation_stats", { range });
}

export function lastTranscription(): Promise<HistoryEntry | null> {
  return invoke<HistoryEntry | null>("last_transcription");
}

export function forgetTranscription(at: number): Promise<boolean> {
  return invoke<boolean>("forget_transcription", { at });
}

export function clearDictationHistory(): Promise<number> {
  return invoke<number>("clear_dictation_history");
}

/** What the keyboard hook currently believes. */
export interface HookState {
  /** The hook is installed, not merely configured. */
  armed: boolean;
  listening: boolean;
  triggerHeld: boolean;
  recording: boolean;
  /** Key events the hook has been handed. Zero means it sees no input. */
  keysSeen: number;
  /** Key events that arrived synthesised rather than typed. Acted on. */
  injectedSeen: number;
  /** Key events ignored for being Sill's own. */
  ownSeen: number;
  /** Presses of the trigger key, whatever else was held. */
  chordKeySeen: number;
  /** Times the trigger was seen with the right modifiers held. */
  triggersSeen: number;
  /** Modifiers held at the last trigger-key press, e.g. "Alt". */
  lastModifiers: string | null;
}

export function dictationHookState(): Promise<HookState> {
  return invoke<HookState>("dictation_hook_state");
}

/** Puts the hook back to idle, for when it has got stuck. */
export function resetDictationHook(): Promise<void> {
  return invoke("reset_dictation_hook");
}

export function startDictation(): Promise<void> {
  return invoke("start_dictation");
}

export function confirmDictation(): Promise<void> {
  return invoke("confirm_dictation");
}

export function cancelDictation(): Promise<void> {
  return invoke("cancel_dictation");
}

/**
 * The panel's current status.
 *
 * The panel window is declared hidden, so its webview can miss the very first
 * status event. Asking on mount is what stops the first dictation after
 * launch rendering an empty pill.
 */
export function getDictationPanelStatus(): Promise<string | null> {
  return invoke<string | null>("get_dictation_panel_status");
}

/**
 * Whisper's language list, trimmed to the ones worth offering.
 *
 * `null` is auto-detect, which is right for anyone who dictates in one
 * language and never thinks about it. Pinning one is faster and stops a
 * strong accent being detected as a neighbouring language.
 */
export const LANGUAGES: { code: string | null; name: string }[] = [
  { code: null, name: "Auto" },
  { code: "en", name: "English" },
  { code: "es", name: "Spanish" },
  { code: "fr", name: "French" },
  { code: "de", name: "German" },
  { code: "it", name: "Italian" },
  { code: "pt", name: "Portuguese" },
  { code: "nl", name: "Dutch" },
  { code: "pl", name: "Polish" },
  { code: "ru", name: "Russian" },
  { code: "uk", name: "Ukrainian" },
  { code: "tr", name: "Turkish" },
  { code: "ar", name: "Arabic" },
  { code: "hi", name: "Hindi" },
  { code: "zh", name: "Chinese" },
  { code: "ja", name: "Japanese" },
  { code: "ko", name: "Korean" },
];

/** "12h 17m", "4m", "38s". The shape a saved-time figure wants. */
export function formatDuration(seconds: number): { value: string; unit: string }[] {
  if (seconds < 60) return [{ value: String(Math.round(seconds)), unit: "s" }];

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return [{ value: String(minutes), unit: "m" }];

  return [
    { value: String(Math.floor(minutes / 60)), unit: "h" },
    { value: String(minutes % 60), unit: "m" },
  ];
}

/** "118k", "1.2M". Big counts, without the reader counting digits. */
export function formatCount(value: number): { value: string; unit: string } {
  if (value >= 1_000_000) return { value: (value / 1_000_000).toFixed(1), unit: "M" };
  if (value >= 1_000) return { value: String(Math.floor(value / 1_000)), unit: "k" };
  return { value: String(value), unit: "" };
}

/** Human-readable download size, for a button that is about to cost minutes. */
export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 MB";
  const mb = bytes / (1024 * 1024);
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}
