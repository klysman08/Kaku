//! OpenAI-compatible Chat Completions (OpenAI, Azure/corporate gateways, OpenRouter,
//! LiteLLM, Ollama, LM Studio...). The API key is optional for local servers.

use super::{AiError, Completion, ModelInfo, estimate_tokens, read_error};
use crate::actions::wrap_input;
use serde_json::{Value, json};

pub fn request_body(model: &str, system: &str, text: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": wrap_input(text) }
        ]
    })
}

pub fn parse_response(v: &Value, input_text: &str) -> Result<Completion, AiError> {
    let content = v
        .pointer("/choices/0/message/content")
        .ok_or_else(|| AiError::Parse {
            message: "missing choices[0].message.content".into(),
        })?;
    // Content is usually a string; some gateways return an array of parts.
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect(),
        _ => String::new(),
    };
    let text = strip_think(&text).trim().to_string();
    if text.is_empty() {
        return Err(AiError::Empty);
    }
    let input = v.pointer("/usage/prompt_tokens").and_then(Value::as_u64);
    let output = v
        .pointer("/usage/completion_tokens")
        .and_then(Value::as_u64);
    Ok(Completion {
        estimated_tokens: input.is_none() || output.is_none(),
        input_tokens: input.unwrap_or_else(|| estimate_tokens(input_text)),
        output_tokens: output.unwrap_or_else(|| estimate_tokens(&text)),
        text,
    })
}

/// Local reasoning models (e.g. via Ollama) may prefix `<think>…</think>`.
fn strip_think(s: &str) -> &str {
    let t = s.trim_start();
    if t.starts_with("<think>")
        && let Some(end) = t.find("</think>")
    {
        return &t[end + "</think>".len()..];
    }
    s
}

fn with_auth(req: reqwest::RequestBuilder, api_key: Option<&str>) -> reqwest::RequestBuilder {
    match api_key {
        Some(k) if !k.is_empty() => req.bearer_auth(k),
        _ => req,
    }
}

pub async fn complete(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    system: &str,
    text: &str,
) -> Result<Completion, AiError> {
    let req = client
        .post(format!("{base_url}/chat/completions"))
        .json(&request_body(model, system, text));
    let resp = with_auth(req, api_key).send().await?;
    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    let v: Value = resp.json().await?;
    parse_response(&v, text)
}

/// Dictation via `POST /audio/transcriptions` (OpenAI, Azure, Groq, LocalAI...).
pub async fn transcribe(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    prompt: &str,
    language: Option<&str>,
    wav: Vec<u8>,
) -> Result<Completion, AiError> {
    let file = reqwest::multipart::Part::bytes(wav)
        .file_name("dictation.wav")
        .mime_str("audio/wav")
        .map_err(|e| AiError::Parse {
            message: e.to_string(),
        })?;
    let mut form = reqwest::multipart::Form::new()
        .text("model", model.to_string())
        .text("response_format", "json")
        .text("prompt", prompt.to_string())
        .part("file", file);
    if let Some(lang) = language {
        form = form.text("language", lang.to_string());
    }
    let req = client
        .post(format!("{base_url}/audio/transcriptions"))
        .multipart(form);
    let resp = with_auth(req, api_key).send().await?;
    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    let v: Value = resp.json().await?;
    parse_transcription(&v)
}

pub fn parse_transcription(v: &Value) -> Result<Completion, AiError> {
    let text = v
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| AiError::Parse {
            message: "missing text".into(),
        })?
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(AiError::Empty);
    }
    let input = v.pointer("/usage/input_tokens").and_then(Value::as_u64);
    let output = v.pointer("/usage/output_tokens").and_then(Value::as_u64);
    Ok(Completion {
        estimated_tokens: input.is_none() || output.is_none(),
        input_tokens: input.unwrap_or(0),
        output_tokens: output.unwrap_or_else(|| estimate_tokens(&text)),
        text,
    })
}

pub async fn list_models(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<ModelInfo>, AiError> {
    let resp = with_auth(client.get(format!("{base_url}/models")), api_key)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    let v: Value = resp.json().await?;
    let mut models: Vec<ModelInfo> = v
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("id").and_then(Value::as_str))
        .map(|id| ModelInfo {
            id: id.to_string(),
            name: id.to_string(),
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_and_array_content() {
        let v = json!({
            "choices": [{ "message": { "content": "Olá!" } }],
            "usage": { "prompt_tokens": 20, "completion_tokens": 2 }
        });
        let c = parse_response(&v, "x").unwrap();
        assert_eq!(
            (c.text.as_str(), c.input_tokens, c.output_tokens),
            ("Olá!", 20, 2)
        );

        let arr = json!({ "choices": [{ "message": { "content": [{ "type": "text", "text": "A" }, { "type": "text", "text": "B" }] } }] });
        let c = parse_response(&arr, "x").unwrap();
        assert_eq!(c.text, "AB");
        assert!(c.estimated_tokens);
    }

    #[test]
    fn parses_transcriptions() {
        let v = json!({ "text": " Olá, tudo bem? ", "usage": { "input_tokens": 50, "output_tokens": 6 } });
        let c = parse_transcription(&v).unwrap();
        assert_eq!(
            (c.text.as_str(), c.input_tokens, c.output_tokens),
            ("Olá, tudo bem?", 50, 6)
        );
        assert!(!c.estimated_tokens);
        assert_eq!(
            parse_transcription(&json!({ "text": "  " })),
            Err(AiError::Empty)
        );
        assert!(parse_transcription(&json!({ "error": "x" })).is_err());
    }

    #[test]
    fn strips_think_blocks() {
        let v = json!({ "choices": [{ "message": { "content": "<think>hmm</think>\nFinal" } }] });
        assert_eq!(parse_response(&v, "x").unwrap().text, "Final");
    }

    #[test]
    fn body_has_system_and_wrapped_user() {
        let b = request_body("m", "S", "t");
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(
            b["messages"][1]["content"],
            "<input_text>\nt\n</input_text>"
        );
    }
}
