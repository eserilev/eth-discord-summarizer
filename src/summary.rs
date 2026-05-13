use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::clustering::TopicCluster;
use crate::config::LlmConfig;
use crate::discord::DiscordMessage;

/// A summary for a single discussion topic
#[derive(Debug, Clone)]
pub struct TopicSummary {
    /// Topic title
    pub title: String,
    /// Generated markdown summary
    pub summary: String,
    /// Participants involved
    pub participants: Vec<String>,
    /// Original messages (kept for log output)
    pub messages: Vec<DiscordMessage>,
}

/// Anthropic API types (shared with clustering, but kept local for simplicity)
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

/// Generate summaries for each topic cluster using the LLM.
///
/// For each cluster, sends the messages to the LLM and asks for a concise
/// summary capturing key points, decisions, and conclusions.
pub async fn summarize_topics(
    config: &LlmConfig,
    clusters: &[TopicCluster],
) -> Result<Vec<TopicSummary>> {
    if clusters.is_empty() {
        return Ok(vec![]);
    }

    info!(topic_count = clusters.len(), "Generating summaries");

    let client = Client::new();
    let mut summaries = Vec::with_capacity(clusters.len());

    for cluster in clusters {
        let summary = summarize_single_topic(config, &client, cluster).await?;
        summaries.push(summary);
    }

    info!(
        summaries = summaries.len(),
        "Summary generation complete"
    );

    Ok(summaries)
}

/// Generate a summary for a single topic cluster.
async fn summarize_single_topic(
    config: &LlmConfig,
    client: &Client,
    cluster: &TopicCluster,
) -> Result<TopicSummary> {
    debug!(
        topic = %cluster.title,
        message_count = cluster.messages.len(),
        "Summarizing topic"
    );

    let formatted = format_messages_for_summary(&cluster.messages);
    let prompt = build_summary_prompt(&cluster.title, &formatted);

    let request = AnthropicRequest {
        model: config.model.clone(),
        max_tokens: 2048,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    let response = client
        .post(format!("{}/v1/messages", config.base_url))
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .context("Failed to call LLM API for summary")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("LLM API returned {} during summarization: {}", status, body);
    }

    let api_response: AnthropicResponse = response.json().await
        .context("Failed to parse LLM summary response")?;

    let summary_text = api_response
        .content
        .into_iter()
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("");

    Ok(TopicSummary {
        title: cluster.title.clone(),
        summary: summary_text.trim().to_string(),
        participants: cluster.participants.clone(),
        messages: cluster.messages.clone(),
    })
}

/// Format messages for the summary prompt.
fn format_messages_for_summary(messages: &[DiscordMessage]) -> String {
    let mut output = String::new();

    for msg in messages {
        let author = msg.author.global_name.as_deref()
            .unwrap_or(&msg.author.username);

        let reply_marker = if msg.message_reference.is_some() {
            " (reply)"
        } else {
            ""
        };

        output.push_str(&format!(
            "[{}] {}{}: {}\n",
            msg.timestamp, author, reply_marker, msg.content
        ));
    }

    output
}

/// Build the summary prompt for a single topic.
fn build_summary_prompt(title: &str, formatted_messages: &str) -> String {
    format!(
        r#"You are summarizing a discussion from an Ethereum R&D Discord channel.

Topic: "{title}"

Here are the messages in this discussion:

{formatted_messages}

Write a concise summary of this discussion in Markdown. Your summary should:
- Capture the key technical points discussed
- Note any decisions, conclusions, or consensus reached
- Mention any open questions or action items
- Be concise but preserve important technical detail
- Use 1-3 short paragraphs (not bullet points unless listing specific items)
- Do NOT include a title/heading (that's handled separately)
- Do NOT hallucinate details that aren't in the messages

Write the summary now:"#
    )
}
