use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

use crate::config::LlmConfig;

/// A match result for a single topic title
#[derive(Debug, Clone)]
pub struct TopicMatch {
    /// Original title from clustering
    pub original_title: String,
    /// Matched or newly generated slug
    pub slug: String,
    /// One-line description
    pub description: String,
    /// Whether this is a new topic (not matched to existing)
    pub is_new: bool,
}

/// Registry of all known topics across all runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicRegistry {
    pub topics: HashMap<String, TopicEntry>,
}

/// A single topic entry in the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicEntry {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub occurrences: Vec<TopicOccurrence>,
}

/// A single occurrence of a topic (one channel + date combination)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicOccurrence {
    pub channel: String,
    pub date: String,
    pub path: String,
}

/// Anthropic API types for LLM matching
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

/// LLM response schema for topic matching
#[derive(Debug, Deserialize)]
struct MatchResponse {
    matches: Vec<MatchEntry>,
}

#[derive(Debug, Deserialize)]
struct MatchEntry {
    title: String,
    slug: String,
    description: String,
    is_new: bool,
}

impl TopicRegistry {
    pub fn new() -> Self {
        Self {
            topics: HashMap::new(),
        }
    }
}

/// Load the topic registry from `topics/_registry.json`.
/// Returns an empty registry if the file doesn't exist.
pub fn load_registry(output_dir: &Path) -> Result<TopicRegistry> {
    let registry_path = output_dir.join("topics").join("_registry.json");

    if !registry_path.exists() {
        info!("No existing registry found, starting fresh");
        return Ok(TopicRegistry::new());
    }

    let content = std::fs::read_to_string(&registry_path)
        .with_context(|| format!("Failed to read registry at {}", registry_path.display()))?;

    let registry: TopicRegistry = serde_json::from_str(&content)
        .with_context(|| "Failed to parse _registry.json")?;

    info!(topics = registry.topics.len(), "Loaded topic registry");
    Ok(registry)
}

/// Save the topic registry to `topics/_registry.json`.
pub fn save_registry(output_dir: &Path, registry: &TopicRegistry) -> Result<()> {
    let topics_dir = output_dir.join("topics");
    std::fs::create_dir_all(&topics_dir)?;

    let registry_path = topics_dir.join("_registry.json");
    let content = serde_json::to_string_pretty(registry)?;
    std::fs::write(&registry_path, content)?;

    debug!("Saved registry to {}", registry_path.display());
    Ok(())
}

/// Use the LLM to match new topic titles against the existing registry.
///
/// For each new title, determines if it's a continuation of an existing topic
/// or a brand new topic. Returns a `TopicMatch` for each input title.
pub async fn match_topics(
    config: &LlmConfig,
    new_titles: &[String],
    registry: &TopicRegistry,
) -> Result<Vec<TopicMatch>> {
    if new_titles.is_empty() {
        return Ok(vec![]);
    }

    // If registry is empty, all topics are new — skip LLM call
    if registry.topics.is_empty() {
        info!("Empty registry, generating slugs for all {} topics", new_titles.len());
        return Ok(new_titles
            .iter()
            .map(|title| TopicMatch {
                original_title: title.clone(),
                slug: slugify(title),
                description: title.clone(),
                is_new: true,
            })
            .collect());
    }

    info!(
        new_titles = new_titles.len(),
        existing_topics = registry.topics.len(),
        "Matching topics against registry via LLM"
    );

    let client = Client::new();
    let prompt = build_matching_prompt(new_titles, registry);

    let request = AnthropicRequest {
        model: config.model.clone(),
        max_tokens: 4096,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    let mut req_builder = client
        .post(format!("{}/v1/messages", config.base_url))
        .header("content-type", "application/json");

    if config.api_type == "bedrock-mantle" {
        req_builder = req_builder.header("Authorization", format!("Bearer {}", config.api_key));
    } else {
        req_builder = req_builder
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01");
    }

    let response = req_builder
        .json(&request)
        .send()
        .await
        .context("Failed to call LLM API for topic matching")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("LLM API returned {} during topic matching: {}", status, body);
    }

    let api_response: AnthropicResponse = response
        .json()
        .await
        .context("Failed to parse LLM topic matching response")?;

    let text = api_response
        .content
        .into_iter()
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("");

    parse_match_response(&text, new_titles)
}

