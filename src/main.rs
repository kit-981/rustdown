#![warn(clippy::all, clippy::cargo, clippy::pedantic)]
#![allow(clippy::multiple_crate_versions)]

mod cache;
mod channel;
mod digest;
mod download;

use cache::Cache;
use channel::Manifest;
use clap::{Parser, Subcommand};
use download::Downloader;
use eyre::{eyre, Result, WrapErr};
use std::{num::NonZeroUsize, path::PathBuf};
use tokio::{fs::File, io::AsyncReadExt};
use tracing::{info, Level};

async fn synchronise(cache: &Cache, jobs: NonZeroUsize) -> Result<()> {
    cache.refresh(&Downloader::default(), jobs).await?;

    info!("synchronised cache");
    Ok(())
}

async fn verify(cache: &Cache, jobs: NonZeroUsize) -> Result<()> {
    cache.refresh(&Downloader::default(), jobs).await?;

    info!("verified cache");
    Ok(())
}

/// Collects the program arguments.
#[derive(Parser, Debug)]
#[clap(version, about)]
struct Arguments {
    #[clap(subcommand)]
    action: Action,

    /// The path of the cache.
    #[clap(short, long)]
    path: PathBuf,

    /// The path to the channel manifest.
    #[clap(short, long)]
    manifest: PathBuf,

    /// The number of jobs that can run in parallel.
    #[clap(short, long, default_value_t = NonZeroUsize::new(1).unwrap())]
    jobs: NonZeroUsize,

    /// The log level.
    #[clap(short, long, default_value_t = Level::INFO)]
    log_level: Level,
}

#[derive(Debug, Subcommand)]
enum Action {
    /// Synchronises a cache
    #[clap(name = "sync")]
    Synchronise,

    /// Verifies the cache
    ///
    /// This will (re)download missing or corrupt files described in the manifest. Files that are
    /// not described in the manifest are not removed.
    #[clap(name = "verify")]
    Verify,
}

#[tokio::main]
async fn main() -> Result<()> {
    let arguments = Arguments::parse();

    tracing_subscriber::fmt()
        .with_max_level(arguments.log_level)
        .init();

    let mut bytes = Vec::new();
    let mut file = File::open(arguments.manifest)
        .await
        .wrap_err(eyre!("failed to open manifest"))?;

    file.read_to_end(&mut bytes)
        .await
        .wrap_err(eyre!("failed to read manifest"))?;

    let manifest =
        Manifest::from_slice(bytes.as_slice()).wrap_err(eyre!("failed to deserialise manifest"))?;

    let cache = Cache::new(arguments.path, manifest);

    match arguments.action {
        Action::Synchronise => synchronise(&cache, arguments.jobs).await,
        Action::Verify => verify(&cache, arguments.jobs).await,
    }
}
