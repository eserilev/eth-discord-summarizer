use anyhow::Result;
use chrono::NaiveDate;
use std::path::Path;

use crate::clustering::TopicCluster;
use crate::summary::TopicSummary;

/// Write the summary.md and messages.log files for a channel.
///
/// Output structure:
///   {output_dir}/{date}/{channel_name}/summary.md
///   {output_dir}/{date}/{channel_name}/messages.log
pub fn write_output(
    output_dir: &Path,
    channel_name: &str,
    date: NaiveDate,
    clusters: &[TopicCluster],
    summaries: &[TopicSummary],
) -> Result<()> {
    let date_str = date.format("%Y-%m-%d").to_string();
    let dir = output_dir.join(&date_str).join(channel_name);
    std::fs::create_dir_all(&dir)?;

    // Write summary.md
    let summary_path = dir.join("summary.md");
    let summary_content = format_summary_md(channel_name, &date_str, summaries);
    std::fs::write(&summary_path, summary_content)?;

    // Write messages.log
    let log_path = dir.join("messages.log");
    let log_content = format_messages_log(clusters);
    std::fs::write(&log_path, log_content)?;

    tracing::info!("Wrote output to {}", dir.display());
    Ok(())
}

/// Format the summary.md content
fn format_summary_md(channel_name: &str, date: &str, summaries: &[TopicSummary]) -> String {
    let mut out = format!("# {} — {}\n\n", channel_name, date);

    for summary in summaries {
        out.push_str(&format!("## Topic: {}\n\n", summary.title));
        out.push_str(&summary.summary);
        out.push_str("\n\n### Participants\n");
        out.push_str(&format!("- {}\n", summary.participants.join(", ")));
        out.push_str("\n---\n\n");
    }

    out
}

/// Format the messages.log content
fn format_messages_log(clusters: &[TopicCluster]) -> String {
    let mut out = String::new();

    for cluster in clusters {
        out.push_str(&format!("=== Topic: {} ===\n\n", cluster.title));
        for msg in &cluster.messages {
            // Parse the ISO timestamp to a friendlier format
            let ts = &msg.timestamp;
            out.push_str(&format!("[{}] {}: {}\n", ts, msg.author.username, msg.content));
        }
        out.push('\n');
    }

    out
}
