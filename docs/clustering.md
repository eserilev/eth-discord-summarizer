# Clustering Module

## File: `src/clustering.rs`

## Status: Not yet implemented

## Approach: LLM-Assisted Topic Clustering

Given a flat list of chronological messages from a single channel+day, use an LLM to identify distinct discussion topics and assign each message to a topic.

## Input

A list of `DiscordMessage` objects (chronological, from one channel, one day).

## Output

```rust
pub struct TopicCluster {
    pub title: String,                // Short topic title (inferred by LLM)
    pub messages: Vec<DiscordMessage>, // Messages belonging to this topic
}
```

## Strategy

1. Format messages as a numbered list with timestamps, authors, content, and reply references
2. Send to LLM with a prompt asking it to:
   - Identify distinct discussion topics
   - Assign each message (by number) to a topic
   - Generate a short title for each topic
3. Parse the LLM response to group messages into `TopicCluster`s

## LLM Prompt Design (TODO)

The prompt should:
- Provide context: "These are messages from an Ethereum R&D Discord channel"
- Ask for topic identification with clear boundaries
- Handle the case where a single message could belong to multiple topics (assign to primary)
- Handle noise (bot messages, reactions-only, off-topic) — group as "misc" or exclude
- Return structured output (JSON) for reliable parsing

## Considerations

- Token limits: for busy channels, may need to chunk messages and cluster in batches
- Reply chains: include `message_reference` info so LLM can see conversation threading
- Context: message type 19 = reply, which helps the LLM understand conversation flow
