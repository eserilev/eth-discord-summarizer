use anyhow::Result;
use tracing::{debug, info};

use crate::clustering::TopicCluster;
use crate::config::LlmConfig;
use crate::discord::DiscordMessage;
use crate::llm;

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

/// Generate summaries for each topic cluster using the LLM.
pub async fn summarize_topics(
    config: &LlmConfig,
    clusters: &[TopicCluster],
) -> Result<Vec<TopicSummary>> {
    if clusters.is_empty() {
        return Ok(vec![]);
    }

    info!(topic_count = clusters.len(), "Generating summaries");

    let mut summaries = Vec::with_capacity(clusters.len());

    for cluster in clusters {
        let summary = summarize_single_topic(config, cluster).await?;
        summaries.push(summary);
    }

    info!(summaries = summaries.len(), "Summary generation complete");
    Ok(summaries)
}

/// Generate a summary for a single topic cluster.
async fn summarize_single_topic(
    config: &LlmConfig,
    cluster: &TopicCluster,
) -> Result<TopicSummary> {
    debug!(
        topic = %cluster.title,
        message_count = cluster.messages.len(),
        "Summarizing topic"
    );

    let formatted = format_messages_for_summary(&cluster.messages);
    let prompt = build_summary_prompt(&cluster.title, &formatted);

    let summary_text = llm::complete(config, &prompt, 2048).await?;

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
