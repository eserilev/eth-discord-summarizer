# Output Module

## File: `src/output.rs`

## Status: Implemented

## Output Directory Structure (Obsidian-style)

```
<output_dir>/
├── topics/
│   ├── _registry.json              # Topic registry (source of truth)
│   ├── epbs-builder-payments.md    # MOC: description + links to all dates
│   └── blob-gas-limits.md
├── <channel-name>/
│   └── YYYY/
│       └── MM/
│           └── DD/
│               ├── _index.md                  # Daily overview linking to topics via [[]]
│               ├── epbs-builder-payments.md   # Individual topic summary
│               └── blob-gas-limits.md
```

## Individual Topic File Format (`<channel>/YYYY/MM/DD/<slug>.md`)

```markdown
# <Topic Title>

**Channel:** <channel-name> | **Date:** YYYY-MM-DD

## Summary

<LLM-generated summary>

## Participants

- user1
- user2

## Raw Messages

<details>
<summary>Show raw messages</summary>

\```
[timestamp] author: message text
[timestamp] author: reply text
\```

</details>
```

## Daily Index Format (`<channel>/YYYY/MM/DD/_index.md`)

```markdown
# <channel-name> — YYYY-MM-DD

## Topics

- [[slug-1]] — Topic Title 1
- [[slug-2]] — Topic Title 2
```

## Topic MOC Format (`topics/<slug>.md`)

```markdown
# <Title Case Topic Name>

<one-line description>

## Discussions
- [[<channel>/YYYY/MM/DD/<slug>]]
- [[<channel>/YYYY/MM/DD/<slug>]]
```

## Behavior

- Creates directories as needed
- Overwrites existing files (idempotent)
- Uses channel `name` from config for directory naming (not raw ID)
- Topics span across channels — same topic in different channels links to both
- The `topics/_registry.json` is the source of truth; MOC files are generated from it
- Slug filenames come from the topic registry (LLM-matched or newly generated)
- Raw messages are included in collapsible `<details>` sections
- Obsidian `[[wikilink]]` syntax used for cross-linking
