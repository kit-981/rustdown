#![warn(clippy::all, clippy::cargo, clippy::pedantic)]
#![allow(clippy::multiple_crate_versions)]

mod cache;
mod channel;
mod digest;
mod download;
mod extension;

use cache::Cache;
use channel::{manifest::Manifest, Channel};
use clap::{Arg, Command};
use download::Downloader;
use eyre::{eyre, Result, WrapErr};
use std::{env, num::NonZeroUsize, path::PathBuf, str::FromStr};
use tokio::{fs::File, io::AsyncReadExt};
use tracing::{info, Level};
use url::Url;

async fn synchronise(cache: &Cache, jobs: NonZeroUsize) -> Result<()> {
    cache.refresh(&Downloader::default(), jobs).await?;

    info!("synchronised cache");
    Ok(())
}

#[derive(Debug)]
struct Arguments {
    path: PathBuf,
    host: Url,
    manifest: PathBuf,
    channel: Channel,
    jobs: NonZeroUsize,
    log_level: Level,
}

#[derive(Debug)]
struct Parser {
    command: Command<'static>,
}

impl Parser {
    fn new() -> Self {
        let command = Command::new(env!("CARGO_PKG_NAME"))
            .version(env!("CARGO_PKG_VERSION"))
            .author(env!("CARGO_PKG_AUTHORS"))
            .about(env!("CARGO_PKG_DESCRIPTION"))
            .arg(
                Arg::new("path")
                    .takes_value(true)
                    .validator(|p| Url::from_directory_path(p).map_err(|_| "invalid path"))
                    .required(true)
                    .help("The path of the cache"),
            )
            .arg(
                Arg::new("host")
                    .long("host")
                    .takes_value(true)
                    .validator(Url::parse)
                    .help("The URL describing where the cache will be hosted")
                    .long_help("The URL describing where the cache will be hosted. The file system location will be used when this argument is not provided."),
            )
            .arg(
                Arg::new("manifest")
                    .short('m')
                    .long("manifest")
                    .takes_value(true)
                    .required(true)
                    .help("The path to the channel manifest"),
            )
            .arg(
                Arg::new("channel")
                    .short('c')
                    .long("channel")
                    .takes_value(true)
                    .validator(Channel::from_str)
                    .help("The channel that the manifest is for"),
            )
            .arg(
                Arg::new("jobs")
                    .short('j')
                    .long("jobs")
                    .takes_value(true)
                    .validator(NonZeroUsize::from_str)
                    .help("The number of jobs that can run in parallel"),
            )
            .arg(
                Arg::new("log_level")
                    .short('l')
                    .long("log-level")
                    .takes_value(true)
                    .possible_values(["trace", "debug", "info", "warn", "error"])
                    .default_value("info")
                    .help("The log level"),
            );

        Self { command }
    }

    fn parse(&mut self, arguments: impl Iterator<Item = String>) -> Result<Arguments, clap::Error> {
        let matches = self.command.try_get_matches_from_mut(arguments)?;

        let path = PathBuf::from(matches.value_of("path").expect("missing path"));
        let host = match matches.value_of("host") {
            Some(host) => Url::parse(host).expect("invalid host"),
            None => Url::from_directory_path(&path).expect("invalid path"),
        };

        let manifest = PathBuf::from(matches.value_of("manifest").expect("missing manifest"));
        let channel = Channel::from_str(matches.value_of("channel").expect("missing channel"))
            .expect("invalid channel");

        let jobs = NonZeroUsize::from_str(matches.value_of("jobs").expect("missing jobs"))
            .expect("invalid jobs");

        let log_level = Level::from_str(matches.value_of("log_level").expect("missing log level"))
            .expect("invalid log level");

        Ok(Arguments {
            path,
            host,
            manifest,
            channel,
            jobs,
            log_level,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let arguments = Parser::new()
        .parse(env::args().into_iter())
        .map_err(|error| error.exit())
        .expect("unhandled error");

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

    let cache = Cache::new(arguments.path, manifest, arguments.channel, arguments.host);
    synchronise(&cache, arguments.jobs).await
}
