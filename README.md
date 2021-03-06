# rustdown

*rustdown* is a command line program that can be used to cache Rust toolchains distributed using
[Rust Release Channels](https://forge.rust-lang.org/infra/channel-layout.html). *rustdown* is
modular and unopinionated by design and can be composed with other tools to provide a fully-fledged
offline Rust development environment.

## Features

- Recovers from file system and network failures
- Supports official and alternative release channels

## Usage

*rustdown* requires an output directory along with a release channel manifest file. Manifest files
for the official toolchain (as distributed by [rustup](https://rustup.rs/)) can be found at
<https://static.rust-lang.org> at the paths [described in the
documentation](https://forge.rust-lang.org/infra/channel-layout.html#channel-manifests). However,
*rustdown* is unopinionated and will support any manifest in a compatible format.

```
$ rustdown --manifest /path/to/manifest stable:1.60.0 /path/to/cache
```

Temporary file system errors (eg. not enough disk space) or network failures (eg. internet outages)
are recoverable by running the command again until it's successful.

### Mirroring

The contents of the cache can by hosted by any static web server.

*rustup* [describes a series of environment
variables](https://rust-lang.github.io/rustup/environment-variables.html) that can be set to
redirect *rustup* requests to the mirror.

## License

[GPL version 3](https://www.gnu.org/licenses/gpl-3.0.en.html) or later
