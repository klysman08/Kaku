//! Runs an action end-to-end: prompt → provider → usage/history bookkeeping.

use crate::ai::{self, AiError, Completion};
use crate::db::{HistoryItem, UsageRecord};
use crate::state::AppState;
use crate::{actions, secrets};
use serde::Serialize;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

/// Error shape returned to the webview: `code` drives the friendly i18n message.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<AiError> for CommandError {
    fn from(e: AiError) -> Self {
        let message = match &e {
            AiError::Http { message, .. } => message.clone(),
            other => other.to_string(),
        };
        Self {
            code: e.code(),
            message,
        }
    }
}

impl From<rusqlite::Error> for CommandError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("db", e.to_string())
    }
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::new("internal", message)
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub async fn run(
    app: &AppHandle,
    action: &str,
    text: &str,
    target_language: Option<&str>,
    source: &str,
) -> Result<Completion, CommandError> {
    if !actions::is_valid(action) {
        return Err(CommandError::new("invalid_action", action));
    }
    if text.trim().is_empty() {
        return Err(CommandError::new("empty_input", "no text"));
    }
    let state = app.state::<AppState>();
    let settings = state.settings();
    let language = target_language
        .filter(|l| !l.trim().is_empty())
        .unwrap_or(&settings.target_language)
        .to_string();
    let custom = settings.custom_prompts.get(action).map(String::as_str);
    let system = actions::system_prompt(action, custom, &language)
        .ok_or_else(|| CommandError::new("invalid_action", action))?;
    let key = secrets::get_key(settings.provider).map_err(|e| CommandError::new("keyring", e))?;

    let started = Instant::now();
    let result = ai::complete(&settings, key.as_deref(), &system, text).await;
    let latency_ms = started.elapsed().as_millis() as u64;
    let ts = now();

    {
        let db = state.db();
        let (input_tokens, output_tokens) = result
            .as_ref()
            .map(|c| (c.input_tokens, c.output_tokens))
            .unwrap_or((0, 0));
        let usage = UsageRecord {
            ts,
            action,
            provider: settings.provider.as_str(),
            model: settings.model(),
            input_tokens,
            output_tokens,
            latency_ms,
            ok: result.is_ok(),
            error_code: result.as_ref().err().map(AiError::code),
            source,
        };
        if let Err(e) = db.record_usage(&usage) {
            log::error!("failed to record usage: {e}");
        }
        if let (Ok(c), true) = (&result, settings.history_enabled) {
            let item = HistoryItem {
                id: 0,
                ts,
                action: action.to_string(),
                model: settings.model().to_string(),
                target_language: (action == "translate").then(|| language.clone()),
                input: text.to_string(),
                output: c.text.clone(),
                input_tokens: c.input_tokens,
                output_tokens: c.output_tokens,
            };
            if let Err(e) = db.add_history(&item) {
                log::error!("failed to save history: {e}");
            }
        }
    }
    // Content is never logged — only metadata.
    match &result {
        Ok(c) => log::info!(
            "action={action} source={source} ok latency={latency_ms}ms tokens={}/{}",
            c.input_tokens,
            c.output_tokens
        ),
        Err(e) => log::warn!("action={action} source={source} failed code={}", e.code()),
    }
    let _ = app.emit("usage-changed", ());
    result.map_err(CommandError::from)
}

/// Transcribe a dictation WAV with the configured provider. Counted on the dashboard as
/// the "dictation" action; never stored in the history.
pub async fn transcribe(
    app: &AppHandle,
    wav: &[u8],
    language: &str,
) -> Result<Completion, CommandError> {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let key = secrets::get_key(settings.provider).map_err(|e| CommandError::new("keyring", e))?;
    let started = Instant::now();
    let result = ai::transcribe(&settings, key.as_deref(), wav, language).await;
    let latency_ms = started.elapsed().as_millis() as u64;
    let model = match settings.provider {
        crate::settings::ProviderKind::Gemini => settings.gemini_model.as_str(),
        crate::settings::ProviderKind::Openai => settings.openai_transcription_model.as_str(),
    };
    let (input_tokens, output_tokens) = result
        .as_ref()
        .map(|c| (c.input_tokens, c.output_tokens))
        .unwrap_or((0, 0));
    let usage = UsageRecord {
        ts: now(),
        action: "dictation",
        provider: settings.provider.as_str(),
        model,
        input_tokens,
        output_tokens,
        latency_ms,
        ok: result.is_ok(),
        error_code: result.as_ref().err().map(AiError::code),
        source: "popup",
    };
    if let Err(e) = state.db().record_usage(&usage) {
        log::error!("failed to record usage: {e}");
    }
    match &result {
        Ok(c) => log::info!(
            "action=dictation ok latency={latency_ms}ms audio={}KB tokens={}/{}",
            wav.len() / 1024,
            c.input_tokens,
            c.output_tokens
        ),
        Err(e) => log::warn!("action=dictation failed code={}", e.code()),
    }
    let _ = app.emit("usage-changed", ());
    result.map_err(CommandError::from)
}
