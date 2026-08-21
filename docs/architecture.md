# Architecture

Implementation notes for the design sketched in [issue #1][issue-1]. Read that first — this
document records how the sketch is laid out in code and which decisions are still open, it does not
restate the design.

[issue-1]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1

## The pipeline

```
 demux ──► decode ──► binarize ──► segment ──► vectorize ──► match ──► assemble ──► write
   │         │           │           │            │           │           │           │
 #4 (+.sup) done/#3   (done)        #5 #6        #7        #9 #10       #11 #12    (done)
```

Each stage is a trait in `subtrackt-core::stage`, implemented in a stage crate, and wired together
by `subtrackt::Pipeline`. No stage crate depends on another stage crate — the dependency graph is a
fan with `subtrackt-core` at the hub. That is what lets an unimplemented stage be a stub returning
`Error::Unsupported { issue }` without any of its neighbours knowing.

## Crates

| Crate | Contains |
| :--- | :--- |
| `subtrackt-core` | Types every stage speaks: `Timestamp`, `IndexedBitmap`, `Palette`, `FeatureVector`, `Cue`, `Confidence`, `Error`, and the stage traits |
| `subtrackt-demux` | `.sup` reader, `.idx`/`.sub` reader, container reader (stub) |
| `subtrackt-decode` | PGS and VOBSUB packet decoders |
| `subtrackt-glyph` | Binarization, connected components, diacritic grouping, feature vectoring, the reference set, the matcher and the session cache |
| `subtrackt-text` | Layout reconstruction, post-correction, SRT and WebVTT writers |
| `subtrackt` | Pipeline orchestration, configuration, the accuracy gate, the report |
| `subtrackt-cli` | The `subtrackt` binary |

### Every library crate has zero dependencies

`subtrackt-core` through `subtrackt` depend on nothing outside the standard library. `clap`,
`anyhow` and `tracing` live in `subtrackt-cli` and nowhere else.

This is not asceticism. Issue #1 asks for a single static binary, and #16 leaves open whether the
deliverable is a CLI at all or a `cdylib` behind P/Invoke from `Sovereign.Media`. A dependency-free
library core means that decision costs a new crate rather than an audit of a transitive tree, and it
keeps cross-compilation to `linux/arm64` uneventful. The one place it costs something is
`Error`, whose `Display` and `Error` impls are written out by hand instead of derived with
`thiserror`.

The rule to hold: a dependency in a library crate needs a reason that outweighs the above. So far
none has.

## What is implemented

Complete and tested:

- Domain types, timestamp arithmetic, palette conversion, geometry.
- `Error`, including `Unsupported { issue }` — every stub names the issue that will replace it.
- PGS `.sup` segment reading.
- **PGS decoding, complete**: segment framing and body parsing, ODS reassembly across fragments,
  run-length coding both ways, incremental palette updates, object cropping, forced-subtitle flags,
  and display-set timing. Multiple composition objects composite into one image, because a top and
  bottom window belong to the same line of dialogue. Repeated compositions of identical pixels
  extend the open cue instead of emitting a new one, so a fade authored as twenty palette updates
  stays one cue rather than becoming twenty.
- VOBSUB `.idx` index parsing.
- Alpha-based binarization and row/column projections.
- `FeatureVector`: 256-bit vectors, Hamming distance, cache keys.
- The reference-set scan, the ambiguity margin, and the session cache.
- The accuracy gate and the extraction report.
- SRT and WebVTT writers.
- The pipeline wiring, end to end.

Stubbed, each returning `Error::Unsupported` naming its issue: all of VOBSUB decoding (#3),
container demuxing (#4), connected components (#5), diacritic grouping (#6), feature vectoring (#7),
reference data (#9), text layout (#11).

Running the CLI over a `.sup` now reaches #5 and stops there, which is the honest measure of how far
the pipeline gets.

The reference set ships **empty**, deliberately. A guessed set is worse than none: a title in an
unlisted typeface would degrade to confident garbage rather than to a clean failure. #8 decides what
belongs in it.

## Decisions still open

These are the §4 questions from #1, with where they live in the code.

| Question | Where | Issue |
| :--- | :--- | :--- |
| Which typefaces the reference set covers | `subtrackt-glyph::reference` | #8 |
| Whether one vector per character survives bold/italic/outline | `FEATURE_GRID`, `reference::Style` | #14 |
| 16×16 versus 32×32 grid | `subtrackt_core::glyph::FEATURE_GRID` | #7 |
| What happens to a cue with an unmatched glyph | `subtrackt::UnmatchedPolicy` | #13 |
| Session cache scope: stream, file or library | `subtrackt-glyph::cache` | #10 |
| Demuxer backend | `subtrackt-demux::container` | #4 |
| CLI versus `cdylib`, and where it runs | `subtrackt-cli` | #16 |

### The accuracy gate

`UnmatchedPolicy` has four variants — `Drop`, `Placeholder`, `FailTrack`, `Threshold { min_ratio }`
— and defaults to `FailTrack`. That default is a placeholder for a measurement, not a conclusion.

The gate is worth having because of a property this design has and a general OCR engine does not:
an unmatched glyph is a **fact**, not a confidence score. `Confidence` therefore counts glyphs
rather than estimating probabilities. Where those counts should live on the Sovereign side is the
open half of #13.

Both a per-cue and a track-level check exist, because "one unread glyph in a feature" and "40% of
the track unread" deserve different answers and only the second is visible at track level.

## Build times

Measured on the scaffold (~4k lines, 7 crates): clean workspace build 5.7s, no-op 0.03s,
rebuild after touching a leaf crate 1.4s, after touching `subtrackt-core` 1.8s.

The shape of the problem here is unusual and worth knowing before reaching for the standard advice.
The usual Rust CI caching guidance exists to avoid recompiling *dependencies* — but every library
crate in this workspace has none, and all 49 third-party crates in the lockfile are pulled in by
`subtrackt-cli` alone. `Swatinem/rust-cache` deliberately does not cache workspace-own crates, since
a stale one is worse than a slow build. So as this grows, the thing that grows is precisely the
thing dependency caching does not help with.

What is in place:

- **The crate split itself.** It is the biggest lever and it is already pulled: editing
  `subtrackt-text` does not rebuild `subtrackt-decode`. Keeping `subtrackt-core` small matters —
  everything depends on it, so a change there is the worst case.
- **`debug = "line-tables-only"`** in the dev profile. Full debug info was 36% of `target/debug`
  (259 MB down to 167 MB) and most of the link time, for information nobody reads. Backtraces still
  resolve to file and line.
- **`CARGO_INCREMENTAL: 0` in CI.** Incremental artifacts are never reused across fresh checkouts
  and roughly double what the cache carries.
- **`save-if` on main only.** Without it every PR branch writes its own cache and the repository's
  10 GB budget evicts the one branch everything restores from. This is the mistake that makes CI
  caching quietly stop working.
- **Type-check rather than release-build on pull requests.** A fully optimised cross-link of a
  binary nobody runs is the most expensive thing in the pipeline; `cargo check --target` still
  catches everything target-specific. Real builds happen on main and on tags.
- **`concurrency` cancellation.** Superseded commits stop building.
- **`sccache`.** The piece that closes the gap above: it keys each compilation unit by content, so
  workspace-own crates survive across runs too. Measured at a 100% hit rate on every job.

### What the CI numbers actually say

Per-job wall clock, before any of this work and after, on an unchanged commit:

| Job | Before | Warm | Cold |
| :--- | ---: | ---: | ---: |
| fmt, clippy, test | 40s | 22s | 50s |
| minimum supported Rust version | 30s | 24s | 38s |
| x86_64-unknown-linux-gnu | 51s | 35s | 75s |
| aarch64-unknown-linux-gnu | 65s | 54s | 80s |

Read the cold column before celebrating the warm one. A run that has to populate the cache is
*slower* than having no cache at all — setup, plus storing every artifact. At this size the warm
margin is real but modest, and it is fair to say sccache is not yet paying for itself on any single
run.

The reason to keep it anyway is what it caches rather than how much it saves today. Compilation of
our own crates is the cost that grows as the project does, and it is precisely the cost `rust-cache`
declines to cover. Setting this up now means the curve stays flat instead of being noticed at the
point where it hurts. If the hit rate ever drops — the per-job stats step prints it — that
assumption has broken and this should be reconsidered.

A cold run happens whenever the cache is empty or invalidated: a dependency change, a toolchain
bump, or eviction after 7 days idle.

Deliberately not done yet, with the trigger for revisiting:

| Tool | Why not yet | When to add |
| :--- | :--- | :--- |
| `cargo-nextest` | 116 tests execute in milliseconds; the runner is not the bottleneck | Test *execution* becomes visible against compile time, or flaky-test retries are wanted |
| `lld` / `mold` | Linking is not dominant with this little code and no C dependencies | A demuxer backend (#4) brings in native libraries |
| A separate `dist` profile | `release` should keep meaning "what we ship" | Release-build time on main becomes an obstacle |

The one to watch is `sccache`: if #4 lands `ffmpeg-next`, dependency build time stops being
negligible and the calculus changes.

## Checks before pushing

`scripts/check.sh` runs everything CI runs, in the same order: fmt, clippy, tests, docs, MSRV.

Run it. The two gates easiest to skip locally are the two that catch the most. Clippy runs at
pedantic with warnings denied, and the MSRV build uses a toolchain thirteen releases older than a
typical development machine — let-chains, for instance, compile happily on stable and are rejected
outright at 1.85. Both have already caught breakage that a plain `cargo test` waved through.

```console
$ rustup toolchain install 1.85 --profile minimal   # once
$ scripts/check.sh
```

## Conventions

- Every stub returns `Error::Unsupported { issue }` rather than `todo!()`. A panic in a media
  pipeline is a crashed worker; an error is a fallback to burn-in.
- Thresholds are fractions, never pixel counts or raw cell counts. The same title ships at several
  resolutions, and `FEATURE_GRID` may yet change.
- Test names state the property under test, not the function under test.
- `unsafe` is forbidden workspace-wide. If the SIMD work in #10 needs it, that is a decision with a
  justification, taken then.
