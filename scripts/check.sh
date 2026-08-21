#!/usr/bin/env bash
# Everything CI checks, in the order CI checks it.
#
# Exists because the two gates easiest to skip locally are the two that catch the most: clippy runs
# at pedantic with warnings denied, and the MSRV build uses a toolchain thirteen releases older
# than the one on a typical development machine. Both have caught real breakage that a plain
# `cargo test` waved through.
#
# Usage: scripts/check.sh [--skip-msrv]
set -euo pipefail

cd "$(dirname "$0")/.."

export RUSTFLAGS="-D warnings"
export CARGO_INCREMENTAL=0

msrv="$(grep -m1 '^rust-version' Cargo.toml | tr -d ' "' | cut -d= -f2)"
skip_msrv=false
[[ "${1:-}" == "--skip-msrv" ]] && skip_msrv=true

step() { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }

step "formatting"
cargo fmt --all -- --check

step "clippy"
cargo clippy --workspace --all-targets --all-features --locked

step "tests"
cargo test --workspace --all-features --locked

step "docs"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

if [[ "$skip_msrv" == true ]]; then
  step "msrv ($msrv) — skipped"
else
  step "msrv ($msrv)"
  if rustup toolchain list | grep -q "^$msrv"; then
    # rust-toolchain.toml pins stable, so the toolchain has to be named explicitly.
    # xtask is dev tooling and is allowed a newer toolchain; see its Cargo.toml.
    cargo "+$msrv" check --workspace --all-targets --exclude xtask --locked
  else
    echo "toolchain $msrv is not installed; run: rustup toolchain install $msrv --profile minimal"
    exit 1
  fi
fi

printf '\n\033[1;32mall checks passed\033[0m\n'
