//! Windows speech recognition (Windows.Media.SpeechRecognition): live dictation with
//! partial results. Free-form dictation runs on Microsoft's online service, so it needs
//! Settings › Privacy & security › Speech › "Online speech recognition" turned on.

use super::{SpeechLanguage, emit_error, emit_state};
use crate::engine::CommandError;
use tauri::{AppHandle, Emitter};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Globalization::Language;
use windows::Media::SpeechRecognition::{
    SpeechContinuousRecognitionCompletedEventArgs,
    SpeechContinuousRecognitionResultGeneratedEventArgs, SpeechContinuousRecognitionSession,
    SpeechRecognitionConfidence, SpeechRecognitionHypothesisGeneratedEventArgs,
    SpeechRecognitionResultStatus, SpeechRecognizer,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::{HRESULT, HSTRING, Ref};

/// SPERR_SPEECH_PRIVACY_POLICY_NOT_ACCEPTED: "Online speech recognition" is off.
const PRIVACY_NOT_ACCEPTED: HRESULT = HRESULT(0x8004_5509_u32 as i32);
const ACCESS_DENIED: HRESULT = HRESULT(0x8007_0005_u32 as i32);
/// Stop on our own after this much silence (the user normally clicks the mic again).
const SILENCE_TIMEOUT_SECS: i64 = 60;

/// WinRT needs COM on the calling thread; tokio's threads start without it.
fn ensure_com() {
    // SAFETY: plain initialisation call; S_FALSE / RPC_E_CHANGED_MODE are fine to ignore.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn win_err(e: windows::core::Error) -> CommandError {
    let code = match e.code() {
        PRIVACY_NOT_ACCEPTED => "speech_privacy",
        ACCESS_DENIED => "speech_mic",
        _ => "speech_failed",
    };
    CommandError::new(code, format!("{} ({:#010x})", e.message(), e.code().0))
}

/// Dictation languages installed on this PC, plus the system speech language tag.
pub fn languages() -> (Vec<SpeechLanguage>, String) {
    ensure_com();
    let list = SpeechRecognizer::SupportedTopicLanguages()
        .map(|v| {
            v.into_iter()
                .filter_map(|l| {
                    Some(SpeechLanguage {
                        tag: l.LanguageTag().ok()?.to_string(),
                        name: l.NativeName().ok()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let system = SpeechRecognizer::SystemSpeechLanguage()
        .and_then(|l| l.LanguageTag())
        .map(|t| t.to_string())
        .unwrap_or_default();
    (list, system)
}

pub struct Dictation {
    pub id: u64,
    // Kept alive for the whole session; the session belongs to it.
    _recognizer: SpeechRecognizer,
    session: SpeechContinuousRecognitionSession,
}

/// Start listening. Partial text arrives as "speech:partial", finished phrases as
/// "speech:final"; "speech:state" = "idle" when the session ends for any reason.
pub fn start(app: AppHandle, id: u64, language: &str) -> Result<Dictation, CommandError> {
    ensure_com();
    let recognizer = if language.is_empty() {
        SpeechRecognizer::new()
    } else {
        Language::CreateLanguage(&HSTRING::from(language))
            .and_then(|l| SpeechRecognizer::Create(&l))
    }
    .map_err(win_err)?;

    // No constraint added = the predefined free-form dictation grammar.
    let compiled = recognizer
        .CompileConstraintsAsync()
        .and_then(|op| op.join())
        .map_err(win_err)?;
    let status = compiled.Status().map_err(win_err)?;
    if status != SpeechRecognitionResultStatus::Success {
        return Err(CommandError::new(
            "speech_language",
            format!(
                "dictation is not available for this language (status {})",
                status.0
            ),
        ));
    }

    let session = recognizer.ContinuousRecognitionSession().map_err(win_err)?;
    let _ = session.SetAutoStopSilenceTimeout(TimeSpan {
        Duration: SILENCE_TIMEOUT_SECS * 10_000_000,
    });

    let a = app.clone();
    recognizer
        .HypothesisGenerated(&TypedEventHandler::new(
            move |_, args: Ref<SpeechRecognitionHypothesisGeneratedEventArgs>| {
                if let Some(args) = args.as_ref() {
                    let text = args.Hypothesis()?.Text()?.to_string();
                    let _ = a.emit("speech:partial", text);
                }
                Ok(())
            },
        ))
        .map_err(win_err)?;

    let a = app.clone();
    session
        .ResultGenerated(&TypedEventHandler::new(
            move |_, args: Ref<SpeechContinuousRecognitionResultGeneratedEventArgs>| {
                if let Some(args) = args.as_ref() {
                    let result = args.Result()?;
                    let accepted = result.Status()? == SpeechRecognitionResultStatus::Success
                        && result.Confidence()? != SpeechRecognitionConfidence::Rejected;
                    let text = result.Text()?.to_string();
                    if accepted && !text.trim().is_empty() {
                        let _ = a.emit("speech:final", text);
                    }
                }
                Ok(())
            },
        ))
        .map_err(win_err)?;

    let a = app.clone();
    session
        .Completed(&TypedEventHandler::new(
            move |_, args: Ref<SpeechContinuousRecognitionCompletedEventArgs>| {
                let status = args
                    .as_ref()
                    .and_then(|x| x.Status().ok())
                    .unwrap_or(SpeechRecognitionResultStatus::Unknown);
                let code = match status {
                    SpeechRecognitionResultStatus::MicrophoneUnavailable => Some("speech_mic"),
                    SpeechRecognitionResultStatus::NetworkFailure => Some("network"),
                    SpeechRecognitionResultStatus::TopicLanguageNotSupported => {
                        Some("speech_language")
                    }
                    SpeechRecognitionResultStatus::AudioQualityFailure => Some("speech_audio"),
                    SpeechRecognitionResultStatus::Unknown => Some("speech_failed"),
                    // Success, UserCanceled, silence/pause timeouts: a normal end.
                    _ => None,
                };
                // After stop() or a newer start(), someone else already reported the end.
                if !super::forget_native(id) {
                    return Ok(());
                }
                if let Some(code) = code {
                    log::warn!("native dictation ended with status {}", status.0);
                    emit_error(&a, &CommandError::new(code, format!("status {}", status.0)));
                }
                emit_state(&a, "idle");
                Ok(())
            },
        ))
        .map_err(win_err)?;

    session
        .StartAsync()
        .and_then(|op| op.join())
        .map_err(win_err)?;
    Ok(Dictation {
        id,
        _recognizer: recognizer,
        session,
    })
}

impl Dictation {
    /// Stop listening; the last phrase is still delivered as "speech:final".
    pub fn stop(&self) {
        ensure_com();
        if let Err(e) = self.session.StopAsync().and_then(|op| op.join()) {
            log::warn!("stopping dictation failed: {e}");
        }
    }

    /// Stop and drop whatever was still being recognised.
    pub fn cancel(&self) {
        ensure_com();
        let _ = self.session.CancelAsync().and_then(|op| op.join());
    }
}
