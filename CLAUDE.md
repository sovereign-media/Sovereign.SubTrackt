# Working rules

Conventions this project already runs on. They are written down because each one has already earned
its place — most of them by something breaking when they were not followed.

A fuller set, derived from a scan of the codebase, belongs here once the pipeline is complete. This
is the interim.

## Before pushing

Run `scripts/check.sh`. Every time.

It runs what CI runs, in CI order: fmt, clippy, tests, docs. The gate easiest to skip is the one
that catches the most: **clippy** runs at pedantic with `-D warnings`, and a `cargo test` that
passes says nothing about it. It has already broken `main`.

There is no MSRV job. `rust-version` is declared so Cargo gives a clean error, but nothing enforces
it: releases are built here on stable and dropped into a container with no Rust toolchain, so the
floor had no consumer to protect and the job only ever tested a constraint it was itself the sole
source of. See #71.

## Landing work

One issue, one pull request. Open it, wait for checks, merge when green. Do not push to `main`
directly.

Commit messages explain *why*, not what — the diff already says what. Record decisions that a
reader would otherwise have to re-derive, and non-obvious behaviour the format or the domain forced.

## Dependencies

**Library crates take no dependencies.** `subtrackt-core` through `subtrackt` use only the standard
library. `clap`, `anyhow` and `tracing` live in `subtrackt-cli` and nowhere else.

This is not asceticism: #1 asks for a single static binary, and #16 leaves open whether the
deliverable is a CLI or a `cdylib` behind P/Invoke. A dependency-free core makes that a new crate
rather than an audit of a transitive tree. The cost is real and accepted — `Error` implements
`Display` by hand rather than deriving with `thiserror`.

Adding one to a library crate needs a reason that outweighs the above.

**One has.** `subtrackt-demux` takes `miniz_oxide` for zlib. A scan of the library found 83% of PGS
tracks stored zlib-compressed inside Matroska, so refusing the dependency meant failing on most of
the library. The alternative considered was hand-rolling inflate; that was the wrong call. DEFLATE
is not this project's problem domain, a subtle Huffman bug produces garbage bitmaps, and
`miniz_oxide` is pure Rust with no build script and one tiny dependency — so the single-binary and
cross-compilation goals the rule exists to protect are all intact.

The lesson worth keeping: the rule is "justify it", not "never". Reach for a crate when the work is
someone else's problem domain and the crate is pure Rust with a shallow tree.

## Failing

**Never invent data to avoid an error.** This is the project's whole thesis: a glyph matcher is
worth building over general OCR because an unmatched glyph is a *fact* rather than a confidence
score, and a fact is something a caller can act on.

Concretely:

- A truncated object is rejected, never padded with blank lines. A partially decoded subtitle reads
  as legitimately empty, and an empty subtitle is indistinguishable from one that had no text.
- Unimplemented stages return `Error::Unsupported { issue }` naming the tracking issue, never
  `todo!()`. A panic in a media pipeline is a crashed worker; an error is a fallback to burn-in.
- Malformed input names the presentation timestamp, so a failure can be located in the stream.
- Where an approximation is genuinely the lesser evil — the trailing cue of a stream that never
  clears — it is documented, uses a named constant, and is counted so it can be audited.

## Numbers

Thresholds are fractions of something measured, never absolute pixels or raw cell counts. The same
title ships at several resolutions, and `FEATURE_GRID` may yet change. A constant that works at
1080p and merges half a line at 480p is a bug waiting for a different disc.

## Tests

Test names state the property under test, not the function under test. `a_run_past_the_right_edge_is_rejected_rather_than_clipped`
tells a future reader what the code guarantees; `test_decode_3` does not.

Cover the failure modes, not just the happy path. Every codec parser needs truncated input,
over-long input and wrong-length input. Prefer a round trip over a hand-written expectation where an
encoder exists.

Pin surprising behaviour with a named test so the decision is visible rather than accidental —
8-connectivity fusing characters that touch at a corner, one display set becoming one cue.

## Accuracy

`cargo run -p xtask -- accuracy` generates a fixture and a reference set from the same font, runs
the pipeline, and scores the text against known ground truth. It is the only measurement in the
project that answers whether the *right characters* come out; everything else answers whether shapes
look alike, and those two diverge.

Treat its number as a ceiling. Fixture and reference share a font, so typeface mismatch is excluded
by construction and real material can only do worse. A change that improves coverage but worsens CER
has made things worse.

## Scope

Stages are traits in `subtrackt-core::stage`. No stage crate depends on another stage crate; the
graph is a fan with `subtrackt-core` at the hub. That is what lets an unimplemented stage be a stub
without its neighbours knowing, and it is worth preserving.

When a stage lands, the pipeline's boundary test moves to the next issue number. That test is the
honest measure of how far the pipeline actually reaches — keep it accurate.

## What needs real media

Almost nothing. Fixtures are generated in code, and `rle::encode` is public so decoders round-trip
against known bitmaps.

The exception is **#8**, the typeface survey, which asks what studios actually author against. No
synthetic fixture can answer that. It needs breadth rather than length: a few cues each from many
titles beats one full film. #14 needs font files, not media.
