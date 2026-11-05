//! User preferences persisted as JSON in the app config directory.
//! Secrets (API keys) are NOT stored here — see `secrets.rs`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    Gemini,
    /// Any OpenAI-compatible Chat Completions endpoint (OpenAI, Azure OpenAI gateway,
    /// OpenRouter, LiteLLM, Ollama, LM Studio...).
    Openai,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Gemini => "gemini",
            ProviderKind::Openai => "openai",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Shortcuts {
    /// Opens the action palette near the cursor.
    pub popup: String,
    /// Per-action shortcuts that replace the selection directly (empty = disabled).
    pub actions: BTreeMap<String, String>,
}

impl Default for Shortcuts {
    fn default() -> Self {
        let mut actions = BTreeMap::new();
        actions.insert("review".into(), "Ctrl+Shift+1".into());
        actions.insert("translate".into(), "Ctrl+Shift+2".into());
        actions.insert("professional".into(), "Ctrl+Shift+3".into());
        Self {
            popup: "Ctrl+Shift+Space".into(),
            actions,
        }
    }
}

/// Price per one million tokens, used only for the dashboard cost estimate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub provider: ProviderKind,
    pub gemini_model: String,
    pub openai_base_url: String,
    pub openai_model: String,

    /// Default target language for Translate.
    pub target_language: String,
    /// "auto" | "pt-BR" | "en"
    pub ui_language: String,
    /// "system" | "dark" | "light"
    pub theme: String,
    pub sounds: bool,
    /// Minimize and close buttons send the window to the tray instead of the taskbar/quitting.
    pub minimize_to_tray: bool,

    pub history_enabled: bool,
    pub history_retention_days: u32,

    pub auto_update: bool,
    /// Explicit proxy URL (e.g. http://proxy.corp:8080). Empty = use the system proxy.
    pub proxy_url: String,
    pub request_timeout_secs: u64,

    /// Dictation in the palette: "native" (Windows speech recognition, live text),
    /// "voice_typing" (Windows voice typing, the Win+H panel) or "ai" (record, then
    /// transcribe with the configured provider). Linux: "ai" only.
    pub speech_engine: String,
    /// BCP-47 tag for dictation ("" = system speech language / auto-detect).
    pub speech_language: String,
    /// OpenAI-compatible transcription model (`/audio/transcriptions`).
    pub openai_transcription_model: String,

    /// Temporarily disables all global shortcuts.
    pub paused: bool,
    pub shortcuts: Shortcuts,
    /// Custom system prompts keyed by action id. Must contain "CRITICAL:" to be used.
    pub custom_prompts: BTreeMap<String, String>,
    /// Actions hidden from the palette.
    pub hidden_actions: Vec<String>,
    /// Prices keyed by model id.
    pub pricing: BTreeMap<String, ModelPrice>,

    /// Set once the first-run onboarding has been shown.
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Gemini,
            gemini_model: "gemini-3.7-flash".into(),
            openai_base_url: "https://api.openai.com/v1".into(),
            openai_model: "gpt-5-mini".into(),
            target_language: "English (US)".into(),
            ui_language: "auto".into(),
            theme: "system".into(),
            sounds: true,
            minimize_to_tray: true,
            history_enabled: true,
            history_retention_days: 30,
            auto_update: true,
            proxy_url: String::new(),
            request_timeout_secs: 45,
            speech_engine: default_speech_engine().into(),
            speech_language: String::new(),
            openai_transcription_model: "gpt-4o-mini-transcribe".into(),
            paused: false,
            shortcuts: Shortcuts::default(),
            custom_prompts: BTreeMap::new(),
            hidden_actions: Vec::new(),
            pricing: BTreeMap::new(),
            onboarded: false,
        }
    }
}

impl Settings {
    pub fn model(&self) -> &str {
        match self.provider {
            ProviderKind::Gemini => &self.gemini_model,
            ProviderKind::Openai => &self.openai_model,
        }
    }

    /// Clamp values a hand-edited file could break.
    pub fn sanitize(mut self) -> Self {
        self.request_timeout_secs = self.request_timeout_secs.clamp(5, 300);
        self.history_retention_days = self.history_retention_days.min(3650);
        self.openai_base_url = self
            .openai_base_url
            .trim()
            .trim_end_matches('/')
            .to_string();
        self.proxy_url = self.proxy_url.trim().to_string();
        if !speech_engine_available(&self.speech_engine) {
            self.speech_engine = default_speech_engine().into();
        }
        self.speech_language = self.speech_language.trim().to_string();
        if self.openai_transcription_model.trim().is_empty() {
            self.openai_transcription_model = Settings::default().openai_transcription_model;
        }
        if self.target_language.trim().is_empty() {
            self.target_language = Settings::default().target_language;
        }
        self
    }
}

