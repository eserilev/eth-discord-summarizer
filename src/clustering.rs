use anyhow::Result;

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

/// Cluster a list of messages into distinct discussion topics.
///
/// Uses the configured LLM to analyze message content, reply chains,
/// and temporal proximity to group messages into coherent topics.
///
/// Approach:
/// 1. Pre-process messages: extract reply chains, note time gaps
/// 2. Send messages (or batches) to LLM with a clustering prompt
/// 3. LLM returns topic assignments for each message
/// 4. Group messages by assigned topic
/// 5. Ask LLM to generate a short title for each cluster
pub async fn cluster_messages(
    _config: &LlmConfig,
    _messages: &[DiscordMessage],
) -> Result<Vec<TopicCluster>> {
    // TODO: Implement LLM-assisted topic clustering
    // - Build a prompt with message content, timestamps, and reply references
    // - Call LLM API to get topic groupings
    // - Parse response and construct TopicCluster structs
    // - Handle edge cases: single-message topics, very long conversations

    tracing::warn!("cluster_messages not yet implemented");
    Ok(vec![])
}
