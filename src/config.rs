use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

/// Top-level configuration structure
#[derive(Debug, Deserialize)]
pub struct Config {
    pub discord: DiscordConfig,
    pub llm: LlmConfig,
    pub channels: Vec<ChannelConfig>,
}

/// Discord authentication configuration
#[derive(Debug, Deserialize)]
pub struct DiscordConfig {
    /// User token (raw) or bot token (with "Bot " prefix)
    pub token: String,
}

/// LLM API configuration for clustering and summarization
#[derive(Debug, Deserialize)]
pub struct LlmConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

/// A single Discord channel to scrape
#[derive(Debug, Deserialize, Clone)]
pub struct ChannelConfig {
    pub id: String,
    pub name: String,
    pub guild_id: String,
}

/// Load and parse the config file from the given path.
///
/// Supports env var overrides:
///   - DISCORD_TOKEN overrides discord.token
///   - ANTHROPIC_API_KEY overrides llm.api_key
pub fn load_config(path: &Path) -> Result<Config> {
    let content = std::fs::read_to_string(path)?;
    let mut config: Config = toml::from_str(&content)?;

    // Env var overrides
    if let Ok(token) = std::env::var("DISCORD_TOKEN") {
        config.discord.token = token;
    }
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        config.llm.api_key = key;
    }

    Ok(config)
}
