//! LLM integration for sentence analysis via OpenAI-compatible API.
//!
//! Supports any OpenAI-compatible API endpoint (OpenAI, OpenRouter, Ollama, LM Studio, etc.).
//! Responses are cached in the `llm_cache` database table to avoid redundant API calls.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::LlmConfig;
use crate::db::models;

/// The system prompt used for sentence analysis, loaded from data/system_prompt.txt at build time.
const SYSTEM_PROMPT: &str = include_str!("../../data/system_prompt.txt");

/// A single component in the sentence breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentBreakdown {
    pub japanese: String,
    pub romaji: String,
    pub meaning: String,
}

/// Full structured analysis of a Japanese sentence from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentenceAnalysis {
    pub translation: String,
    #[serde(default)]
    pub component_breakdown: Vec<ComponentBreakdown>,
    #[serde(default)]
    pub explanation: String,
}

/// Result of an LLM call, including the parsed analysis and metadata.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LlmResponse {
    pub analysis: SentenceAnalysis,
    pub model: String,
    pub tokens_used: i64,
    pub cached: bool,
    /// The raw JSON response text (stored in cache).
    pub raw_json: String,
}

/// Compute a SHA-256 hash of the request for cache lookup.
/// Hash includes the sentence and model to ensure different models get separate cache entries.
fn compute_hash(sentence: &str, model: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"sentence_analysis|");
    hasher.update(model.as_bytes());
    hasher.update(b"|");
    hasher.update(sentence.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Build the user message for sentence analysis.
/// Includes up to 3 previous sentences as context if provided.
fn build_user_message(sentence: &str, context_sentences: &[String]) -> String {
    if context_sentences.is_empty() {
        sentence.to_string()
    } else {
        let mut msg = String::new();
        for ctx in context_sentences {
            msg.push_str(ctx);
            msg.push('\n');
        }
        msg.push_str(sentence);
        msg
    }
}

/// Call the LLM API (OpenAI-compatible chat completions endpoint).
/// This is a blocking call intended to be run in a background thread.
fn call_api(
    config: &LlmConfig,
    sentence: &str,
    context_sentences: &[String],
) -> Result<(String, i64)> {
    if config.api_key.is_empty() {
        anyhow::bail!(
            "LLM API key not configured. Set it in Settings > LLM or in kotoba.toml under [llm]."
        );
    }

    let url = format!("{}/chat/completions", config.endpoint.trim_end_matches('/'));

    let user_message = build_user_message(sentence, context_sentences);

    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            {
                "role": "system",
                "content": SYSTEM_PROMPT
            },
            {
                "role": "user",
                "content": user_message
            }
        ],
        "max_tokens": config.max_tokens,
        "temperature": 0.3,
        "response_format": { "type": "json_object" }
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", config.api_key))
        .json(&body)
        .send()
        .context("Failed to send request to LLM API")?;

    let status = response.status();
    let response_text = response
        .text()
        .context("Failed to read LLM response body")?;

    if !status.is_success() {
        anyhow::bail!("LLM API error ({}): {}", status, response_text);
    }

    let json: serde_json::Value =
        serde_json::from_str(&response_text).context("Failed to parse LLM response JSON")?;

    let content = json["choices"]
        .get(0)
        .and_then(|c| c["message"]["content"].as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let tokens_used = json["usage"]["total_tokens"].as_i64().unwrap_or(0);

    Ok((content, tokens_used))
}

/// Parse the LLM JSON response into a SentenceAnalysis struct.
/// Handles edge cases like markdown code fences around the JSON.
fn parse_analysis(raw: &str) -> Result<SentenceAnalysis> {
    // Strip markdown code fences if present (some models wrap in ```json ... ```)
    let cleaned = raw
        .trim()
        .strip_prefix("```json")
        .or_else(|| raw.trim().strip_prefix("```"))
        .unwrap_or(raw.trim());
    let cleaned = cleaned.strip_suffix("```").unwrap_or(cleaned).trim();

    serde_json::from_str::<SentenceAnalysis>(cleaned)
        .context("Failed to parse LLM response as SentenceAnalysis JSON")
}

/// Analyze a Japanese sentence using the LLM.
/// Checks the cache first; if not cached, calls the API and caches the result.
///
/// `context_sentences` provides up to 3 previous sentences for contextual understanding.
pub fn analyze_sentence(
    config: &LlmConfig,
    conn: &Connection,
    sentence: &str,
    context_sentences: &[String],
) -> Result<LlmResponse> {
    let hash = compute_hash(sentence, &config.model);

    // Check cache first
    if let Some(entry) = models::get_llm_cache_by_hash(conn, &hash)? {
        let analysis = parse_analysis(&entry.response)?;
        return Ok(LlmResponse {
            analysis,
            model: entry.model,
            tokens_used: entry.tokens_used,
            cached: true,
            raw_json: entry.response,
        });
    }

    // Call API
    let (response_text, tokens_used) = call_api(config, sentence, context_sentences)?;

    // Parse the response
    let analysis = parse_analysis(&response_text)?;

    // Cache the result (store the raw JSON so we can re-parse it later)
    models::insert_llm_cache(
        conn,
        "sentence_analysis",
        &hash,
        sentence,
        &response_text,
        &config.model,
        tokens_used,
    )?;

    Ok(LlmResponse {
        analysis,
        model: config.model.clone(),
        tokens_used,
        cached: false,
        raw_json: response_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn test_compute_hash_deterministic() {
        let h1 = compute_hash("こんにちは", "gpt-4o");
        let h2 = compute_hash("こんにちは", "gpt-4o");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_compute_hash_different_for_different_inputs() {
        let h1 = compute_hash("こんにちは", "gpt-4o");
        let h2 = compute_hash("さようなら", "gpt-4o");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_compute_hash_different_for_different_models() {
        let h1 = compute_hash("こんにちは", "gpt-4o");
        let h2 = compute_hash("こんにちは", "gpt-3.5-turbo");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_parse_analysis_basic() {
        let json = r#"{"translation":"Hello","component_breakdown":[{"japanese":"こんにちは","romaji":"konnichiwa","meaning":"hello"}],"explanation":"A common greeting"}"#;
        let analysis = parse_analysis(json).unwrap();
        assert_eq!(analysis.translation, "Hello");
        assert_eq!(analysis.component_breakdown.len(), 1);
        assert_eq!(analysis.component_breakdown[0].japanese, "こんにちは");
        assert_eq!(analysis.explanation, "A common greeting");
    }

    #[test]
    fn test_parse_analysis_with_code_fences() {
        let json = "```json\n{\"translation\":\"Hello\",\"component_breakdown\":[],\"explanation\":\"test\"}\n```";
        let analysis = parse_analysis(json).unwrap();
        assert_eq!(analysis.translation, "Hello");
    }

    #[test]
    fn test_parse_analysis_missing_optional_fields() {
        let json = r#"{"translation":"Hello"}"#;
        let analysis = parse_analysis(json).unwrap();
        assert_eq!(analysis.translation, "Hello");
        assert!(analysis.component_breakdown.is_empty());
        assert_eq!(analysis.explanation, "");
    }

    #[test]
    fn test_build_user_message_no_context() {
        let msg = build_user_message("テスト", &[]);
        assert_eq!(msg, "テスト");
    }

    #[test]
    fn test_build_user_message_with_context() {
        let ctx = vec!["前の文。".to_string(), "もう一つ。".to_string()];
        let msg = build_user_message("テスト", &ctx);
        assert_eq!(msg, "前の文。\nもう一つ。\nテスト");
    }
}
