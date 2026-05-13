# Discord API Client Module

## File: `src/discord.rs`

## API Endpoint

```
GET https://discord.com/api/v10/channels/{channel_id}/messages
```

### Query Parameters

| Parameter | Type      | Description                                      |
|-----------|-----------|--------------------------------------------------|
| `after`   | snowflake | Get messages after this message ID               |
| `before`  | snowflake | Get messages before this message ID              |
| `around`  | snowflake | Get messages around this message ID              |
| `limit`   | integer   | Max messages to return (1-100, default 50)       |

Only one of `after`, `before`, `around` can be used at a time.

### Response

JSON array of [Message objects](https://docs.discord.com/developers/resources/message#message-object). When using `after`, messages return **newest first** (we sort by ID to get chronological order).

## Authentication

- **User token:** `Authorization: <raw-token>` (no prefix)
- **Bot token:** `Authorization: Bot <token>`

Both use the same endpoints. Only the header format differs.

## Snowflake Conversion

Discord snowflakes encode timestamps:

```
snowflake = (timestamp_ms - DISCORD_EPOCH) << 22
```

Where `DISCORD_EPOCH = 1420070400000` (2015-01-01T00:00:00Z in millis).

To filter by date, convert the target date's midnight (UTC) to a snowflake and use as `after`/`before` params.

## Pagination Strategy

We paginate **forward** using `after`:

1. Start with `after = snowflake(from_date, start_of_day)`
2. Fetch up to 100 messages
3. Sort batch by ID (ascending = chronological)
4. Filter out any messages beyond `before = snowflake(to_date, end_of_day)`
5. Set `cursor = last_message.id`
6. Repeat until batch size < 100 (no more messages)
7. 500ms delay between requests

## Rate Limiting

Discord returns `429 Too Many Requests` when rate limited:

```json
{
  "message": "You are being rate limited.",
  "retry_after": 64.57,
  "global": false
}
```

### Headers to monitor:
- `X-RateLimit-Remaining` — requests left in current window
- `X-RateLimit-Reset-After` — seconds until window resets
- `Retry-After` — seconds to wait (on 429)

### Our strategy:
- On 429: parse `retry_after` from JSON body, sleep that duration, retry
- Max 5 retries before failing
- 500ms polite delay between all paginated requests regardless

### Global rate limits:
- Bots: 50 requests/second
- User tokens: similar but less documented, be conservative

## Message Object (fields we use)

```rust
pub struct DiscordMessage {
    pub id: String,              // Snowflake ID
    pub author: Author,          // Who sent it
    pub content: String,         // Message text
    pub timestamp: String,       // ISO8601 when sent
    pub message_reference: Option<MessageReference>,  // Reply target
    pub message_type: u32,       // 0=default, 19=reply
}
```

## Error Handling

| Status | Meaning                           | Action            |
|--------|-----------------------------------|-------------------|
| 200    | Success                           | Parse messages    |
| 401    | Unauthorized (bad token)          | Bail immediately  |
| 403    | Forbidden (no channel access)     | Bail immediately  |
| 429    | Rate limited                      | Wait + retry      |
| 5xx    | Server error                      | Bail with message |
