use anyhow::Result;

use crate::clustering::TopicCluster;
use crate::config::LlmConfig;

/// A summary for a single discussion topic
#[derive(Debug, Clone)]
pub struct TopicSummary {
    /// Topic title
    pub title: String,
    /// Generated summary paragraphs
    pub summary: String,
    /// Participants involved
    pub participants: Vec<String>,
}

/// Generate summaries for each topic cluster using the LLM.
///
/// For each cluster, sends the messages to the LLM and asks for a concise
/// summary that captures key points, decisions, and conclusions.
pub async fn summarize_topics(
    _config: &LlmConfig,
    _clusters: &[TopicCluster],
) -> Result<Vec<TopicSummary>> {
    // TODO: Implement LLM-assisted summary generation
    // - For each TopicCluster, build a prompt with the messages
    // - Ask LLM to produce a concise summary capturing:
    //   - Key points discussed
    //   - Any decisions or conclusions reached
    //   - Open questions or action items
    // - Parse and return structured summaries

    tracing::warn!("summarize_topics not yet implemented");
    Ok(vec![])
}
