use anyhow::Result;
use chrono::NaiveDate;
use std::path::Path;

use crate::clustering::TopicCluster;
use crate::summary::TopicSummary;
use crate::topic_registry::TopicMatch;

/// Write Obsidian-style output files for a channel's daily topics.
///
/// Output structure:
///   {output_dir}/{channel_name}/{YYYY}/{MM}/{DD}/_index.md
///   {output_dir}/{channel_name}/{YYYY}/{MM}/{DD}/{slug}.md
///
/// Each topic file contains the summary, participants, and raw messages.
/// The _index.md links to all topic files for the day.
pub fn write_output(
    output_dir: &Path,
    channel_name: &str,
    date: NaiveDate,
    clusters: &[TopicCluster],
    summaries: &[TopicSummary],
    matches: &[TopicMatch],
) -> Result<()> {
    let date_parts = (
        date.format("%Y").to_string(),
        date.format("%m").to_string(),
        date.format("%d").to_string(),
    );

    let dir = output_dir
        .join(channel_name)
        .join(&date_parts.0)
        .join(&date_parts.1)
        .join(&date_parts.2);
    std::fs::create_dir_all(&dir)?;

    // Write individual topic files
    for (i, summary) in summaries.iter().enumerate() {
        let slug = if let Some(m) = matches.get(i) {
            &m.slug
        } else {
            // Fallback: use slugified title
            &crate::topic_registry::slugify(&summary.title)
        };

        let topic_path = dir.join(format!("{}.md", slug));
        let cluster = clusters.get(i);
        let content = format_topic_file(channel_name, date, summary, cluster);
        std::fs::write(&topic_path, content)?;
    }

    // Write _index.md
    let index_path = dir.join("_index.md");
    let index_content = format_index_md(channel_name, date, summaries, matches);
    std::fs::write(&index_path, index_content)?;

    tracing::info!("Wrote output to {}", dir.display());
    Ok(())
}

/// Format an individual topic file with summary, participants, and raw messages.
fn format_topic_file(
    channel_name: &str,
    date: NaiveDate,
    summary: &TopicSummary,
    cluster: Option<&TopicCluster>,
) -> String {
    let date_str = date.format("%Y-%m-%d").to_string();
    let mut out = format!("# {}\n\n", summary.title);
    out.push_str(&format!("**Channel:** {} | **Date:** {}\n\n", channel_name, date_str));

    // Summary
    out.push_str("## Summary\n\n");
    out.push_str(&summary.summary);
    out.push_str("\n\n");

    // Participants
    out.push_str("## Participants\n\n");
    for participant in &summary.participants {
        out.push_str(&format!("- {}\n", participant));
    }
    out.push('\n');

    // Raw messages
    if let Some(cluster) = cluster {
        out.push_str("## Raw Messages\n\n");
        out.push_str("<details>\n<summary>Show raw messages</summary>\n\n");
        out.push_str("```\n");
        for msg in &cluster.messages {
            let author = msg
                .author
                .global_name
                .as_deref()
                .unwrap_or(&msg.author.username);
            out.push_str(&format!("[{}] {}: {}\n", msg.timestamp, author, msg.content));
        }
        out.push_str("```\n\n");
        out.push_str("</details>\n");
    }

    out
}

/// Format the _index.md file that links to all topics for the day.
fn format_index_md(
    channel_name: &str,
    date: NaiveDate,
    summaries: &[TopicSummary],
    matches: &[TopicMatch],
) -> String {
    let date_str = date.format("%Y-%m-%d").to_string();
    let mut out = format!("# {} — {}\n\n", channel_name, date_str);
    out.push_str("## Topics\n\n");

    for (i, summary) in summaries.iter().enumerate() {
        let slug = if let Some(m) = matches.get(i) {
            &m.slug
        } else {
            &crate::topic_registry::slugify(&summary.title)
        };

        out.push_str(&format!("- [[{}]] — {}\n", slug, summary.title));
    }

    out
}
