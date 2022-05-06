#![warn(clippy::all, clippy::cargo, clippy::pedantic)]
#![allow(clippy::multiple_crate_versions)]

mod cache;
mod channel;
mod digest;
mod download;
mod extension;

use ahash::AHashMap;
use cache::Cache;
use channel::{manifest::Manifest, Channel};
use clap::{
    error::ErrorKind::{TooFewValues, ValueValidation},
    Arg, Command,
};
use download::Downloader;
use eyre::Result;
use futures::{stream, StreamExt, TryStreamExt};
use std::{env, iter::IntoIterator, num::NonZeroUsize, path::PathBuf, str::FromStr};
use tokio::{fs::File, io::AsyncReadExt};
use tracing::{info, Level};
use url::Url;

#[derive(Debug)]
struct Arguments {
    path: PathBuf,
    host: Url,
    channels: AHashMap<Channel, PathBuf>,
    jobs: NonZeroUsize,
    log_level: Level,
}

#[derive(Debug)]
struct Parser<'a> {
    command: Command<'a>,
}

impl<'a> Parser<'a> {
    fn new(ncpus: &'a str) -> Self {
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
                    .multiple_values(true)
                    .min_values(2)
                    .multiple_occurrences(true)
                    .value_names(&["PATH", "CHANNEL"])
                    .required(true)
                    .help("The path to the channel manifest"),
            )
            .arg(
                Arg::new("jobs")
                    .short('j')
                    .long("jobs")
                    .takes_value(true)
                    .default_value(ncpus)
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

        let channels = matches
            .grouped_values_of("manifest")
            .expect("missing manifest")
            .into_iter()
            .map(IntoIterator::into_iter)
            .map(|mut group| {
                let path = PathBuf::from(group.next().ok_or_else(|| {
                    self.command
                        .clone()
                        .error(TooFewValues, "missing manifest path")
                })?);

                let channel = Channel::from_str(group.next().ok_or_else(|| {
                    self.command
                        .clone()
                        .error(TooFewValues, "missing manifest channel")
                })?)
                .map_err(|error| self.command.clone().error(ValueValidation, error))?;

                Ok::<_, clap::Error>((channel, path))
            })
            .try_fold(AHashMap::new(), |mut map, pair| {
                let (channel, path) = pair?;

                if map.insert(channel, path).is_some() {
                    return Err(self
                        .command
                        .clone()
                        .error(ValueValidation, "overloaded channel"));
                }

                Ok(map)
            })?;

        let jobs = NonZeroUsize::from_str(matches.value_of("jobs").expect("missing jobs"))
            .expect("invalid jobs");

        let log_level = Level::from_str(matches.value_of("log_level").expect("missing log level"))
            .expect("invalid log level");

        Ok(Arguments {
            path,
            host,
            channels,
            jobs,
            log_level,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let ncpus = num_cpus::get();
    let arguments = Parser::new(&ncpus.to_string())
        .parse(env::args().into_iter())
        .map_err(|error| error.exit())
        .expect("unhandled error");

    tracing_subscriber::fmt()
        .with_max_level(arguments.log_level)
        .init();

    let channels = stream::iter(arguments.channels.into_iter())
        .map(|(channel, path)| async {
            let mut file = File::open(path).await?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).await?;
            Ok::<_, eyre::Error>((channel, Manifest::from_slice(&bytes)?))
        })
        .map(Ok)
        .try_buffer_unordered(arguments.jobs.get())
        .try_collect::<AHashMap<Channel, Manifest>>()
        .await?;

    let cache = Cache::new(arguments.path, arguments.host);
    cache
        .build(&channels, &Downloader::default(), arguments.jobs)
        .await?;

    info!("built cache");
    Ok(())
}
