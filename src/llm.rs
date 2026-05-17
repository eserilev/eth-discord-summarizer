//! Unified LLM client that supports multiple API backends.
//!
//! Backends:
//! - `anthropic`: Anthropic Messages API (api.anthropic.com)
//! - `bedrock-converse`: AWS Bedrock Converse API (bedrock-runtime.*.amazonaws.com)

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::config::LlmConfig;

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_secs(5);

/// Make a simple text-in, text-out LLM call using the configured backend.
pub async fn complete(config: &LlmConfig, prompt: &str, max_tokens: u32) -> Result<String> {
    for attempt in 0..MAX_RETRIES {
        let result = match config.api_type.as_str() {
            "bedrock-converse" => bedrock_converse(config, prompt, max_tokens).await,
            _ => anthropic_messages(config, prompt, max_tokens).await,
        };

        match result {
            Ok(text) => return Ok(text),
            Err(e) => {
                let err_str = format!("{}", e);
                // Retry on 500s and throttling
                if (err_str.contains("500") || err_str.contains("503") || err_str.contains("429") || err_str.contains("throttl"))
                    && attempt < MAX_RETRIES - 1
                {
                    let delay = RETRY_DELAY * (attempt + 1);
                    warn!(
                        attempt = attempt + 1,
                        "LLM call failed ({}), retrying in {:?}",
                        err_str.chars().take(100).collect::<String>(),
                        delay
                    );
                    sleep(delay).await;
                    continue;
                }
                return Err(e);
            }
        }
    }
    unreachable!()
}

// --- Anthropic Messages API ---

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

async fn anthropic_messages(config: &LlmConfig, prompt: &str, max_tokens: u32) -> Result<String> {
    let client = Client::new();

    let request = AnthropicRequest {
        model: config.model.clone(),
        max_tokens,
        messages: vec![Message {
            role: "user".to_string(),
            content: serde_json::Value::String(prompt.to_string()),
        }],
    };

    let mut req_builder = client
        .post(format!("{}/v1/messages", config.base_url))
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01");

    req_builder = req_builder.header("x-api-key", &config.api_key);

    let response = req_builder
        .json(&request)
        .send()
        .await
        .context("Failed to call Anthropic API")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic API returned {}: {}", status, body);
    }

    let api_response: AnthropicResponse = response
        .json()
        .await
        .context("Failed to parse Anthropic response")?;

    let text = api_response
        .content
        .into_iter()
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("");

    debug!(response_len = text.len(), "LLM response received (anthropic)");
    Ok(text)
}

// --- AWS Bedrock Converse API ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConverseRequest {
    messages: Vec<ConverseMessage>,
    inference_config: InferenceConfig,
}

#[derive(Debug, Serialize)]
struct ConverseMessage {
    role: String,
    content: Vec<ConverseContent>,
}

#[derive(Debug, Serialize)]
struct ConverseContent {
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InferenceConfig {
    max_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ConverseResponse {
    output: ConverseOutput,
}

#[derive(Debug, Deserialize)]
struct ConverseOutput {
    message: ConverseOutputMessage,
}

#[derive(Debug, Deserialize)]
struct ConverseOutputMessage {
    content: Vec<ConverseOutputContent>,
}

#[derive(Debug, Deserialize)]
struct ConverseOutputContent {
    text: Option<String>,
}

async fn bedrock_converse(config: &LlmConfig, prompt: &str, max_tokens: u32) -> Result<String> {
    let client = Client::new();

    // base_url should be like "https://bedrock-runtime.us-west-2.amazonaws.com"
    // model should be like "us.anthropic.claude-sonnet-4-20250514-v1:0"
    let url = format!("{}/model/{}/converse", config.base_url, config.model);

    let request = ConverseRequest {
        messages: vec![ConverseMessage {
            role: "user".to_string(),
            content: vec![ConverseContent {
                text: prompt.to_string(),
            }],
        }],
        inference_config: InferenceConfig { max_tokens },
    };

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .with_context(|| format!("Failed to call Bedrock Converse API at {}", url))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Bedrock Converse API returned {}: {}", status, body);
    }

    let api_response: ConverseResponse = response
        .json()
        .await
        .context("Failed to parse Bedrock Converse response")?;

    let text = api_response
        .output
        .message
        .content
        .into_iter()
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("");

    debug!(response_len = text.len(), "LLM response received (bedrock-converse)");
    Ok(text)
}
