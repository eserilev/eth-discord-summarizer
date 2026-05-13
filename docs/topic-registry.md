# Topic Registry

## File: `src/topic_registry.rs`

## Status: Implemented

## Overview

The topic registry tracks all discussion topics across channels and dates, enabling cross-channel linking and continuity detection between runs. It uses LLM matching to determine whether a new topic title is a continuation of a previously seen topic or something entirely new.

## Data Structures

### TopicRegistry

```rust
struct TopicRegistry {
    topics: HashMap<String, TopicEntry>,  // slug -> entry
}
```

### TopicEntry

```rust
struct TopicEntry {
    slug: String,
    title: String,
    description: String,              // One-liner description
    occurrences: Vec<TopicOccurrence>,
}
```

### TopicOccurrence

```rust
struct TopicOccurrence {
    channel: String,    // e.g., "consensus-specs"
    date: String,       // "YYYY/MM/DD"
    path: String,       // relative path: "consensus-specs/2026/05/13/epbs-builder-payments"
}
```

## Storage

- **Location:** `<output_dir>/topics/_registry.json`
- **Format:** Pretty-printed JSON (human-readable, git-friendly)
- **Persistence:** Loaded at start, saved at end of each run

## Functions

### `load_registry(output_dir) -> TopicRegistry`
Load from `topics/_registry.json`. Returns empty registry if file doesn't exist.

### `save_registry(output_dir, registry)`
Write the full registry to `topics/_registry.json`.

### `match_topics(config, new_titles, registry) -> Vec<TopicMatch>`
Use LLM to match new topic titles against existing registry entries.
- If registry is empty, skips LLM call and generates slugs directly.
- Returns a `TopicMatch` per input title with slug, description, and is_new flag.

### `update_registry(registry, matches, channel, date)`
Add new occurrences to the registry for matched/new topics.
- Avoids duplicate occurrences (same channel + date combination).
- Creates new entries for `is_new` topics.

### `write_topic_files(output_dir, registry)`
Write/update all `topics/<slug>.md` MOC files from the registry.

### `slugify(title) -> String`
Deterministic slug generation:
- Lowercase everything
- Replace spaces/special chars with hyphens
- Remove filler words (the, a, an, of, for, in, on, to, and, or, is, are, etc.)
- Collapse multiple hyphens
- Trim leading/trailing hyphens
- Max 60 chars (truncates at word boundary)

## LLM Matching

### When it's called
After clustering produces topic titles, before writing output files.

### Prompt strategy
Feeds the LLM:
1. List of existing topic slugs + descriptions from the registry
2. List of new topic titles from the current run

Asks: for each new title, is it a continuation of an existing topic?

### Response format
```json
{
  "matches": [
    {"title": "...", "slug": "existing-or-new-slug", "description": "...", "is_new": true}
  ]
}
```

### Fallback behavior
- If LLM returns wrong number of matches, unmatched titles get auto-generated slugs
- If LLM call fails, could fall back to slugify (not currently implemented — errors propagate)

## Cross-Channel Linking

Topics are global — the same topic slug can appear in multiple channels. The MOC file lists all occurrences regardless of channel:

```markdown
# Epbs Builder Payments

Discussion about ePBS builder payment mechanisms and implementation details.

## Discussions
- [[consensus-specs/2026/05/12/epbs-builder-payments]]
- [[consensus-specs/2026/05/13/epbs-builder-payments]]
- [[execution-layer/2026/05/13/epbs-builder-payments]]
```

## Design Decisions

- **Registry is source of truth:** MOC files are always regenerated from registry data
- **LLM for matching:** Semantic matching beats string similarity for topic continuity
- **Skip LLM on empty registry:** Optimization — no point matching against nothing
- **Slug max 60 chars:** Keeps filenames manageable in file systems
- **Same auth pattern:** Uses the same bedrock-mantle / anthropic auth as clustering.rs
