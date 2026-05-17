//! Read messages from the ethereum/eth-rnd-archive GitHub repository.
//!
//! Supports two modes:
//! - **Local**: read from a local clone of the repo
//! - **Remote**: fetch raw JSON files directly from GitHub (no clone needed)
//!
//! The archive stores one JSON file per channel per day, e.g.:
//!   `consensus-dev/2024-01-15.json`
//!
//! Each file is an array of `ArchivedMessage` objects.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use reqwest::Client;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::discord::DiscordMessage;

const GITHUB_RAW_BASE: &str =
    "https://raw.githubusercontent.com/ethereum/eth-rnd-archive/master";

/// Delay between GitHub raw requests to avoid rate limiting
const REQUEST_DELAY: Duration = Duration::from_millis(200);

/// A message as stored in the eth-rnd-archive JSON files.
#[derive(Debug, Deserialize, Clone)]
pub struct ArchivedMessage {
    pub author: String,
    pub category: String,
    /// Thread parent channel name, if this message is in a thread
    #[serde(default)]
    pub parent: String,
    pub content: String,
    pub created_at: String,
    #[serde(default)]
    pub attachments: Option<Vec<ArchivedAttachment>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ArchivedAttachment {
    #[serde(rename = "type")]
    pub mime_type: String,
    pub origin_name: String,
    /// Hash of the attachment bytes (filename in attachments/ subdir)
    pub content: String,
}

/// Source for reading archive data.
#[derive(Debug, Clone)]
pub enum ArchiveSource {
    /// Path to a local clone of ethereum/eth-rnd-archive
    Local(PathBuf),
    /// Fetch directly from GitHub raw URLs (no local clone needed)
    Remote,
}

impl ArchiveSource {
    /// Create a local source from a path.
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self::Local(path.into())
    }

    /// Create a remote source (fetches from GitHub).
    pub fn remote() -> Self {
        Self::Remote
    }

    /// Try to auto-detect: use local clone if it exists, otherwise remote.
    ///
    /// Checks ETH_RND_ARCHIVE_PATH env var first, then common locations.
    pub fn auto_detect() -> Self {
        // Check env var first
        if let Ok(path) = std::env::var("ETH_RND_ARCHIVE_PATH") {
            let p = PathBuf::from(&path);
            if p.exists() {
                info!("Using archive from ETH_RND_ARCHIVE_PATH: {}", p.display());
                return Self::Local(p);
            }
        }

        // Check common locations
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let candidates = [
            PathBuf::from(&home).join("eth-rnd-archive"),
            PathBuf::from(&home).join("Documents/Code/Ethereum/misc/eth-rnd-archive"),
            PathBuf::from("/tmp/eth-rnd-archive"),
        ];
        for candidate in candidates {
            if candidate.join("README.md").exists() {
                info!("Using local archive at {}", candidate.display());
                return Self::Local(candidate);
            }
        }
        info!("No local archive found, using remote GitHub source");
        Self::Remote
    }
}

/// Fetch all messages for a channel on a given date range from the archive.
///
/// Returns messages in chronological order, converted to `DiscordMessage`
/// for compatibility with the clustering/summary pipeline.
pub async fn fetch_archived_messages(
    source: &ArchiveSource,
    channel_name: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<DiscordMessage>> {
    let mut all_messages = Vec::new();
    let mut current = from;

    info!(
        channel = channel_name,
        from = %from,
        to = %to,
        "Fetching archived messages"
    );

    while current <= to {
        let filename = format!("{}.json", current);

        let raw = match source {
            ArchiveSource::Local(base) => {
                read_local_file(base, channel_name, &filename).await?
            }
            ArchiveSource::Remote => {
                fetch_remote_file(channel_name, &filename).await?
            }
        };

        if let Some(json_str) = raw {
            let archived: Vec<ArchivedMessage> = serde_json::from_str(&json_str)
                .with_context(|| {
                    format!("Failed to parse {}/{}", channel_name, filename)
                })?;

            debug!(
                channel = channel_name,
                date = %current,
                count = archived.len(),
                "Parsed archived messages"
            );

            let converted = convert_messages(archived, current);
            all_messages.extend(converted);
        } else {
            debug!(
                channel = channel_name,
                date = %current,
                "No archive file found for this date"
            );
        }

        current = current.succ_opt().unwrap_or(current);
        if current > to {
            break;
        }
    }

    info!(
        total = all_messages.len(),
        channel = channel_name,
        "Finished reading archived messages"
    );

    Ok(all_messages)
}

/// Read a file from the local clone.
async fn read_local_file(
    base: &Path,
    channel_name: &str,
    filename: &str,
) -> Result<Option<String>> {
    let path = base.join(channel_name).join(filename);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| {
            format!("Failed to read {}", path.display())
        }),
    }
}

