# Summary Generation Module

## File: `src/summary.rs`

## Status: Not yet implemented

## Approach: LLM-Generated Summaries

Given a `TopicCluster` (title + messages), use an LLM to generate a concise summary of the discussion.

## Input

```rust
pub struct TopicCluster {
    pub title: String,
    pub messages: Vec<DiscordMessage>,
}
```

## Output

```rust
pub struct TopicSummary {
    pub title: String,           // Topic title
    pub summary: String,         // Markdown summary paragraph(s)
    pub participants: Vec<String>, // Usernames involved
    pub messages: Vec<DiscordMessage>, // Original messages (for log output)
}
```

## LLM Prompt Design (TODO)

The prompt should ask the LLM to:
- Summarize the key points of the discussion
- Note any decisions, conclusions, or action items
- Be concise but capture technical substance (this is Ethereum R&D)
- Use Markdown formatting
- Not hallucinate details that aren't in the messages

## Output Format (summary.md)

```markdown
# <channel-name> — YYYY-MM-DD

## Topic: <inferred topic title>

<summary paragraph(s)>

### Participants
- user1, user2, ...

---

## Topic: <next topic>

...
```
