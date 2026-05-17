use anyhow::Result;
use chrono::NaiveDate;
use std::path::Path;

use crate::clustering::TopicCluster;
use crate::summary::TopicSummary;
use crate::topic_registry::TopicMatch;

/// Write output files: per-topic summaries and daily index.
///
/// Output structure:
///   {output_dir}/topics/{slug}.md              ← cumulative topic file (appended per day)
///   {output_dir}/{channel_name}/{YYYY}/{MM}/{DD}/_index.md  ← daily index linking to topics
///
/// Each topic file accumulates dated summary sections over time.
/// The daily _index.md links to the relevant topic files for that day.
pub fn write_output(
    output_dir: &Path,
    channel_name: &str,
    date: NaiveDate,
    clusters: &[TopicCluster],
    summaries: &[TopicSummary],
    matches: &[TopicMatch],
) -> Result<()> {
    // Ensure topics dir exists
    let topics_dir = output_dir.join("topics");
    std::fs::create_dir_all(&topics_dir)?;

    // Append to per-topic files
    for (i, summary) in summaries.iter().enumerate() {
        let slug = if let Some(m) = matches.get(i) {
            &m.slug
        } else {
            &crate::topic_registry::slugify(&summary.title)
        };

        let topic_path = topics_dir.join(format!("{}.md", slug));
        let cluster = clusters.get(i);
        let section = format_topic_section(channel_name, date, summary, cluster);

        // If file doesn't exist, create with header; otherwise append
        if !topic_path.exists() {
            let header = format!("# {}\n\n", summary.title);
            std::fs::write(&topic_path, format!("{}{}", header, section))?;
        } else {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&topic_path)?;
            file.write_all(section.as_bytes())?;
        }
    }

    // Write daily _index.md
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

    let index_path = dir.join("_index.md");
    let index_content = format_index_md(channel_name, date, summaries, matches);
    std::fs::write(&index_path, index_content)?;

    tracing::info!("Wrote output to {}", dir.display());
    Ok(())
}

/// Format a dated section to append to a topic file.
fn format_topic_section(
    channel_name: &str,
    date: NaiveDate,
    summary: &TopicSummary,
    cluster: Option<&TopicCluster>,
) -> String {
    let date_str = date.format("%Y-%m-%d").to_string();
    let mut out = String::new();

    out.push_str(&format!("---\n\n## {} ({})\n\n", date_str, channel_name));

    // Summary
    out.push_str(&summary.summary);
    out.push_str("\n\n");

    // Participants
    out.push_str("**Participants:** ");
    out.push_str(&summary.participants.join(", "));
    out.push_str("\n\n");

    // Raw messages
    if let Some(cluster) = cluster {
        out.push_str("<details>\n<summary>Raw messages</summary>\n\n");
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
        out.push_str("</details>\n\n");
    }

    out
}

/// Format the daily _index.md that links to topic files.
fn format_index_md(
    channel_name: &str,
    date: NaiveDate,
    summaries: &[TopicSummary],
    matches: &[TopicMatch],
) -> String {
    let date_str = date.format("%Y-%m-%d").to_string();
    let mut out = format!("# {} — {}\n\n", channel_name, date_str);
    out.push_str("## Topics Discussed\n\n");

    for (i, summary) in summaries.iter().enumerate() {
        let slug = if let Some(m) = matches.get(i) {
            &m.slug
        } else {
            &crate::topic_registry::slugify(&summary.title)
        };

        // Link to the topic file in topics/
        out.push_str(&format!("- [[topics/{}|{}]]\n", slug, summary.title));
    }

    out
}