/// Native dictation exists only on Windows.
pub fn default_speech_engine() -> &'static str {
    if cfg!(windows) { "native" } else { "ai" }
}

pub fn speech_engine_available(engine: &str) -> bool {
    engine == "ai" || (matches!(engine, "native" | "voice_typing") && cfg!(windows))
}

/// Palette window preferences, toggled from the palette itself. Kept apart from
/// [`Settings`] so saving the Settings window never overwrites them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PopupPrefs {
    /// Stay open (and on top) when another window is clicked.
    pub keep_open: bool,
    /// Open at `position` instead of next to the cursor.
    pub lock_position: bool,
    /// Top-left corner in physical pixels.
    pub position: Option<(i32, i32)>,
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("settings.json"),
        }
    }

    /// Missing or corrupt files fall back to defaults (a corrupt file is kept as .bak).
    pub fn load(&self) -> Settings {
        match fs::read_to_string(&self.path) {
            Ok(raw) => match serde_json::from_str::<Settings>(&raw) {
                Ok(s) => s.sanitize(),
                Err(e) => {
                    log::warn!("settings.json is invalid ({e}); using defaults");
                    let _ = fs::rename(&self.path, self.path.with_extension("json.bak"));
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self, settings: &Settings) -> std::io::Result<()> {
        write_atomic(&self.path, &serde_json::to_vec_pretty(settings)?)
    }

    fn popup_path(&self) -> PathBuf {
        self.path.with_file_name("popup.json")
    }

    pub fn load_popup_prefs(&self) -> PopupPrefs {
        fs::read_to_string(self.popup_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_popup_prefs(&self, prefs: &PopupPrefs) -> std::io::Result<()> {
        write_atomic(&self.popup_path(), &serde_json::to_vec_pretty(prefs)?)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        assert_eq!(store.load(), Settings::default());

        let s = Settings {
            provider: ProviderKind::Openai,
            openai_base_url: "http://localhost:11434/v1/".into(),
            ..Settings::default()
        };
        store.save(&s).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.provider, ProviderKind::Openai);
        assert_eq!(loaded.openai_base_url, "http://localhost:11434/v1");

        // Older/partial files keep defaults for missing keys.
        fs::write(dir.path().join("settings.json"), r#"{"theme":"dark"}"#).unwrap();
        let partial = store.load();
        assert_eq!(partial.theme, "dark");
        assert_eq!(partial.shortcuts.popup, "Ctrl+Shift+Space");
    }

    #[test]
    fn corrupt_file_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.json"), "{not json").unwrap();
        let store = SettingsStore::new(dir.path());
        assert_eq!(store.load(), Settings::default());
        assert!(dir.path().join("settings.json.bak").exists());
    }

    #[test]
    fn speech_engines_follow_the_platform() {
        assert!(speech_engine_available("ai"));
        assert_eq!(speech_engine_available("native"), cfg!(windows));
        assert_eq!(speech_engine_available("voice_typing"), cfg!(windows));
        assert!(!speech_engine_available("whisper"));
        let s = Settings {
            speech_engine: "voice_typing".into(),
            ..Settings::default()
        }
        .sanitize();
        let expected = if cfg!(windows) { "voice_typing" } else { "ai" };
        assert_eq!(s.speech_engine, expected);
    }

    #[test]
    fn popup_prefs_live_apart_from_settings() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        assert_eq!(store.load_popup_prefs(), PopupPrefs::default());
        let prefs = PopupPrefs {
            keep_open: true,
            lock_position: true,
            position: Some((-1200, 340)),
        };
        store.save_popup_prefs(&prefs).unwrap();
        // Saving the Settings window must not touch the palette preferences.
        store.save(&Settings::default()).unwrap();
        assert_eq!(store.load_popup_prefs(), prefs);
        fs::write(dir.path().join("popup.json"), "garbage").unwrap();
        assert_eq!(store.load_popup_prefs(), PopupPrefs::default());
    }
}