/// Fetch a file from GitHub raw content URLs.
async fn fetch_remote_file(channel_name: &str, filename: &str) -> Result<Option<String>> {
    let url = format!("{}/{}/{}", GITHUB_RAW_BASE, channel_name, filename);

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let response = client.get(&url).send().await?;

    match response.status() {
        reqwest::StatusCode::OK => {
            let body = response.text().await?;
            Ok(Some(body))
        }
        reqwest::StatusCode::NOT_FOUND => Ok(None),
        status if status == reqwest::StatusCode::TOO_MANY_REQUESTS => {
            warn!("GitHub rate limited, waiting 60s before retry");
            sleep(Duration::from_secs(60)).await;
            // Single retry
            let response = client.get(&url).send().await?;
            if response.status().is_success() {
                Ok(Some(response.text().await?))
            } else {
                warn!(
                    "Failed to fetch {} after rate limit retry: {}",
                    url,
                    response.status()
                );
                Ok(None)
            }
        }
        status => {
            warn!("Failed to fetch {}: HTTP {}", url, status.as_u16());
            Ok(None)
        }
    }
}

/// Convert archived messages into the `DiscordMessage` format used by
/// the clustering and summary pipeline.
///
/// Since the archive doesn't have Discord snowflake IDs, we generate
/// synthetic sequential IDs to maintain ordering.
fn convert_messages(archived: Vec<ArchivedMessage>, date: NaiveDate) -> Vec<DiscordMessage> {
    use crate::discord::{Author, DiscordMessage as DM, MessageReference};

    archived
        .into_iter()
        .enumerate()
        .map(|(i, msg)| {
            // Generate a synthetic ID that preserves ordering
            // Use date as base + sequence number
            let synthetic_id = format!(
                "{}{:06}",
                date.format("%Y%m%d"),
                i
            );

            DM {
                id: synthetic_id,
                author: Author {
                    username: msg.author.clone(),
                    id: msg.author, // no user ID in archive, use username
                    global_name: None,
                },
                content: msg.content,
                timestamp: msg.created_at,
                message_reference: if msg.parent.is_empty() {
                    None
                } else {
                    // Use parent as a hint that this is a thread message
                    Some(MessageReference {
                        message_id: None,
                        channel_id: Some(msg.parent),
                        guild_id: None,
                    })
                },
                message_type: 0,
            }
        })
        .collect()
}

/// List all available channels in the archive.
///
/// For local sources, reads directories. For remote, fetches the repo tree.
pub async fn list_channels(source: &ArchiveSource) -> Result<Vec<String>> {
    match source {
        ArchiveSource::Local(base) => {
            let mut channels = Vec::new();
            let mut entries = tokio::fs::read_dir(base).await?;
            while let Some(entry) = entries.next_entry().await? {
                if entry.file_type().await?.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        // Skip hidden dirs and common non-channel dirs
                        if !name.starts_with('.') && name != "attachments" {
                            channels.push(name.to_string());
                        }
                    }
                }
            }
            channels.sort();
            Ok(channels)
        }
        ArchiveSource::Remote => {
            // Use GitHub API to list top-level directories
            let client = Client::builder()
                .timeout(Duration::from_secs(30))
                .build()?;

            let url = "https://api.github.com/repos/ethereum/eth-rnd-archive/contents/";
            let response = client
                .get(url)
                .header("User-Agent", "eth-discord-summarizer/0.1")
                .send()
                .await?;

            if !response.status().is_success() {
                anyhow::bail!(
                    "Failed to list archive channels: HTTP {}",
                    response.status()
                );
            }

            #[derive(Deserialize)]
            struct GhEntry {
                name: String,
                #[serde(rename = "type")]
                entry_type: String,
            }

            let entries: Vec<GhEntry> = response.json().await?;
            let channels: Vec<String> = entries
                .into_iter()
                .filter(|e| e.entry_type == "dir")
                .map(|e| e.name)
                .collect();

            Ok(channels)
        }
    }
}
