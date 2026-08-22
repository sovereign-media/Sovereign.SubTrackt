#!/usr/bin/env bash
# Everything CI checks, in the order CI checks it.
#
# Exists because the gate easiest to skip locally is the one that catches the most: clippy runs at
# pedantic with warnings denied, and has caught real breakage that a plain `cargo test` waved
# through.
#
# Usage: scripts/check.sh
set -euo pipefail

cd "$(dirname "$0")/.."

export RUSTFLAGS="-D warnings"
export CARGO_INCREMENTAL=0

step() { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }

step "formatting"
cargo fmt --all -- --check

step "clippy"
cargo clippy --workspace --all-targets --all-features --locked

step "tests"
cargo test --workspace --all-features --locked

step "docs"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

printf '\n\033[1;32mall checks passed\033[0m\n'
