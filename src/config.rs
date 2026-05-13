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
    /// API type: "anthropic" (default) or "bedrock-mantle"
    /// bedrock-mantle uses the same Anthropic Messages format but
    /// authenticates via AWS_BEARER_TOKEN_BEDROCK
    #[serde(default = "default_api_type")]
    pub api_type: String,
}

fn default_api_type() -> String {
    "anthropic".to_string()
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
    if let Ok(key) = std::env::var("AWS_BEARER_TOKEN_BEDROCK") {
        config.llm.api_key = key;
        if config.llm.api_type == "anthropic" {
            config.llm.api_type = "bedrock-mantle".to_string();
        }
    }

    Ok(config)
}
