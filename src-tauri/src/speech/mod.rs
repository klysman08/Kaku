//! Dictation for the palette's text box. Two engines, picked in Settings › General:
//!
//! - "native" (Windows only): Windows speech recognition, text appears while you speak.
//! - "voice_typing" (Windows only): opens Windows voice typing (Win+H), which types into
//!   the focused text box by itself. Kaku only launches it; there is no session to track.
//! - "ai": record the microphone, then transcribe with the configured provider
//!   (Gemini audio input, or `/audio/transcriptions` on OpenAI-compatible servers).
//!
//! Events to the palette: "speech:state" ("listening" | "transcribing" | "idle"),
//! "speech:level" (0..1, AI engine), "speech:partial" / "speech:final" (native engine),
//! "speech:error" (a `CommandError`).

#[cfg(windows)]
mod native;
pub mod recorder;

use crate::engine::{self, CommandError};
use crate::state::AppState;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

enum Session {
    Ai(recorder::Recording),
    #[cfg(windows)]
    Native(native::Dictation),
}

static ACTIVE: Mutex<Option<Session>> = Mutex::new(None);

fn active() -> std::sync::MutexGuard<'static, Option<Session>> {
    ACTIVE.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SpeechLanguage {
    pub tag: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeechInfo {
    /// Windows speech recognition can be used on this machine.
    pub native: bool,
    /// Windows voice typing (Win+H) exists on this OS.
    pub voice_typing: bool,
    pub native_languages: Vec<SpeechLanguage>,
    /// Windows' speech language (used when no language is chosen).
    pub system_language: String,
}

pub fn info() -> SpeechInfo {
    #[cfg(windows)]
    {
        let (native_languages, system_language) = native::languages();
        SpeechInfo {
            native: !native_languages.is_empty(),
            voice_typing: true,
            native_languages,
            system_language,
        }
    }
    #[cfg(not(windows))]
    SpeechInfo {
        native: false,
        voice_typing: false,
        native_languages: Vec::new(),
        system_language: String::new(),
    }
}

pub(crate) fn emit_state(app: &AppHandle, state: &str) {
    let _ = app.emit("speech:state", state);
}

/// Only the Windows engine reports errors after start; the AI engine returns them.
#[cfg(windows)]
pub(crate) fn emit_error(app: &AppHandle, err: &CommandError) {
    let _ = app.emit("speech:error", err);
}

/// A native session ended by itself (silence timeout, error). Returns false when it is
/// no longer the current session. The objects are released off-thread: releasing WinRT
/// objects inside their own event handler is not allowed.
#[cfg(windows)]
pub(crate) fn forget_native(id: u64) -> bool {
    let mut guard = active();
    if !matches!(&*guard, Some(Session::Native(d)) if d.id == id) {
        return false;
    }
    let ended = guard.take();
    drop(guard);
    std::thread::spawn(move || drop(ended));
    true
}

#[cfg(windows)]
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Start dictation with the engine from settings. Returns the engine used.
pub async fn start(app: &AppHandle) -> Result<&'static str, CommandError> {
    cancel();
    let settings = app.state::<AppState>().settings();
    let language = settings.speech_language.clone();

    #[cfg(windows)]
    if settings.speech_engine == "voice_typing" {
        tauri::async_runtime::spawn_blocking(crate::selection::toggle_voice_typing)
            .await
            .map_err(|e| CommandError::new("speech_failed", e.to_string()))?
            .map_err(|e| CommandError::new("speech_failed", e.to_string()))?;
        log::info!("voice typing toggled (Win+H)");
        return Ok("voice_typing");
    }

    #[cfg(windows)]
    if settings.speech_engine == "native" {
        let a = app.clone();
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dictation =
            tauri::async_runtime::spawn_blocking(move || native::start(a, id, &language))
                .await
                .map_err(|e| CommandError::new("speech_failed", e.to_string()))??;
        *active() = Some(Session::Native(dictation));
        emit_state(app, "listening");
        log::info!("dictation started (native)");
        return Ok("native");
    }
    let _ = language;

    let level_app = app.clone();
    let limit_app = app.clone();
    let recording = tauri::async_runtime::spawn_blocking(move || {
        recorder::start(
            move |level| {
                let _ = level_app.emit("speech:level", level);
            },
            move || {
                let _ = limit_app.emit("speech:limit", ());
            },
        )
    })
    .await
    .map_err(|e| CommandError::new("speech_failed", e.to_string()))?
    .map_err(|e| match e {
        recorder::RecordError::NoDevice => CommandError::new("speech_mic", e.to_string()),
        recorder::RecordError::Device(m) => CommandError::new("speech_mic", m),
    })?;
    *active() = Some(Session::Ai(recording));
    emit_state(app, "listening");
    log::info!("dictation started (ai)");
    Ok("ai")
}

