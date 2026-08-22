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

# CLAUDE.md's dependency rule, enforced rather than remembered. `subtrackt-glyph` gained an
# optional `fontdue` for #80, and the whole point of it being optional is that a consumer who does
# not opt in still gets the tree below. Feature unification makes that easy to break by accident
# from a *different* crate's manifest, which is exactly the kind of change nobody would look for.
step "dependency discipline"
expected="adler2 miniz_oxide"
actual=$(cargo tree -p subtrackt -e normal --prefix none --locked | awk '{print $1}' |
  grep -v '^subtrackt' | sort -u | xargs)
if [ "$actual" != "$expected" ]; then
  echo "the subtrackt library tree changed."
  echo "  expected: $expected"
  echo "  got:      $actual"
  echo "If that is deliberate, justify it in CLAUDE.md and update this list."
  exit 1
fi

step "docs"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

printf '\n\033[1;32mall checks passed\033[0m\n'
