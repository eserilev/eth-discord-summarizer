use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::config::LlmConfig;
use crate::discord::DiscordMessage;

/// A cluster of messages belonging to a single discussion topic
#[derive(Debug, Clone)]
pub struct TopicCluster {
    /// LLM-inferred topic title
    pub title: String,
    /// Messages belonging to this topic, in chronological order
    pub messages: Vec<DiscordMessage>,
    /// Unique participants in this topic
    pub participants: Vec<String>,
}

/// LLM response schema for topic clustering
#[derive(Debug, Deserialize)]
struct ClusteringResponse {
    topics: Vec<TopicAssignment>,
}

#[derive(Debug, Deserialize)]
struct TopicAssignment {
    title: String,
    /// Message indices (0-based) belonging to this topic
    message_indices: Vec<usize>,
}

/// Anthropic API request/response types
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

/// Maximum messages to send in a single clustering request.
/// Beyond this, we chunk and cluster in batches.
const MAX_MESSAGES_PER_BATCH: usize = 200;

/// Cluster a list of messages into distinct discussion topics.
///
/// Uses the configured LLM to analyze message content, reply chains,
/// and temporal proximity to group messages into coherent topics.
pub async fn cluster_messages(
    config: &LlmConfig,
    messages: &[DiscordMessage],
) -> Result<Vec<TopicCluster>> {
    if messages.is_empty() {
        return Ok(vec![]);
    }

    info!(message_count = messages.len(), "Clustering messages into topics");

    // For small batches, cluster all at once. For large ones, chunk.
    let assignments = if messages.len() <= MAX_MESSAGES_PER_BATCH {
        call_clustering_llm(config, messages).await?
    } else {
        // Chunk messages and cluster each batch, then merge
        let mut all_assignments = Vec::new();
        let mut offset = 0;

        for chunk in messages.chunks(MAX_MESSAGES_PER_BATCH) {
            let mut batch_assignments = call_clustering_llm(config, chunk).await?;
            // Adjust indices to account for offset
            for topic in &mut batch_assignments {
                for idx in &mut topic.message_indices {
                    *idx += offset;
                }
            }
            all_assignments.extend(batch_assignments);
            offset += chunk.len();
        }

        // TODO: merge topics with similar titles across batches
        all_assignments
    };

    // Build TopicCluster structs from assignments
    let clusters = build_clusters(messages, &assignments);

    info!(topic_count = clusters.len(), "Clustering complete");
    Ok(clusters)
}

/// Call the LLM to cluster a batch of messages into topics.
async fn call_clustering_llm(
    config: &LlmConfig,
    messages: &[DiscordMessage],
) -> Result<Vec<TopicAssignment>> {
    let client = Client::new();

    let formatted_messages = format_messages_for_prompt(messages);
    let prompt = build_clustering_prompt(&formatted_messages);

    debug!(
        message_count = messages.len(),
        prompt_len = prompt.len(),
        "Sending clustering request to LLM"
    );

    let request = AnthropicRequest {
        model: config.model.clone(),
        max_tokens: 4096,
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
        .context("Failed to call LLM API")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("LLM API returned {}: {}", status, body);
    }

    let api_response: AnthropicResponse = response.json().await
        .context("Failed to parse LLM response")?;

    let text = api_response
        .content
        .into_iter()
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("");

    parse_clustering_response(&text, messages.len())
}

/// Format messages into a numbered list for the LLM prompt.
fn format_messages_for_prompt(messages: &[DiscordMessage]) -> String {
    let mut output = String::new();

    for (i, msg) in messages.iter().enumerate() {
        let author = msg.author.global_name.as_deref()
            .unwrap_or(&msg.author.username);

        let reply_info = if let Some(ref reference) = msg.message_reference {
            if let Some(ref msg_id) = reference.message_id {
                // Find the index of the referenced message
                let ref_idx = messages.iter().position(|m| m.id == *msg_id);
                match ref_idx {
                    Some(idx) => format!(" [replying to #{}]", idx),
                    None => " [replying to earlier message]".to_string(),
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        output.push_str(&format!(
            "[{}] {} ({}){}: {}\n",
            i, msg.timestamp, author, reply_info, msg.content
        ));
    }

    output
}

/// Build the clustering prompt.
fn build_clustering_prompt(formatted_messages: &str) -> String {
    format!(
        r#"You are analyzing messages from an Ethereum R&D Discord channel. Your task is to identify distinct discussion topics and group messages by topic.

Here are the messages (numbered for reference):

{}

Analyze these messages and group them into distinct discussion topics. Consider:
- Reply chains (messages replying to each other belong together)
- Temporal proximity (messages close in time about the same subject)
- Semantic similarity (messages discussing the same technical topic)
- A single message can only belong to ONE topic

Respond with ONLY a JSON object in this exact format (no markdown, no explanation):
{{
  "topics": [
    {{
      "title": "Short descriptive title for the topic",
      "message_indices": [0, 1, 5, 8]
    }},
    {{
      "title": "Another topic title",
      "message_indices": [2, 3, 4, 6, 7]
    }}
  ]
}}

Rules:
- Every message index must appear in exactly one topic
- Topic titles should be concise but descriptive (max 10 words)
- If there are isolated messages that don't fit any topic, group them as "Miscellaneous"
- Order topics by the earliest message in each group"#,
        formatted_messages
    )
}

/// Parse the LLM's JSON response into TopicAssignments.
fn parse_clustering_response(
    text: &str,
    total_messages: usize,
) -> Result<Vec<TopicAssignment>> {
    // Try to extract JSON from the response (handle potential markdown wrapping)
    let json_str = extract_json(text);

    let response: ClusteringResponse = serde_json::from_str(json_str)
        .context("Failed to parse clustering JSON from LLM response")?;

    // Validate: check all indices are in range
    for topic in &response.topics {
        for &idx in &topic.message_indices {
            if idx >= total_messages {
                anyhow::bail!(
                    "LLM returned invalid message index {} (max {})",
                    idx,
                    total_messages - 1
                );
            }
        }
    }

    debug!(
        topics = response.topics.len(),
        "Parsed clustering response"
    );

    Ok(response.topics)
}

/// Extract JSON from text that might be wrapped in markdown code fences.
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();

    // Strip ```json ... ``` wrapper if present
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return &trimmed[start..=end];
        }
    }

    trimmed
}

/// Build TopicCluster structs from the LLM's topic assignments.
fn build_clusters(
    messages: &[DiscordMessage],
    assignments: &[TopicAssignment],
) -> Vec<TopicCluster> {
    let mut clusters = Vec::new();

    for assignment in assignments {
        let mut topic_messages: Vec<DiscordMessage> = assignment
            .message_indices
            .iter()
            .filter_map(|&idx| messages.get(idx).cloned())
            .collect();

        // Ensure chronological order
        topic_messages.sort_by(|a, b| a.id.cmp(&b.id));

        // Extract unique participants
        let mut participants: Vec<String> = topic_messages
            .iter()
            .map(|m| {
                m.author.global_name.clone()
                    .unwrap_or_else(|| m.author.username.clone())
            })
            .collect();
        participants.sort();
        participants.dedup();

        clusters.push(TopicCluster {
            title: assignment.title.clone(),
            messages: topic_messages,
            participants,
        });
    }

    clusters
}