/// Update the registry with new occurrences from matched topics.
pub fn update_registry(
    registry: &mut TopicRegistry,
    matches: &[TopicMatch],
    channel: &str,
    date: chrono::NaiveDate,
) {
    let date_str = date.format("%Y/%m/%d").to_string();

    for topic_match in matches {
        let entry = registry
            .topics
            .entry(topic_match.slug.clone())
            .or_insert_with(|| TopicEntry {
                slug: topic_match.slug.clone(),
                title: topic_match.original_title.clone(),
                description: topic_match.description.clone(),
                occurrences: Vec::new(),
            });

        // Update description if this is an existing topic with a better description
        if !topic_match.is_new && !topic_match.description.is_empty() {
            entry.description = topic_match.description.clone();
        }

        let rel_path = format!("{}/{}/{}", channel, date_str, topic_match.slug);

        // Avoid duplicate occurrences
        let already_exists = entry
            .occurrences
            .iter()
            .any(|o| o.channel == channel && o.date == date_str);

        if !already_exists {
            entry.occurrences.push(TopicOccurrence {
                channel: channel.to_string(),
                date: date_str.clone(),
                path: rel_path,
            });
        }
    }
}

/// Write/update all topic MOC (Map of Content) files in `topics/`.
pub fn write_topic_files(output_dir: &Path, registry: &TopicRegistry) -> Result<()> {
    let topics_dir = output_dir.join("topics");
    std::fs::create_dir_all(&topics_dir)?;

    for entry in registry.topics.values() {
        let file_path = topics_dir.join(format!("{}.md", entry.slug));
        let content = format_topic_moc(entry);
        std::fs::write(&file_path, content)?;
    }

    info!(
        topics = registry.topics.len(),
        "Wrote topic MOC files"
    );

    Ok(())
}

/// Format a topic MOC file content.
fn format_topic_moc(entry: &TopicEntry) -> String {
    let title = title_case(&entry.slug);
    let mut out = format!("# {}\n\n{}\n\n## Discussions\n", title, entry.description);

    for occurrence in &entry.occurrences {
        out.push_str(&format!("- [[{}/{}/{}]]\n", occurrence.channel, occurrence.date, entry.slug));
    }

    out
}

