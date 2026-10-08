# libSQL API for Rust

This directory publishes as **`do-harness-libsql` 0.9.30**, a maintenance fork
of crates.io `libsql` 0.9.30 for do-harness. It retains the upstream MIT license,
authors, API and Rust library name (`libsql`). The upstream manifest is retained
as `Cargo.toml.upstream`; package metadata records the upstream repository and
release. The maintained configuration is `default-features = false` plus `core`.

The source change makes `local::Connection::disconnect()` idempotent by clearing
the raw handle after closing it (upstream
[tursodatabase/libsql#2251](https://github.com/tursodatabase/libsql/issues/2251)).
Both upstream stable 0.9.30 and 0.10.0-pre.4 remain unfixed as of 2026-10-08.
The project pins a digest of the fixed connection source and checks normalized
packages and resolved registry dependencies before shipping. Remove this fork
only after validating an upstream release with the fix on musl and Windows.

The sections below are the original upstream README.

[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]

[crates-badge]: https://img.shields.io/crates/v/libsql.svg
[crates-url]: https://crates.io/crates/libsql
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/libsql/libsql/blob/main/LICENSE.md

This repository contains the libSQL API for Rust.

## Developing

See [DEVELOPING.md](DEVELOPING.md) for more information.

## License

This project is licensed under the [MIT license].

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in libSQL by you, shall be licensed as MIT, without any additional
terms or conditions.

[MIT license]: https://github.com/libsql/libsql/blob/main/LICENSE.md
