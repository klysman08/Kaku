// Dev-only stand-in for the Rust backend, so the UI renders in a plain browser with sample
// data (README screenshots, quick UI work). Enabled with `?mock` on the Vite dev server:
//
//   pnpm dev
//   http://localhost:1420/?mock&tab=dashboard&theme=dark          (settings window)
//   http://localhost:1420/popup.html?mock&scene=result&theme=dark  (palette)
//
// Never bundled: main.tsx only imports it behind `import.meta.env.DEV`.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import pkg from "../../package.json";
import type { Settings, UsageSummary } from "@/lib/ipc";

const params = new URLSearchParams(location.search);
const theme = (params.get("theme") ?? "dark") as Settings["theme"];
const scene = params.get("scene") ?? "menu";

const ACTIONS = [
  "rewrite",
  "review",
  "professional",
  "appealing",
  "friendly",
  "simplify",
  "summarize",
  "detail",
  "emojis",
  "shorten",
  "translate",
];

const SAMPLE = "hey team, the launch is moved to thursday cause the build wasnt ready, sorry for the late notice";
const RESULT =
  "Hi team, the launch has been moved to Thursday because the build was not ready yet. Apologies for the short notice, and thank you for your flexibility.";

const settings: Settings = {
  provider: "gemini",
  geminiModel: "gemini-3.7-flash",
  openaiBaseUrl: "https://api.openai.com/v1",
  openaiModel: "gpt-5-mini",
  targetLanguage: "English (US)",
  uiLanguage: "en",
  theme,
  sounds: false,
  minimizeToTray: true,
  historyEnabled: true,
  historyRetentionDays: 30,
  autoUpdate: true,
  proxyUrl: "",
  requestTimeoutSecs: 45,
  speechEngine: "native",
  speechLanguage: "",
  openaiTranscriptionModel: "gpt-4o-mini-transcribe",
  paused: false,
  shortcuts: {
    popup: "Ctrl+Shift+Space",
    actions: { review: "Ctrl+Shift+1", translate: "Ctrl+Shift+2", professional: "Ctrl+Shift+3" },
  },
  customPrompts: {},
  hiddenActions: [],
  pricing: { "gemini-3.7-flash": { input: 0.3, output: 2.5 } },
  onboarded: true,
};

/** Deterministic pseudo-random numbers, so every capture looks the same. */
function rng(seed: number) {
  return () => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 2147483648;
  };
}

function usage(days: number): UsageSummary {
  const rand = rng(7);
  const byDay = Array.from({ length: days }, (_, i) => {
    const d = new Date(Date.UTC(2026, 10, 5) - (days - 1 - i) * 86_400_000);
    const weekend = [0, 6].includes(d.getUTCDay());
    const requests = Math.round((weekend ? 6 : 26) + rand() * (weekend ? 8 : 30));
    return {
      day: d.toISOString().slice(0, 10),
      requests,
      inputTokens: requests * Math.round(150 + rand() * 90),
      outputTokens: requests * Math.round(70 + rand() * 60),
    };
  });
  const sum = (k: "requests" | "inputTokens" | "outputTokens") => byDay.reduce((a, b) => a + b[k], 0);
  const requests = sum("requests");
  const split = [
    ["review", 0.34],
    ["translate", 0.22],
    ["professional", 0.16],
    ["rewrite", 0.1],
    ["dictation", 0.08],
    ["summarize", 0.06],
    ["friendly", 0.04],
  ] as const;
  const bucket = (key: string, share: number) => ({
    key,
    requests: Math.round(requests * share),
    inputTokens: Math.round(sum("inputTokens") * share),
    outputTokens: Math.round(sum("outputTokens") * share),
  });
  return {
    totals: {
      requests,
      errors: Math.round(requests * 0.012),
      inputTokens: sum("inputTokens"),
      outputTokens: sum("outputTokens"),
      avgLatencyMs: 1180,
    },
    byDay,
    byAction: split.map(([k, s]) => bucket(k, s)),
    byModel: [bucket("gemini-3.7-flash", 0.86), bucket("gemini-3.5-flash-lite", 0.14)],
  };
}

