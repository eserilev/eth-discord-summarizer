use anyhow::Result;
use chrono::NaiveDate;
use serde::Deserialize;

use crate::config::DiscordConfig;

/// Discord epoch: first second of 2015 (used for snowflake conversion)
const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

/// A raw Discord message as returned by the API
#[derive(Debug, Deserialize, Clone)]
pub struct DiscordMessage {
    pub id: String,
    pub author: Author,
    pub content: String,
    pub timestamp: String,
    /// Reference to a replied-to message, if any
    pub message_reference: Option<MessageReference>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Author {
    pub username: String,
    pub id: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MessageReference {
    pub message_id: Option<String>,
    pub channel_id: Option<String>,
}

/// Convert a UTC date to a Discord snowflake (for use as `after`/`before` params)
pub fn date_to_snowflake(date: NaiveDate, start_of_day: bool) -> u64 {
    let datetime = if start_of_day {
        date.and_hms_opt(0, 0, 0).unwrap()
    } else {
        date.and_hms_opt(23, 59, 59).unwrap()
    };
    let timestamp_ms = datetime.and_utc().timestamp_millis() as u64;
    // Discord snowflake = (timestamp_ms - DISCORD_EPOCH) << 22
    (timestamp_ms - DISCORD_EPOCH_MS) << 22
}

/// Fetch all messages from a channel for a given date range.
///
/// Paginates through Discord's API (max 100 messages per request).
/// Respects rate limits (429 responses) by waiting and retrying.
///
/// Discord API endpoint:
///   GET /channels/{channel_id}/messages?after={snowflake}&before={snowflake}&limit=100
///
/// Authorization header: raw token for user tokens, "Bot <token>" for bot tokens.
pub async fn fetch_messages(
    _config: &DiscordConfig,
    _channel_id: &str,
    _from: NaiveDate,
    _to: NaiveDate,
) -> Result<Vec<DiscordMessage>> {
    // TODO: Implement Discord API message fetching
    // 1. Convert from/to dates to snowflakes using date_to_snowflake()
    // 2. Make paginated GET requests (limit=100 per request)
    // 3. Use `after` parameter starting from the `from` snowflake
    // 4. Continue until we get fewer than 100 messages or exceed `to` snowflake
    // 5. Handle 429 rate limit responses: parse Retry-After header, sleep, retry
    // 6. Add small delays between requests to be polite to the API
    //
    // Headers needed:
    //   Authorization: <token>  (raw token for user auth)
    //   User-Agent: eth-discord-summarizer
    //
    // Response is a JSON array of message objects, newest first when using `before`,
    // oldest first when using `after`.

    tracing::warn!("fetch_messages not yet implemented");
    Ok(vec![])
}
