//! Google Gemini (Generative Language API).

use super::{AiError, Completion, ModelInfo, estimate_tokens, read_error};
use crate::actions::wrap_input;
use serde_json::{Value, json};

const BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

pub fn request_body(system: &str, text: &str) -> Value {
    json!({
        "systemInstruction": { "parts": [{ "text": system }] },
        "contents": [{ "role": "user", "parts": [{ "text": wrap_input(text) }] }]
    })
}

pub fn parse_response(v: &Value, input_text: &str) -> Result<Completion, AiError> {
    let parts = v
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            let reason = v
                .pointer("/promptFeedback/blockReason")
                .or_else(|| v.pointer("/candidates/0/finishReason"))
                .and_then(Value::as_str)
                .unwrap_or("no candidates");
            AiError::Parse {
                message: reason.to_string(),
            }
        })?;
    // Skip "thought" parts emitted by thinking models.
    let text: String = parts
        .iter()
        .filter(|p| !p.get("thought").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .collect();
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AiError::Empty);
    }
    let input = v
        .pointer("/usageMetadata/promptTokenCount")
        .and_then(Value::as_u64);
    let output = v
        .pointer("/usageMetadata/candidatesTokenCount")
        .and_then(Value::as_u64);
    Ok(Completion {
        estimated_tokens: input.is_none() || output.is_none(),
        input_tokens: input.unwrap_or_else(|| estimate_tokens(input_text)),
        output_tokens: output.unwrap_or_else(|| estimate_tokens(&text)),
        text,
    })
}

pub async fn complete(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    text: &str,
) -> Result<Completion, AiError> {
    let resp = client
        .post(format!("{BASE}/models/{model}:generateContent"))
        // Key in a header, not in the query string (keeps it out of proxy/access logs).
        .header("x-goog-api-key", api_key)
        .json(&request_body(system, text))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    let v: Value = resp.json().await?;
    parse_response(&v, text)
}

/// Same filter as the extension: text-capable Gemini ≥ 3, no TTS/image/embedding/live
/// variants; flash models first, then by id descending.
pub fn filter_models(v: &Value) -> Vec<ModelInfo> {
    const EXCLUDED: [&str; 16] = [
        "tts",
        "speech",
        "text-to-speech",
        "audio",
        "voice",
        "sound",
        "banana",
        "image",
        "imagen",
        "video",
        "live",
        "realtime",
        "embedding",
        "aqa",
        "diffusion",
        "robotics",
    ];
    let mut out: Vec<ModelInfo> = v
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|m| {
            m.get("supportedGenerationMethods")
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().any(|x| x == "generateContent"))
        })
        .filter_map(|m| {
            let name = m.get("name")?.as_str()?;
            let display = m.get("displayName").and_then(Value::as_str).unwrap_or(name);
            let desc = m.get("description").and_then(Value::as_str).unwrap_or("");
            let combined = format!("{name} {display} {desc}").to_lowercase();
            if EXCLUDED.iter().any(|k| combined.contains(k)) || !combined.contains("gemini") {
                return None;
            }
            let version = gemini_version(&combined)?;
            if version < 3.0 {
                return None;
            }
            let id = name.trim_start_matches("models/").to_string();
            Some(ModelInfo {
                name: display.to_string(),
                id,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        let fa = a.id.contains("flash");
        let fb = b.id.contains("flash");
        fb.cmp(&fa).then_with(|| natural_cmp(&b.id, &a.id))
    });
    out.dedup_by(|a, b| a.id == b.id);
    out
}

fn gemini_version(s: &str) -> Option<f64> {
    let idx = s.find("gemini")? + "gemini".len();
    let rest = s[idx..].trim_start_matches(['-', '/']);
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    num.trim_end_matches('.').parse().ok()
}

/// Compare strings treating digit runs as numbers ("3.10" > "3.9").
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |s: &str| -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let d = c.is_ascii_digit();
            match out.last_mut() {
                Some((pd, buf)) if *pd == d => buf.push(c),
                _ => out.push((d, c.to_string())),
            }
        }
        out
    };
    let (xa, xb) = (split(a), split(b));
    for (pa, pb) in xa.iter().zip(xb.iter()) {
        let ord = if pa.0 && pb.0 {
            pa.1.parse::<u64>()
                .unwrap_or(0)
                .cmp(&pb.1.parse::<u64>().unwrap_or(0))
        } else {
            pa.1.cmp(&pb.1)
        };
        if ord.is_ne() {
            return ord;
        }
    }
    xa.len().cmp(&xb.len())
}

pub async fn list_models(
    client: &reqwest::Client,
    api_key: &str,
) -> Result<Vec<ModelInfo>, AiError> {
    let resp = client
        .get(format!("{BASE}/models?pageSize=100"))
        .header("x-goog-api-key", api_key)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    let v: Value = resp.json().await?;
    Ok(filter_models(&v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_body_with_wrapped_input() {
        let b = request_body("SYS", "hello");
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "SYS");
        assert_eq!(
            b["contents"][0]["parts"][0]["text"],
            "<input_text>\nhello\n</input_text>"
        );
    }

    #[test]
    fn parses_text_and_usage_skipping_thoughts() {
        let v = json!({
            "candidates": [{ "content": { "parts": [
                { "text": "thinking...", "thought": true },
                { "text": " Hello world " }
            ]}}],
            "usageMetadata": { "promptTokenCount": 12, "candidatesTokenCount": 3 }
        });
        let c = parse_response(&v, "hi").unwrap();
        assert_eq!(c.text, "Hello world");
        assert_eq!(
            (c.input_tokens, c.output_tokens, c.estimated_tokens),
            (12, 3, false)
        );
    }

    #[test]
    fn estimates_when_usage_missing_and_reports_blocks() {
        let v = json!({ "candidates": [{ "content": { "parts": [{ "text": "abcdefgh" }] } }] });
        let c = parse_response(&v, "abcd").unwrap();
        assert!(c.estimated_tokens);
        assert_eq!((c.input_tokens, c.output_tokens), (1, 2));

        let blocked = json!({ "promptFeedback": { "blockReason": "SAFETY" } });
        assert_eq!(
            parse_response(&blocked, "x"),
            Err(AiError::Parse {
                message: "SAFETY".into()
            })
        );
        let empty = json!({ "candidates": [{ "content": { "parts": [{ "text": "  " }] } }] });
        assert_eq!(parse_response(&empty, "x"), Err(AiError::Empty));
    }

    #[test]
    fn filters_and_sorts_models() {
        let methods = json!(["generateContent"]);
        let v = json!({ "models": [
            { "name": "models/gemini-2.5-flash", "supportedGenerationMethods": methods },
            { "name": "models/gemini-3.5-pro", "supportedGenerationMethods": methods },
            { "name": "models/gemini-3.7-flash", "displayName": "Gemini 3.7 Flash", "supportedGenerationMethods": methods },
            { "name": "models/gemini-3.5-flash", "supportedGenerationMethods": methods },
            { "name": "models/gemini-3.5-flash-tts", "supportedGenerationMethods": methods },
            { "name": "models/text-embedding-004", "supportedGenerationMethods": ["embedContent"] }
        ]});
        let ids: Vec<String> = filter_models(&v).into_iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            ["gemini-3.7-flash", "gemini-3.5-flash", "gemini-3.5-pro"]
        );
    }
}
