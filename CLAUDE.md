# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## About

This is a personal fork of [arxanas/git-branchless](https://github.com/arxanas/git-branchless), a suite of tools for branchless Git workflows. The fork adds custom commands (`git advance`) and config options (`branchless.advance.auto`), and fixes worktree support for bare repos. Master is unstable and may be force-pushed to track upstream.

## Build & Development Commands

```bash
# Build
cargo build --workspace

# Install locally (this fork)
cargo install --path=git-branchless

# Lint
cargo fmt --all
cargo clippy --workspace --all-features --all-targets -- --deny warnings

# Test (all)
cargo test --all-features --examples --tests --workspace --no-fail-fast
cargo test --all-features --doc --workspace --no-fail-fast

# Test (single file / single test)
cargo test --test test_advance                  # all tests in a test file
cargo test test_advance_basic                   # single test by name

# Docs
cargo doc --workspace --no-deps
```

Rust toolchain: 1.88 (edition 2024). See `rust-toolchain.toml`.

## Architecture

**Workspace layout** — 16 crates in a Cargo workspace:

- `git-branchless` — Main binary. Entry point at `src/main.rs`, command dispatch in `src/commands/mod.rs`. Commands like `advance`, `amend`, `hide`, `restack`, `split`, `sync`, `wrap` live here.
- `git-branchless-lib` — Core library. Contains:
  - `core/` — DAG operations (`dag.rs`), effects system (`effects.rs`), event log for undo (`eventlog.rs`), commit rewriting/rebasing (`rewrite/`), config, formatting, GC
  - `git/` — Git abstractions: repo, oid, reference, diff, index, tree, status, snapshots
  - `testing.rs` — Test utilities (`make_git()`, `Git`, `GitRunOptions`, PTY helpers)
- `git-branchless-opts` — CLI argument definitions (clap). Defines the `Command` enum that routes to all subcommands.
- `git-branchless-invoke` — Command invocation framework, provides `CommandContext` (effects + git_run_info)
- **Feature crates** (each owns a subcommand): `git-branchless-hook`, `git-branchless-init`, `git-branchless-move`, `git-branchless-navigation`, `git-branchless-query`, `git-branchless-record`, `git-branchless-revset`, `git-branchless-reword`, `git-branchless-smartlog`, `git-branchless-submit`, `git-branchless-test`, `git-branchless-undo`
- `scm-bisect` — Standalone bisect utility

**Key patterns:**

- **Effects system** — I/O is abstracted through an `Effects` struct, passed to all command handlers
- **Event log** — All repo mutations are logged, enabling `git undo` to reconstruct history
- **In-memory rebasing** — Primary rebase strategy operates without touching the working copy; falls back to on-disk when needed
- **Revsets** — Query language for selecting commits, parsed in `git-branchless-revset`, evaluated in `git-branchless-query`
- **Error handling** — Uses `eyre`/`color-eyre` throughout; commands return `EyreExitOr<()>`

## Testing

Tests are in `git-branchless/tests/test_*.rs`. They use:

- **`insta`** for snapshot testing (output comparison with `insta::assert_snapshot!`)
- **`make_git()`** from `lib::testing` to create temporary Git repos for each test
- **PTY helpers** (`run_in_pty`, `PtyAction`) for testing interactive TUI commands (undo)
- **`assert_cmd`** for CLI integration testing

Typical test pattern:
```rust
#[test]
fn test_example() -> eyre::Result<()> {
    let git = make_git()?;
    git.init_repo()?;
    // ... set up commits, run commands ...
    let stdout = git.smartlog()?;
    insta::assert_snapshot!(stdout, @"expected output");
    Ok(())
}
```

Environment variables for testing: `TEST_GIT` and `TEST_GIT_EXEC_PATH` to specify a custom Git binary.
