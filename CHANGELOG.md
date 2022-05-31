# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- A mandatory argument for specifying the channel a manifest describes so that the correct cache
  metadata can be generated.
- Generation of channel metadata.
- An optional argument to configure where the cache will be hosted so that the correct cache
  metadata can be generated.
- Support for downloading multiple channels.
- Support for the optional `version` and `git_commit_hash` package data fields.

### Changed
- Updating a cache is now destructive and will prune untracked files and directories.
- The default number of parallel jobs is now generated based on hardware information.

### Removed
- Subcommands have been removed in favour of a single consistent behaviour.

## [1.0.0] - 2022-02-17
