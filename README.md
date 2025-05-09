# cargo-easy-dep

[![Crates.io](https://img.shields.io/crates/v/cargo-easy-dep.svg)](https://crates.io/crates/cargo-easy-dep)
[![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/cargo-easy-dep.svg)](#license)
[![Rust: 2024 Edition](https://img.shields.io/badge/Rust-2024_Edition-orange.svg)](#rust-version)

A cargo subcommand that simplifies dependency management in Rust workspaces by automatically unifying common dependencies at the workspace level.

## Overview

In large Rust workspaces, managing dependencies across multiple crates can become cumbersome. `cargo-easy-dep` analyzes your workspace and:

1. Identifies dependencies used by multiple workspace members
2. Adds these common dependencies to the workspace's `Cargo.toml`
3. Updates each member's `Cargo.toml` to use the workspace dependency

This approach reduces duplication, simplifies updates, and ensures version consistency across your workspace.

## Installation

```bash
cargo install cargo-easy-dep
```

## Usage

From your workspace root directory, run:

```bash
cargo easy-dep
```

### Options

```
Options:
  -m, --min-occurrences <MIN_OCCURRENCES>
          Minimum number of occurrences to consider a dependency common [env: CARGO_EASY_DEP_MIN_OCCURRENCES=] [default: 2]

  -w, --workspace-root <WORKSPACE_ROOT>
          Path to workspace root (defaults to current directory) [env: CARGO_EASY_DEP_WORKSPACE_ROOT=]

  -q, --quiet
          Suppress all output [env: CARGO_EASY_DEP_QUIET=]

  -h, --help
          Print help

  -V, --version
          Print version
```

## Examples

### Basic Usage

```bash
# Run from your workspace root
cargo easy-dep
```

### Customize Minimum Occurrences

Consider all dependencies used by workspace members:

```bash
cargo easy-dep --min-occurrences 1
```

### Specify Workspace Root

```bash
cargo easy-dep --workspace-root /path/to/my/workspace
```

### Silent Mode

```bash
cargo easy-dep --quiet
```

## How It Works

1. Analyzes your workspace structure using `cargo_metadata`
2. Counts the occurrences of each dependency across workspace members
3. Identifies dependencies used by multiple crates (configurable via `--min-occurrences`)
4. Updates the root `Cargo.toml` to add these dependencies to the `[workspace.dependencies]` section
5. Updates each member's `Cargo.toml` to use `workspace = true` for these dependencies

## Notes

1. [The workspace default-features is problematic](https://github.com/rust-lang/cargo/issues/12162), so `cargo-easy-dep` will disable default-features for all dependencies in the workspace and enable them based on each member's preferences. This disabling has been done without diffing and it gives maintainers better visibility by explicitly indicating the features without falling into the mentioned issue.
2. Only the first dependency version seen will be used in the workspace dependencies; tune the version based on your requirements in the workspace `Cargo.toml`.
3. There is no difference between '^1.0' and '1.0' in versioning and as `cargo-easy-dep` uses `semver::VersionReq` as versioning strategy like what [`cargo`](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#caret-requirements) does, you will see '^1.0' in your root workspace `Cargo.toml` for dependencies versions. (`1.0` version used as an example)

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