/// Stop dictation. The AI engine returns the transcript; the native engine has already
/// streamed its text through events and returns None.
pub async fn stop(app: &AppHandle) -> Result<Option<String>, CommandError> {
    let session = active().take();
    match session {
        None => Ok(None),
        #[cfg(windows)]
        Some(Session::Native(d)) => {
            let _ = tauri::async_runtime::spawn_blocking(move || d.stop()).await;
            emit_state(app, "idle");
            Ok(None)
        }
        Some(Session::Ai(recording)) => {
            emit_state(app, "transcribing");
            let result = transcribe(app, recording).await;
            emit_state(app, "idle");
            result.map(Some)
        }
    }
}

async fn transcribe(
    app: &AppHandle,
    recording: recorder::Recording,
) -> Result<String, CommandError> {
    let samples = tauri::async_runtime::spawn_blocking(move || recording.finish())
        .await
        .map_err(|e| CommandError::new("speech_failed", e.to_string()))?;
    if !recorder::has_speech(&samples, Duration::from_millis(300)) {
        return Err(CommandError::new("speech_empty", "nothing was heard"));
    }
    let wav = recorder::to_wav(&samples);
    let language = app.state::<AppState>().settings().speech_language;
    match engine::transcribe(app, &wav, &language).await {
        Ok(c) => Ok(c.text),
        Err(e) if e.code == "empty" => Err(CommandError::new("speech_empty", e.message)),
        Err(e) => Err(e),
    }
}

/// Drop any running dictation without a result (palette closed, new start).
pub fn cancel() {
    let Some(session) = active().take() else {
        return;
    };
    std::thread::spawn(move || match session {
        Session::Ai(r) => drop(r.finish()),
        #[cfg(windows)]
        Session::Native(d) => d.cancel(),
    });
}

#[cfg(test)]
mod tests {
    use crate::settings::{ProviderKind, SettingsStore};

    /// Manual smoke test against the real provider, using your saved settings and key:
    /// `KAKU_TEST_WAV=sample.wav cargo test speech -- --ignored --nocapture`
    /// (16 kHz mono 16-bit WAV; Windows TTS can make one without a microphone).
    #[tokio::test]
    #[ignore = "calls the configured AI provider"]
    async fn transcribes_a_sample_with_the_configured_provider() {
        let wav = std::fs::read(std::env::var("KAKU_TEST_WAV").expect("KAKU_TEST_WAV")).unwrap();
        let config = if cfg!(windows) {
            std::env::var("APPDATA").unwrap()
        } else {
            format!("{}/.config", std::env::var("HOME").unwrap())
        };
        let settings =
            SettingsStore::new(&std::path::Path::new(&config).join("io.github.klysman08.kaku"))
                .load();
        let key = crate::secrets::get_key(settings.provider).unwrap();
        assert!(
            key.is_some() || settings.provider == ProviderKind::Openai,
            "no API key saved"
        );
        let started = std::time::Instant::now();
        let c = crate::ai::transcribe(&settings, key.as_deref(), &wav, &settings.speech_language)
            .await
            .unwrap();
        println!(
            "{:?} {} in {:?}, tokens {}/{}: {}",
            settings.provider,
            settings.model(),
            started.elapsed(),
            c.input_tokens,
            c.output_tokens,
            c.text
        );
        assert!(!c.text.is_empty());
        assert!(!c.text.contains('—'));
    }
}