/// Convert a slug to title case for display.
fn title_case(slug: &str) -> String {
    slug.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build the LLM prompt for matching new titles against existing registry topics.
fn build_matching_prompt(new_titles: &[String], registry: &TopicRegistry) -> String {
    let mut existing_list = String::new();
    for entry in registry.topics.values() {
        existing_list.push_str(&format!("- slug: \"{}\" | description: \"{}\"\n", entry.slug, entry.description));
    }

    let mut new_list = String::new();
    for (i, title) in new_titles.iter().enumerate() {
        new_list.push_str(&format!("{}. \"{}\"\n", i + 1, title));
    }

    format!(
        r#"You are matching discussion topics from an Ethereum R&D Discord channel.

## Existing topics in the registry:
{existing_list}

## New topic titles to match:
{new_list}

For each new topic title, determine:
1. Is it a continuation/variant of an existing topic? If yes, return the existing slug.
2. If it's a genuinely new topic, generate a new slug (lowercase, hyphens, no filler words like "the/a/an/of/for/in/on", max 60 chars) and a one-line description.

Respond with ONLY a JSON object (no markdown fences, no explanation):
{{
  "matches": [
    {{"title": "original title", "slug": "existing-or-new-slug", "description": "one-line description", "is_new": true}}
  ]
}}

Rules:
- Return one entry per new title, in the same order
- Be generous with matching — if a new title is clearly about the same subject as an existing topic, match it
- Slugs: lowercase, hyphens only, no filler words, max 60 chars
- Descriptions: one concise sentence describing what the topic is about"#
    )
}

/// Parse the LLM's JSON response into TopicMatch structs.
fn parse_match_response(text: &str, new_titles: &[String]) -> Result<Vec<TopicMatch>> {
    let json_str = extract_json(text);

    let response: MatchResponse =
        serde_json::from_str(json_str).with_context(|| {
            format!(
                "Failed to parse topic matching JSON from LLM response: {}",
                &text[..text.len().min(500)]
            )
        })?;

    if response.matches.len() != new_titles.len() {
        // If counts don't match, fall back to generating slugs
        tracing::warn!(
            expected = new_titles.len(),
            got = response.matches.len(),
            "LLM returned wrong number of matches, generating slugs for unmatched"
        );
    }

    let mut results = Vec::with_capacity(new_titles.len());

    for (i, title) in new_titles.iter().enumerate() {
        if let Some(entry) = response.matches.get(i) {
            results.push(TopicMatch {
                original_title: title.clone(),
                slug: if entry.slug.is_empty() {
                    slugify(title)
                } else {
                    entry.slug.clone()
                },
                description: if entry.description.is_empty() {
                    title.clone()
                } else {
                    entry.description.clone()
                },
                is_new: entry.is_new,
            });
        } else {
            // Fallback for missing entries
            results.push(TopicMatch {
                original_title: title.clone(),
                slug: slugify(title),
                description: title.clone(),
                is_new: true,
            });
        }
    }

    // Deduplicate: if two titles got the same slug, append a suffix
    let mut seen_slugs: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for result in &mut results {
        let count = seen_slugs.entry(result.slug.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            result.slug = format!("{}-{}", result.slug, count);
        }
    }

    Ok(results)
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

/// Deterministic slug generation from a title.
///
/// A proposed merge: move all occurrences from source into target.
#[derive(Debug, Clone)]
pub struct TopicMerge {
    pub source_slug: String,
    pub target_slug: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct MergeResponse {
    merges: Vec<MergeEntry>,
}

#[derive(Debug, Deserialize)]
struct MergeEntry {
    source: String,
    target: String,
    reason: String,
}

/// Ask the LLM to identify topics in the registry that should be merged.
///
/// Returns a list of proposed merges (source → target).
pub async fn find_merge_candidates(
    config: &LlmConfig,
    registry: &TopicRegistry,
) -> Result<Vec<TopicMerge>> {
    if registry.topics.len() < 2 {
        return Ok(vec![]);
    }

    let client = Client::new();
    let prompt = build_cleanup_prompt(registry);

    let request = AnthropicRequest {
        model: config.model.clone(),
        max_tokens: 4096,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    let mut req_builder = client
        .post(format!("{}/v1/messages", config.base_url))
        .header("content-type", "application/json");

    if config.api_type == "bedrock-mantle" {
        req_builder = req_builder.header("Authorization", format!("Bearer {}", config.api_key));
    } else {
        req_builder = req_builder
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01");
    }

    let response = req_builder
        .json(&request)
        .send()
        .await
        .context("Failed to call LLM API for cleanup")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("LLM API returned {} during cleanup: {}", status, body);
    }

    let api_response: AnthropicResponse = response.json().await
        .context("Failed to parse LLM cleanup response")?;

    let text = api_response
        .content
        .into_iter()
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("");

    parse_merge_response(&text)
}

fn build_cleanup_prompt(registry: &TopicRegistry) -> String {
    let mut topic_list = String::new();
    for entry in registry.topics.values() {
        topic_list.push_str(&format!(
            "- slug: \"{}\" | description: \"{}\" | occurrences: {}\n",
            entry.slug,
            entry.description,
            entry.occurrences.len()
        ));
    }

    format!(
        r#"You are reviewing a topic registry from an Ethereum R&D Discord summarizer.

Here are all the topics currently in the registry:

{topic_list}

Identify any topics that are duplicates or should be merged because they cover the same subject.
For each proposed merge, pick the better slug as the "target" (prefer the one with more occurrences or a clearer name).

Respond with ONLY a JSON object (no markdown, no explanation):
{{
  "merges": [
    {{"source": "slug-to-remove", "target": "slug-to-keep", "reason": "brief explanation"}}
  ]
}}

If no merges are needed, return: {{"merges": []}}

Rules:
- Only merge topics that are clearly about the same subject
- Don't merge topics that are merely related but distinct
- The target should be the more established/better-named topic"#
    )
}

fn parse_merge_response(text: &str) -> Result<Vec<TopicMerge>> {
    let json_str = extract_json(text);
    let response: MergeResponse = serde_json::from_str(json_str)
        .with_context(|| "Failed to parse merge JSON from LLM response")?;

    Ok(response
        .merges
        .into_iter()
        .map(|e| TopicMerge {
            source_slug: e.source,
            target_slug: e.target,
            reason: e.reason,
        })
        .collect())
}

/// Apply a merge: move all occurrences from source topic into target topic,
/// then remove the source. Also renames any output files.
pub fn apply_merge(
    registry: &mut TopicRegistry,
    merge: &TopicMerge,
    output_dir: &Path,
) -> Result<()> {
    // Get source occurrences
    let source_occurrences = registry
        .topics
        .get(&merge.source_slug)
        .map(|e| e.occurrences.clone())
        .unwrap_or_default();

    // Move occurrences to target
    if let Some(target) = registry.topics.get_mut(&merge.target_slug) {
        for mut occ in source_occurrences {
            // Update the path to use the target slug
            let old_path = occ.path.clone();
            occ.path = occ.path.replace(&merge.source_slug, &merge.target_slug);

            // Rename the actual file if it exists
            let old_file = output_dir.join(format!("{}.md", old_path));
            let new_file = output_dir.join(format!("{}.md", occ.path));
            if old_file.exists() {
                if let Some(parent) = new_file.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::rename(&old_file, &new_file)?;
                debug!(
                    "Renamed {} -> {}",
                    old_file.display(),
                    new_file.display()
                );
            }

            // Add to target if not already present
            let already_exists = target
                .occurrences
                .iter()
                .any(|o| o.channel == occ.channel && o.date == occ.date);
            if !already_exists {
                target.occurrences.push(occ);
            }
        }
    } else {
        tracing::warn!(
            "Merge target '{}' not found in registry, skipping",
            merge.target_slug
        );
        return Ok(());
    }

    // Remove source topic
    registry.topics.remove(&merge.source_slug);

    // Remove source MOC file if it exists
    let source_moc = output_dir.join("topics").join(format!("{}.md", merge.source_slug));
    if source_moc.exists() {
        std::fs::remove_file(&source_moc)?;
        debug!("Removed MOC file {}", source_moc.display());
    }

    info!(
        "Merged '{}' into '{}'",
        merge.source_slug, merge.target_slug
    );
    Ok(())
}

/// - Lowercase everything
/// - Replace spaces/special chars with hyphens
/// - Remove filler words
/// - Collapse multiple hyphens
/// - Trim leading/trailing hyphens
/// - Max 60 chars
pub fn slugify(title: &str) -> String {
    const FILLER_WORDS: &[&str] = &[
        "the", "a", "an", "of", "for", "in", "on", "to", "and", "or", "is", "are", "was",
        "were", "be", "been", "being", "with", "at", "by", "from", "that", "this", "it",
    ];

    let lower = title.to_lowercase();

    // Replace non-alphanumeric chars with hyphens
    let mut slug: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect();

    // Remove filler words (as whole words between hyphens)
    let words: Vec<&str> = slug
        .split('-')
        .filter(|w| !w.is_empty() && !FILLER_WORDS.contains(w))
        .collect();

    slug = words.join("-");

    // Collapse multiple hyphens
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }

    // Trim leading/trailing hyphens
    slug = slug.trim_matches('-').to_string();

    // Max 60 chars (truncate at a word boundary if possible)
    if slug.len() > 60 {
        if let Some(pos) = slug[..60].rfind('-') {
            slug = slug[..pos].to_string();
        } else {
            slug = slug[..60].to_string();
        }
        slug = slug.trim_end_matches('-').to_string();
    }

    slug
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("ePBS Builder Payments"), "epbs-builder-payments");
    }

    #[test]
    fn test_slugify_filler_words() {
        assert_eq!(
            slugify("The Future of Blob Gas Limits"),
            "future-blob-gas-limits"
        );
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(
            slugify("EIP-7732: Implementation Details & Questions"),
            "eip-7732-implementation-details-questions"
        );
    }

    #[test]
    fn test_slugify_max_length() {
        let long_title = "A Very Long Topic Title That Exceeds The Maximum Character Limit And Should Be Truncated Appropriately";
        let slug = slugify(long_title);
        assert!(slug.len() <= 60);
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn test_title_case() {
        assert_eq!(title_case("epbs-builder-payments"), "Epbs Builder Payments");
    }
}
