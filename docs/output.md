# Output Module

## File: `src/output.rs`

## Status: Implemented

## Output Directory Structure

```
<output_dir>/YYYY-MM-DD/
├── <channel-name>/
│   ├── summary.md       # Topic-grouped summaries
│   └── messages.log     # Raw messages grouped by topic
```

## summary.md Format

```markdown
# <channel-name> — YYYY-MM-DD

## Topic: <title>

<LLM-generated summary>

### Participants
- user1, user2, ...

---

## Topic: <next title>
...
```

## messages.log Format

```
=== Topic: <title> ===

[2026-05-13 14:01:02] user1: message text
[2026-05-13 14:01:15] user2: reply text
...

=== Topic: <next title> ===
...
```

## Behavior

- Creates directories as needed
- Overwrites existing files (idempotent)
- Uses channel `name` from config for directory naming (not raw ID)
- Timestamps in messages.log are formatted from ISO8601 to `YYYY-MM-DD HH:MM:SS`
