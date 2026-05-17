use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::{debug, info};

use crate::config::LlmConfig;
use crate::discord::DiscordMessage;
use crate::llm;

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
    let formatted_messages = format_messages_for_prompt(messages);
    let prompt = build_clustering_prompt(&formatted_messages);

    debug!(
        message_count = messages.len(),
        prompt_len = prompt.len(),
        "Sending clustering request to LLM"
    );

    let text = llm::complete(config, &prompt, 4096).await?;

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
    let json_str = extract_json(text);

    let mut response: ClusteringResponse = serde_json::from_str(json_str)
        .context("Failed to parse clustering JSON from LLM response")?;

    // Filter out any invalid indices (LLM sometimes returns out-of-range)
    for topic in &mut response.topics {
        topic.message_indices.retain(|&idx| idx < total_messages);
    }

    // Remove empty topics after filtering
    response.topics.retain(|t| !t.message_indices.is_empty());

    debug!(topics = response.topics.len(), "Parsed clustering response");
    Ok(response.topics)
}

/// Extract JSON from text that might be wrapped in markdown code fences.
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();
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

        topic_messages.sort_by(|a, b| a.id.cmp(&b.id));

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
