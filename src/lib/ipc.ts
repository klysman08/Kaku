// Typed wrappers around the Rust commands (src-tauri/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";

export type ProviderKind = "gemini" | "openai";

export interface Shortcuts {
  popup: string;
  actions: Record<string, string>;
}

export interface ModelPrice {
  input: number;
  output: number;
}

export interface Settings {
  provider: ProviderKind;
  geminiModel: string;
  openaiBaseUrl: string;
  openaiModel: string;
  targetLanguage: string;
  uiLanguage: "auto" | "pt-BR" | "en";
  theme: "system" | "dark" | "light";
  sounds: boolean;
  minimizeToTray: boolean;
  historyEnabled: boolean;
  historyRetentionDays: number;
  autoUpdate: boolean;
  proxyUrl: string;
  requestTimeoutSecs: number;
  /** Dictation engine: Windows speech recognition or the configured AI provider. */
  speechEngine: SpeechEngine;
  /** BCP-47 tag, "" = system speech language / auto-detect. */
  speechLanguage: string;
  openaiTranscriptionModel: string;
  paused: boolean;
  shortcuts: Shortcuts;
  customPrompts: Record<string, string>;
  hiddenActions: string[];
  pricing: Record<string, ModelPrice>;
  onboarded: boolean;
}

export type SpeechEngine = "native" | "ai";

export interface SpeechInfo {
  /** Windows speech recognition is available on this machine. */
  native: boolean;
  nativeLanguages: { tag: string; name: string }[];
  systemLanguage: string;
}

export interface ActionInfo {
  id: string;
  label: string;
  defaultPrompt: string;
}

export interface Catalog {
  actions: ActionInfo[];
  languages: string[];
}

export interface KeyStatus {
  configured: boolean;
  masked: string | null;
}

export interface Completion {
  text: string;
  inputTokens: number;
  outputTokens: number;
  estimatedTokens: boolean;
}

export interface ModelInfo {
  id: string;
  name: string;
}

export interface ShortcutFailure {
  command: string;
  shortcut: string;
  reason: "invalid" | "duplicate" | "unavailable";
}

export interface PopupPayload {
  text: string;
  /** The selection was copied automatically. */
  hasSelection: boolean;
  /** The origin window is known: "Replace" pastes the result back there. */
  canReplace: boolean;
  /** The copy is still on its way; another "popup:open" with the text follows. */
  capturing: boolean;
}

/** Palette window preferences, toggled from the palette header. */
export interface PopupPrefs {
  /** Stay open and on top when another window is clicked. */
  keepOpen: boolean;
  /** Open where it was locked instead of next to the cursor. */
  lockPosition: boolean;
}

export interface Totals {
  requests: number;
  errors: number;
  inputTokens: number;
  outputTokens: number;
  avgLatencyMs: number;
}

export interface DayBucket {
  day: string;
  requests: number;
  inputTokens: number;
  outputTokens: number;
}

export interface KeyBucket {
  key: string;
  requests: number;
  inputTokens: number;
  outputTokens: number;
}

export interface UsageSummary {
  totals: Totals;
  byDay: DayBucket[];
  byAction: KeyBucket[];
  byModel: KeyBucket[];
}

export interface HistoryItem {
  id: number;
  ts: number;
  action: string;
  model: string;
  targetLanguage: string | null;
  input: string;
  output: string;
  inputTokens: number;
  outputTokens: number;
}

export interface AppInfo {
  version: string;
  os: string;
  sessionType: string;
  dataDir: string;
  logDir: string;
}

/** Error shape thrown by every command (see engine::CommandError). */
export interface CommandError {
  code: string;
  message: string;
}

export function isCommandError(e: unknown): e is CommandError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<ShortcutFailure[]>("save_settings", { settings }),
  getCatalog: () => invoke<Catalog>("get_catalog"),
  getApiKeyStatus: (provider: ProviderKind) => invoke<KeyStatus>("get_api_key_status", { provider }),
  setApiKey: (provider: ProviderKind, key: string) => invoke<KeyStatus>("set_api_key", { provider, key }),
  deleteApiKey: (provider: ProviderKind) => invoke<void>("delete_api_key", { provider }),
  testConnection: () => invoke<{ latencyMs: number; model: string; reply: string }>("test_connection"),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  runAction: (action: string, text: string, targetLanguage?: string) => invoke<Completion>("run_action", { action, text, targetLanguage }),
  popupState: () => invoke<PopupPayload>("popup_state"),
  popupApply: (text: string) => invoke<"pasted" | "copied">("popup_apply", { text }),
  popupClose: () => invoke<void>("popup_close"),
  speechInfo: () => invoke<SpeechInfo>("speech_info"),
  speechStart: () => invoke<SpeechEngine>("speech_start"),
  /** Resolves with the transcript (AI engine) or null (native engine streams events). */
  speechStop: () => invoke<string | null>("speech_stop"),
  speechCancel: () => invoke<void>("speech_cancel"),
  openSpeechSettings: () => invoke<void>("open_speech_settings"),
  popupPrefs: () => invoke<PopupPrefs>("popup_prefs"),
  popupSetPrefs: (prefs: Partial<PopupPrefs>) => invoke<PopupPrefs>("popup_set_prefs", prefs),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  openSettings: (tab?: string) => invoke<void>("open_settings", { tab }),
  usageSummary: (days: number) => invoke<UsageSummary>("usage_summary", { days, tzOffsetSecs: -new Date().getTimezoneOffset() * 60 }),
  resetUsage: () => invoke<void>("reset_usage"),
  listHistory: (query: string, limit: number, offset: number) =>
    invoke<{ items: HistoryItem[]; total: number }>("list_history", { query, limit, offset }),
  deleteHistory: (id: number) => invoke<void>("delete_history", { id }),
  clearHistory: () => invoke<void>("clear_history"),
  exportHistoryCsv: (path: string) => invoke<number>("export_history_csv", { path }),
  shortcutStatus: () => invoke<ShortcutFailure[]>("shortcut_status"),
  suspendShortcuts: (suspended: boolean) => invoke<ShortcutFailure[]>("suspend_shortcuts", { suspended }),
  appInfo: () => invoke<AppInfo>("app_info"),
};
