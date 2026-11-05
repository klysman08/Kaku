//! AI providers. All network calls happen in Rust so the API key never reaches the webview.

pub mod gemini;
pub mod openai;

use crate::settings::{ProviderKind, Settings};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// True when the provider returned no usage data and tokens were estimated (chars / 4).
    pub estimated_tokens: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, thiserror::Error, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AiError {
    #[error("API key is missing")]
    MissingKey,
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("network error: {message}")]
    Network { message: String },
    #[error("request timed out")]
    Timeout,
    #[error("the model returned an empty response")]
    Empty,
    #[error("unexpected response: {message}")]
    Parse { message: String },
}

impl AiError {
    /// Short machine-friendly code stored in the usage table.
    pub fn code(&self) -> String {
        match self {
            AiError::MissingKey => "missing_key".into(),
            AiError::Http { status, .. } => status.to_string(),
            AiError::Network { .. } => "network".into(),
            AiError::Timeout => "timeout".into(),
            AiError::Empty => "empty".into(),
            AiError::Parse { .. } => "parse".into(),
        }
    }

    fn retryable(&self) -> bool {
        match self {
            AiError::Http { status, .. } => *status == 429 || *status >= 500,
            AiError::Network { .. } => true,
            // A timeout already cost the user the full timeout; retrying would triple the wait.
            _ => false,
        }
    }
}

impl From<reqwest::Error> for AiError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            AiError::Timeout
        } else {
            AiError::Network {
                message: e.without_url().to_string(),
            }
        }
    }
}

pub fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

pub fn http_client(settings: &Settings) -> Result<reqwest::Client, AiError> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(settings.request_timeout_secs))
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("Kaku/", env!("CARGO_PKG_VERSION")));
    if !settings.proxy_url.is_empty() {
        let proxy = reqwest::Proxy::all(&settings.proxy_url).map_err(|e| AiError::Network {
            message: format!("invalid proxy URL: {e}"),
        })?;
        builder = builder.proxy(proxy);
    }
    builder.build().map_err(|e| AiError::Network {
        message: e.to_string(),
    })
}

/// Run one completion with up to two retries (429 / 5xx / network) and backoff.
pub async fn complete(
    settings: &Settings,
    api_key: Option<&str>,
    system: &str,
    user_text: &str,
) -> Result<Completion, AiError> {
    let client = http_client(settings)?;
    with_retry(|| async {
        match settings.provider {
            ProviderKind::Gemini => {
                let key = api_key.ok_or(AiError::MissingKey)?;
                gemini::complete(&client, key, &settings.gemini_model, system, user_text).await
            }
            ProviderKind::Openai => {
                openai::complete(
                    &client,
                    &settings.openai_base_url,
                    api_key,
                    &settings.openai_model,
                    system,
                    user_text,
                )
                .await
            }
        }
    })
    .await
}

/// Instructions for dictation. `language` is a BCP-47 tag or "" (auto-detect).
pub fn transcription_prompt(language: &str) -> String {
    let lang = if language.is_empty() {
        "in the language spoken".to_string()
    } else {
        format!("in the language spoken (most likely {language})")
    };
    format!(
        "You transcribe dictated audio. Write exactly what the speaker says, {lang}, with natural punctuation and capitalization. Drop filler sounds and false starts. Do not translate, summarize, answer, or comment on the content. If there is no intelligible speech, output nothing.\n{}",
        crate::actions::NO_EM_DASH_RULE
    )
}

/// Transcribe a 16 kHz mono WAV with the configured provider.
pub async fn transcribe(
    settings: &Settings,
    api_key: Option<&str>,
    wav: &[u8],
    language: &str,
) -> Result<Completion, AiError> {
    use base64::Engine as _;
    let client = http_client(settings)?;
    let system = transcription_prompt(language);
    match settings.provider {
        ProviderKind::Gemini => {
            let key = api_key.ok_or(AiError::MissingKey)?;
            let data = base64::engine::general_purpose::STANDARD.encode(wav);
            with_retry(|| gemini::transcribe(&client, key, &settings.gemini_model, &system, &data))
                .await
        }
        ProviderKind::Openai => {
            // The transcription endpoint wants ISO-639-1 ("pt"), not "pt-BR".
            let iso = language.split('-').next().filter(|l| l.len() == 2);
            with_retry(|| {
                openai::transcribe(
                    &client,
                    &settings.openai_base_url,
                    api_key,
                    &settings.openai_transcription_model,
                    "Transcribe with punctuation.",
                    iso,
                    wav.to_vec(),
                )
            })
            .await
        }
    }
}

/// Up to two retries (429 / 5xx / network) with backoff.
async fn with_retry<F, Fut, T>(mut call: F) -> Result<T, AiError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, AiError>>,
{
    let mut attempt = 0;
    loop {
        let result = call().await;
        match result {
            Err(e) if e.retryable() && attempt < 2 => {
                attempt += 1;
                let wait = 600 * 2u64.pow(attempt);
                log::warn!(
                    "AI request failed ({}), retry {attempt} in {wait}ms",
                    e.code()
                );
                tokio::time::sleep(Duration::from_millis(wait)).await;
            }
            other => return other,
        }
    }
}

pub async fn list_models(
    settings: &Settings,
    api_key: Option<&str>,
) -> Result<Vec<ModelInfo>, AiError> {
    let client = http_client(settings)?;
    match settings.provider {
        ProviderKind::Gemini => {
            gemini::list_models(&client, api_key.ok_or(AiError::MissingKey)?).await
        }
        ProviderKind::Openai => {
            openai::list_models(&client, &settings.openai_base_url, api_key).await
        }
    }
}

/// Pull a readable message out of a provider error body (`{"error":{"message":..}}`).
pub(crate) fn error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("error"))
                .and_then(|m| m.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| body.chars().take(300).collect())
}

pub(crate) async fn read_error(resp: reqwest::Response) -> AiError {
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    AiError::Http {
        status,
        message: error_message(&body),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcription_prompt_mentions_language_and_rules() {
        let p = transcription_prompt("pt-BR");
        assert!(p.contains("pt-BR") && p.contains("em dash"));
        assert!(transcription_prompt("").contains("in the language spoken,"));
    }

    #[test]
    fn estimates_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn extracts_error_messages() {
        assert_eq!(
            error_message(r#"{"error":{"code":400,"message":"API key not valid"}}"#),
            "API key not valid"
        );
        assert_eq!(
            error_message(r#"{"error":"model not found"}"#),
            "model not found"
        );
        assert_eq!(error_message("Bad Gateway"), "Bad Gateway");
    }

    #[test]
    fn retry_policy() {
        assert!(
            AiError::Http {
                status: 429,
                message: String::new()
            }
            .retryable()
        );
        assert!(
            AiError::Http {
                status: 503,
                message: String::new()
            }
            .retryable()
        );
        assert!(
            !AiError::Http {
                status: 400,
                message: String::new()
            }
            .retryable()
        );
        assert!(!AiError::MissingKey.retryable());
        assert!(!AiError::Timeout.retryable());
    }
}
