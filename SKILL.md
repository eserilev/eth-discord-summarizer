---
name: eth-discord-summarizer
description: Build and maintain the eth-discord-summarizer Rust project. A scraper that pulls messages from configured Ethereum R&D Discord channels and generates daily Markdown summaries grouped by discussion topic, with raw message logs alongside.
---

# Eth Discord Summarizer

## Repo

`~/Documents/Code/Ethereum/misc/eth-discord-summarizer`

## Overview

Rust CLI tool that:
1. Scrapes configured Ethereum R&D Discord channels via REST API
2. Clusters messages into discussion topics (LLM-assisted)
3. Generates per-channel Markdown summaries grouped by topic
4. Produces raw message logs alongside summaries

## CLI Usage

```bash
# Scrape a channel for yesterday
eth-discord-summarizer --channel <channel_id> --date 2026-05-12

# Scrape for a date range
eth-discord-summarizer --channel <channel_id> --from 2026-05-10 --to 2026-05-12

# Multiple channels
eth-discord-summarizer --channel <id1> --channel <id2> --date yesterday

# Custom config/output
eth-discord-summarizer --channel <id> --config ./my-config.toml --output ./results
```

## Architecture

```
src/
├── main.rs          # CLI (clap) + orchestration
├── config.rs        # TOML config parsing
├── discord.rs       # Discord REST API client
├── clustering.rs    # LLM-assisted topic clustering
├── summary.rs       # LLM summary generation
└── output.rs        # Write summary.md + messages.log
```

## Module References

Detailed docs for each module (Obsidian-style linked docs — each module gets its own reference file, linked from here):

- **[[discord]]** — Message fetching, pagination, snowflakes, rate limiting
- **[[clustering]]** — LLM-assisted topic grouping strategy
- **[[summary]]** — Output format and LLM prompting
- **[[output]]** — File structure and formatting

Reference files live in `docs/` within the repo and are committed alongside source code.

### Convention

All skill/reference documentation follows Obsidian-style linked docs:
- Top-level SKILL.md links to sub-documents via `[[filename]]`
- Each module gets its own `docs/<module>.md`
- Keep docs in the repo (committed) so they travel with the code

## Configuration

```toml
[discord]
token = "user-token-here"   # or "Bot <token>" for bot auth

[llm]
api_key = "..."
model = "claude-sonnet-4-20250514"
base_url = "https://api.anthropic.com"

[[channels]]
id = "123456789"
name = "consensus-specs"
guild_id = "987654321"
```

## Key Design Decisions

- Auth: user token initially (same API, just different Authorization header format — easy to switch to bot)
- Pagination: forward (`after` param), 100 msgs/request, 500ms delay between requests
- Rate limiting: parse 429 `retry_after`, wait, retry up to 5x
- Topic clustering: LLM-assisted (not heuristic)
- Output: idempotent (rerun overwrites cleanly)
- Dates: UTC midnight-to-midnight boundaries

## Git Workflow

- PRs always against `eserilev/eth-discord-summarizer`
- Don't push without permission
