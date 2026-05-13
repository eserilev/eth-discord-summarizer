mod clustering;
mod config;
mod discord;
mod output;
mod summary;
mod topic_registry;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;

/// Ethereum Discord Channel Summarizer
///
/// Scrapes messages from configured Discord channels and generates
/// daily Markdown summaries grouped by discussion topic, with
/// Obsidian-style linked notes and a cross-channel topic registry.
#[derive(Parser, Debug)]
#[command(name = "eth-discord-summarizer")]
#[command(about = "Scrape Ethereum Discord channels and generate topic-grouped summaries")]
struct Cli {
    /// Path to config file (default: config.toml)
    #[arg(long, value_name = "PATH", default_value = "config.toml", global = true)]
    config: PathBuf,

    /// Output directory (default: ./output)
    #[arg(long, value_name = "DIR", default_value = "./output", global = true)]
    output: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Scrape channels and generate summaries
    Scrape {
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

        /// Dry run: fetch and cluster but don't write any files
        #[arg(long)]
        dry_run: bool,
    },

    /// Clean up the topic registry by merging similar/duplicate topics
    Cleanup {
        /// Just show what would be merged without doing it
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("eth_discord_summarizer=info".parse()?),
        )
        .init();

    let cli = Cli::parse();

    let cfg = config::load_config(&cli.config)
        .with_context(|| format!("Failed to load config from {}", cli.config.display()))?;

    match cli.command {
        Commands::Scrape {
            channels,
            date,
            from,
            to,
            dry_run,
        } => run_scrape(&cfg, &cli.output, channels, date, from, to, dry_run).await,
        Commands::Cleanup { dry_run } => run_cleanup(&cfg, &cli.output, dry_run).await,
    }
}

async fn run_scrape(
    cfg: &config::Config,
    output_dir: &PathBuf,
    channel_ids: Vec<String>,
    date: Option<String>,
    from: Option<String>,
    to: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let (from_date, to_date) = resolve_date_range(date, from, to)?;

    if dry_run {
        info!("DRY RUN — no files will be written");
    }
    info!("Scraping from {} to {}", from_date, to_date);

    let channels: Vec<_> = if channel_ids.is_empty() {
        cfg.channels.clone()
    } else {
        cfg.channels
            .iter()
            .filter(|c| channel_ids.contains(&c.id))
            .cloned()
            .collect()
    };

    if channels.is_empty() {
        anyhow::bail!("No channels to scrape. Check --channel args or config file.");
    }

    info!("Scraping {} channel(s)", channels.len());

    let mut registry = topic_registry::load_registry(output_dir)?;

    for channel in &channels {
        info!("Processing channel: {} ({})", channel.name, channel.id);

        let messages =
            discord::fetch_messages(&cfg.discord, &channel.id, from_date, to_date).await?;

        if messages.is_empty() {
            info!("No messages found for {} on this date range", channel.name);
            continue;
        }

        info!("Fetched {} messages from {}", messages.len(), channel.name);

        let clusters = clustering::cluster_messages(&cfg.llm, &messages).await?;
        info!("Identified {} topic(s)", clusters.len());

        let summaries = summary::summarize_topics(&cfg.llm, &clusters).await?;

        let titles: Vec<String> = clusters.iter().map(|c| c.title.clone()).collect();
        let matches = topic_registry::match_topics(&cfg.llm, &titles, &registry).await?;
        info!("Matched {} topic(s) against registry", matches.len());

        if dry_run {
            info!("--- DRY RUN RESULTS for {} ---", channel.name);
            for (i, m) in matches.iter().enumerate() {
                let status = if m.is_new { "NEW" } else { "EXISTING" };
                info!(
                    "  [{}] {} → slug: \"{}\" ({})",
                    i + 1,
                    m.original_title,
                    m.slug,
                    status
                );
            }
            continue;
        }

        // Write output
        let mut current_date = from_date;
        while current_date <= to_date {
            output::write_output(
                output_dir,
                &channel.name,
                current_date,
                &clusters,
                &summaries,
                &matches,
            )?;

            topic_registry::update_registry(
                &mut registry,
                &matches,
                &channel.name,
                current_date,
            );

            current_date = current_date.succ_opt().unwrap_or(current_date);
            if from_date == to_date {
                break;
            }
        }
    }

    if !dry_run {
        topic_registry::write_topic_files(output_dir, &registry)?;
        topic_registry::save_registry(output_dir, &registry)?;
    }

    info!("Done!");
    Ok(())
}

async fn run_cleanup(
    cfg: &config::Config,
    output_dir: &PathBuf,
    dry_run: bool,
) -> Result<()> {
    let mut registry = topic_registry::load_registry(output_dir)?;

    if registry.topics.is_empty() {
        info!("Registry is empty, nothing to clean up.");
        return Ok(());
    }

    info!(
        "Checking {} topics for potential merges...",
        registry.topics.len()
    );

    let merges = topic_registry::find_merge_candidates(&cfg.llm, &registry).await?;

    if merges.is_empty() {
        info!("No duplicate or similar topics found. Registry is clean.");
        return Ok(());
    }

    info!("Found {} potential merge(s):", merges.len());
    for merge in &merges {
        info!(
            "  Merge \"{}\" into \"{}\" ({})",
            merge.source_slug, merge.target_slug, merge.reason
        );
    }

    if dry_run {
        info!("DRY RUN — no changes applied");
        return Ok(());
    }

    // Apply merges
    for merge in &merges {
        topic_registry::apply_merge(&mut registry, merge, output_dir)?;
    }

    // Rewrite topic files and save
    topic_registry::write_topic_files(output_dir, &registry)?;
    topic_registry::save_registry(output_dir, &registry)?;

    info!("Cleanup complete. Merged {} topic(s).", merges.len());
    Ok(())
}

fn resolve_date_range(
    date: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> Result<(NaiveDate, NaiveDate)> {
    if let (Some(from), Some(to)) = (&from, &to) {
        let from_date = NaiveDate::parse_from_str(from, "%Y-%m-%d")
            .with_context(|| format!("Invalid --from date: {}", from))?;
        let to_date = NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .with_context(|| format!("Invalid --to date: {}", to))?;
        Ok((from_date, to_date))
    } else if let Some(date) = &date {
        let d = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .with_context(|| format!("Invalid --date: {}", date))?;
        Ok((d, d))
    } else {
        let yesterday = chrono::Utc::now().date_naive() - chrono::Duration::days(1);
        Ok((yesterday, yesterday))
    }
}