const HISTORY = [
  [
    "professional",
    "can u send me the report by eod, need it for the meeting",
    "Could you please send me the report by the end of the day? I need it for the meeting.",
  ],
  ["translate", "A reunião foi remarcada para quinta-feira às dez horas.", "The meeting has been rescheduled to Thursday at ten o'clock."],
  ["review", "Their going to review the the proposal tomorow.", "They're going to review the proposal tomorrow."],
  [
    "summarize",
    "Quarterly results: revenue grew 18% year over year, driven by enterprise...",
    "Revenue grew 18% YoY, led by enterprise deals; margins improved.",
  ],
  ["friendly", "Your request has been received and will be processed.", "Thanks so much! We got your request and we're on it."],
].map(([action, input, output], i) => ({
  id: i + 1,
  ts: Math.floor(Date.now() / 1000) - (i + 1) * 1500,
  action,
  model: "gemini-3.7-flash",
  targetLanguage: action === "translate" ? "English (US)" : null,
  input,
  output,
  inputTokens: 180 + i * 13,
  outputTokens: 60 + i * 9,
}));

const later = <T>(value: T, ms = 250) => new Promise<T>((r) => setTimeout(() => r(value), ms));

function handle(cmd: string, args: Record<string, unknown> | undefined): unknown {
  switch (cmd) {
    case "get_settings":
      return settings;
    case "save_settings":
    case "shortcut_status":
    case "suspend_shortcuts":
      return [];
    case "get_catalog":
      return {
        actions: ACTIONS.map((id) => ({ id, label: id, defaultPrompt: `Default instructions for ${id}.` })),
        languages: ["English (US)", "English (UK)", "Português (Brasil)", "Español", "Français", "Deutsch", "日本語"],
      };
    case "get_api_key_status":
      return { configured: true, masked: "AIza••••••••••••Q3kM" };
    case "list_models":
      return [
        { id: "gemini-3.7-flash", name: "Gemini 3.7 Flash" },
        { id: "gemini-3.5-flash-lite", name: "Gemini 3.5 Flash-Lite" },
        { id: "gemini-3.7-pro", name: "Gemini 3.7 Pro" },
      ];
    case "test_connection":
      return later({ latencyMs: 640, model: settings.geminiModel, reply: "OK" });
    case "usage_summary":
      return usage(Number(args?.days ?? 30));
    case "list_history":
      return { items: HISTORY, total: HISTORY.length };
    case "app_info":
      return { version: pkg.version, os: "windows", sessionType: "", dataDir: "", logDir: "" };
    case "speech_info":
      return {
        native: true,
        voiceTyping: true,
        nativeLanguages: [
          { tag: "en-US", name: "English (United States)" },
          { tag: "pt-BR", name: "Português (Brasil)" },
        ],
        systemLanguage: "en-US",
      };
    case "popup_state":
      return scene === "dictation"
        ? { text: "", hasSelection: false, canReplace: true, capturing: false }
        : { text: SAMPLE, hasSelection: true, canReplace: true, capturing: false };
    case "popup_prefs":
    case "popup_set_prefs":
      return { keepOpen: false, lockPosition: false };
    case "run_action":
      return later({ text: RESULT, inputTokens: 190, outputTokens: 41, estimatedTokens: false }, 120);
    case "speech_start":
      setTimeout(() => void emit("speech:final", "Hi team, quick update on the launch."), 150);
      setTimeout(() => void emit("speech:partial", "it is moving to Thursday"), 300);
      return "native";
    case "plugin:autostart|is_enabled":
      return true;
    default:
      return null;
  }
}

/** Click through the palette to the requested scene once it has rendered. */
function playScene() {
  const clickWhenReady = (selector: string, tries = 40) => {
    const el = document.querySelector<HTMLElement>(selector);
    // Give the palette a moment to load its payload after the buttons appear.
    if (el) setTimeout(() => el.click(), 500);
    else if (tries > 0) setTimeout(() => clickWhenReady(selector, tries - 1), 100);
  };
  if (scene === "result") clickWhenReady('button[data-value="professional"]');
  // The microphone is the only button with both aria-label and aria-pressed.
  if (scene === "dictation") clickWhenReady('button[aria-label][aria-pressed="false"]');
}

export function install(label: "main" | "popup") {
  mockWindows(label);
  mockIPC((cmd, args) => handle(cmd, args as Record<string, unknown>), { shouldMockEvents: true });
  if (label === "popup") playScene();
  else {
    const tab = params.get("tab");
    if (tab) setTimeout(() => void emit("navigate", tab), 300);
  }
}
