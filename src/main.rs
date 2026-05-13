mod clustering;
mod config;
mod discord;
mod output;
mod summary;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::Parser;
use std::path::PathBuf;
use tracing::info;

/// Ethereum Discord Channel Summarizer
///
/// Scrapes messages from configured Discord channels and generates
/// daily Markdown summaries grouped by discussion topic.
#[derive(Parser, Debug)]
#[command(name = "eth-discord-summarizer")]
#[command(about = "Scrape Ethereum Discord channels and generate topic-grouped summaries")]
struct Cli {
    /// Channel ID(s) to scrape (can specify multiple times)
    #[arg(long = "channel", value_name = "CHANNEL_ID")]
    channels: Vec<String>,

    /// Date to scrape (YYYY-MM-DD, default: yesterday)
    #[arg(long, value_name = "DATE")]
    date: Option<String>,

    /// Start date for range scrape (YYYY-MM-DD)
    #[arg(long = "from", value_name = "DATE")]
    from: Option<String>,

    /// End date for range scrape (YYYY-MM-DD)
    #[arg(long = "to", value_name = "DATE")]
    to: Option<String>,

    /// Path to config file (default: config.toml)
    #[arg(long, value_name = "PATH", default_value = "config.toml")]
    config: PathBuf,

    /// Output directory (default: ./output)
    #[arg(long, value_name = "DIR", default_value = "./output")]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing/logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("eth_discord_summarizer=info".parse()?),
        )
        .init();

    let cli = Cli::parse();

    // Load configuration
    let cfg = config::load_config(&cli.config)
        .with_context(|| format!("Failed to load config from {}", cli.config.display()))?;

    // Determine date range
    let (from_date, to_date) = resolve_date_range(&cli)?;
    info!("Scraping from {} to {}", from_date, to_date);

    // Determine which channels to scrape
    let channels: Vec<_> = if cli.channels.is_empty() {
        // Use all channels from config
        cfg.channels.clone()
    } else {
        // Filter to only requested channels
        cfg.channels
            .iter()
            .filter(|c| cli.channels.contains(&c.id))
            .cloned()
            .collect()
    };

    if channels.is_empty() {
        anyhow::bail!("No channels to scrape. Check --channel args or config file.");
    }

    info!("Scraping {} channel(s)", channels.len());

    // Process each channel
    for channel in &channels {
        info!("Processing channel: {} ({})", channel.name, channel.id);

        // Fetch messages from Discord
        let messages =
            discord::fetch_messages(&cfg.discord, &channel.id, from_date, to_date).await?;

        if messages.is_empty() {
            info!("No messages found for {} on this date range", channel.name);
            continue;
        }

        info!("Fetched {} messages from {}", messages.len(), channel.name);

        // Cluster messages into topics
        let clusters = clustering::cluster_messages(&cfg.llm, &messages).await?;
        info!("Identified {} topic(s)", clusters.len());

        // Generate summaries for each topic
        let summaries = summary::summarize_topics(&cfg.llm, &clusters).await?;

        // Write output files
        output::write_output(&cli.output, &channel.name, from_date, &clusters, &summaries)?;
    }

    info!("Done!");
    Ok(())
}

/// Resolve the date range from CLI arguments
fn resolve_date_range(cli: &Cli) -> Result<(NaiveDate, NaiveDate)> {
    if let (Some(from), Some(to)) = (&cli.from, &cli.to) {
        let from_date = NaiveDate::parse_from_str(from, "%Y-%m-%d")
            .with_context(|| format!("Invalid --from date: {}", from))?;
        let to_date = NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .with_context(|| format!("Invalid --to date: {}", to))?;
        Ok((from_date, to_date))
    } else if let Some(date) = &cli.date {
        let d = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .with_context(|| format!("Invalid --date: {}", date))?;
        Ok((d, d))
    } else {
        // Default: yesterday
        let yesterday = chrono::Utc::now().date_naive() - chrono::Duration::days(1);
        Ok((yesterday, yesterday))
    }
}
