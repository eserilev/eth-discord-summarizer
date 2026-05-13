use anyhow::{Context, Result};
use chrono::NaiveDate;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::config::DiscordConfig;

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";
const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;
const MAX_MESSAGES_PER_REQUEST: u32 = 100;
/// Delay between paginated requests to be polite to the API
const REQUEST_DELAY: Duration = Duration::from_millis(500);

/// A raw Discord message as returned by the API
#[derive(Debug, Deserialize, Clone)]
pub struct DiscordMessage {
    pub id: String,
    pub author: Author,
    pub content: String,
    pub timestamp: String,
    /// Reference to a replied-to message, if any
    pub message_reference: Option<MessageReference>,
    /// Message type (0 = default, 19 = reply, etc.)
    #[serde(rename = "type")]
    pub message_type: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Author {
    pub username: String,
    pub id: String,
    pub global_name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MessageReference {
    pub message_id: Option<String>,
    pub channel_id: Option<String>,
    pub guild_id: Option<String>,
}

/// Discord 429 rate limit response body
#[derive(Debug, Deserialize)]
struct RateLimitResponse {
    retry_after: f64,
    #[allow(dead_code)]
    global: bool,
}

/// Convert a UTC date to a Discord snowflake.
///
/// Discord snowflakes encode a timestamp as: (timestamp_ms - DISCORD_EPOCH) << 22
/// The lower 22 bits are worker/process/sequence IDs (we set to 0 for filtering).
pub fn date_to_snowflake(date: NaiveDate, start_of_day: bool) -> u64 {
    let datetime = if start_of_day {
        date.and_hms_opt(0, 0, 0).unwrap()
    } else {
        date.and_hms_opt(23, 59, 59).unwrap()
    };
    let timestamp_ms = datetime.and_utc().timestamp_millis() as u64;
    (timestamp_ms - DISCORD_EPOCH_MS) << 22
}

/// Build a configured HTTP client with auth headers.
fn build_client(config: &DiscordConfig) -> Result<Client> {
    let mut headers = HeaderMap::new();

    // User tokens: raw token. Bot tokens: "Bot <token>"
    let auth_value = HeaderValue::from_str(&config.token)
        .context("Invalid token format")?;
    headers.insert(AUTHORIZATION, auth_value);
    headers.insert(USER_AGENT, HeaderValue::from_static("eth-discord-summarizer/0.1"));

    let client = Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .build()?;

    Ok(client)
}

/// Fetch all messages from a channel for a given date range (UTC midnight to midnight).
///
/// Uses the `after` parameter to paginate forward through messages.
/// Messages are returned in chronological order (oldest first).
///
/// Handles rate limiting by parsing 429 responses and waiting before retrying.
pub async fn fetch_messages(
    config: &DiscordConfig,
    channel_id: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<DiscordMessage>> {
    let client = build_client(config)?;

    let after_snowflake = date_to_snowflake(from, true);
    let before_snowflake = date_to_snowflake(to, false);

    let mut all_messages: Vec<DiscordMessage> = Vec::new();
    let mut cursor = after_snowflake;

    info!(
        channel_id,
        from = %from,
        to = %to,
        "Fetching messages"
    );

    loop {
        let url = format!(
            "{}/channels/{}/messages?after={}&limit={}",
            DISCORD_API_BASE, channel_id, cursor, MAX_MESSAGES_PER_REQUEST
        );

        let response = make_request_with_retry(&client, &url).await?;

        let mut messages: Vec<DiscordMessage> = response;

        if messages.is_empty() {
            debug!("No more messages returned, pagination complete");
            break;
        }

        // Discord returns messages newest-first when using `after`,
        // so we sort by ID (ascending = chronological)
        messages.sort_by(|a, b| a.id.cmp(&b.id));

        // Filter out messages beyond our `before` boundary
        messages.retain(|m| {
            m.id.parse::<u64>().unwrap_or(0) <= before_snowflake
        });

        let batch_size = messages.len();
        debug!(batch_size, "Fetched message batch");

        if let Some(last) = messages.last() {
            cursor = last.id.parse::<u64>().unwrap_or(cursor);
        }

        all_messages.extend(messages);

        // If we got fewer than the limit, we've reached the end
        if batch_size < MAX_MESSAGES_PER_REQUEST as usize {
            break;
        }

        // Polite delay between requests
        sleep(REQUEST_DELAY).await;
    }

    info!(
        total = all_messages.len(),
        channel_id,
        "Finished fetching messages"
    );

    Ok(all_messages)
}

/// Make a GET request with automatic retry on 429 rate limit responses.
///
/// Parses the `retry_after` field from the 429 JSON body and waits accordingly.
/// Retries up to 5 times before giving up.
async fn make_request_with_retry(
    client: &Client,
    url: &str,
) -> Result<Vec<DiscordMessage>> {
    let max_retries = 5;

    for attempt in 0..max_retries {
        let response = client.get(url).send().await?;
        let status = response.status();

        match status {
            StatusCode::OK => {
                let messages: Vec<DiscordMessage> = response.json().await?;
                return Ok(messages);
            }
            StatusCode::TOO_MANY_REQUESTS => {
                let body: RateLimitResponse = response.json().await?;
                let wait = Duration::from_secs_f64(body.retry_after);
                warn!(
                    attempt,
                    retry_after_secs = body.retry_after,
                    "Rate limited, waiting before retry"
                );
                sleep(wait).await;
            }
            StatusCode::UNAUTHORIZED => {
                anyhow::bail!("Discord returned 401 Unauthorized — check your token");
            }
            StatusCode::FORBIDDEN => {
                anyhow::bail!(
                    "Discord returned 403 Forbidden — no access to this channel"
                );
            }
            _ => {
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!(
                    "Discord API error {}: {}",
                    status.as_u16(),
                    body
                );
            }
        }
    }

    anyhow::bail!("Exceeded max retries ({}) due to rate limiting", max_retries)
}
