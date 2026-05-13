# Clustering Module

## File: `src/clustering.rs`

## Status: Implemented

## Approach: LLM-Assisted Topic Clustering

Given a flat list of chronological messages from a single channel+day, uses the Anthropic API to identify distinct discussion topics and assign each message to exactly one topic.

## Pipeline

1. Format messages as a numbered list with: index, timestamp, author, reply references, content
2. Send to LLM with structured prompt asking for JSON topic assignments
3. Parse JSON response → `Vec<TopicAssignment>` (title + message indices)
4. Build `TopicCluster` structs with messages grouped and sorted chronologically

## Batching

- `MAX_MESSAGES_PER_BATCH = 200`
- If a channel has >200 messages in a day, chunk into batches and cluster each independently
- TODO: merge topics with similar titles across batch boundaries

## LLM Prompt Design

The prompt:
- Provides context ("Ethereum R&D Discord channel")
- Shows messages as numbered list with reply chain references (e.g., `[replying to #5]`)
- Asks for JSON output: `{ "topics": [{ "title": "...", "message_indices": [...] }] }`
- Rules: every message assigned to exactly one topic, titles max 10 words, isolated messages go to "Miscellaneous"

## Response Parsing

- Strips markdown code fences if present (```json ... ```)
- Finds first `{` to last `}` as JSON
- Validates all indices are in range
- Builds clusters with participants extracted and deduped

## API Integration

Uses Anthropic Messages API:
- Endpoint: `{base_url}/v1/messages`
- Headers: `x-api-key`, `anthropic-version: 2023-06-01`
- `max_tokens: 4096`
- Model from config (default: claude-sonnet-4-20250514)

## Error Handling

- Non-2xx API responses: bail with status + body
- Invalid JSON from LLM: bail with parse error
- Out-of-range indices: bail with validation error
