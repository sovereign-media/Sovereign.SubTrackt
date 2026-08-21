# Architecture

Implementation notes for the design sketched in [issue #1][issue-1]. Read that first — this
document records how the sketch is laid out in code and which decisions are still open, it does not
restate the design.

[issue-1]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1

## The pipeline

```
 demux ──► decode ──► binarize ──► segment ──► vectorize ──► match ──► assemble ──► write
   │         │           │           │            │           │           │           │
 #4 (+.sup) #2 #3       #5          #5 #6        #7        #9 #10       #11 #12    (done)
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
- PGS `.sup` segment reading, and PGS segment framing with malformed-packet detection.
- VOBSUB `.idx` index parsing.
- Alpha-based binarization and row/column projections.
- `FeatureVector`: 256-bit vectors, Hamming distance, cache keys.
- The reference-set scan, the ambiguity margin, and the session cache.
- The accuracy gate and the extraction report.
- SRT and WebVTT writers.
- The pipeline wiring, end to end.

Stubbed, each returning `Error::Unsupported` naming its issue: PGS RLE and composition (#2), all of
VOBSUB decoding (#3), container demuxing (#4), connected components (#5), diacritic grouping (#6),
feature vectoring (#7), reference data (#9), text layout (#11).

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

## Conventions

- Every stub returns `Error::Unsupported { issue }` rather than `todo!()`. A panic in a media
  pipeline is a crashed worker; an error is a fallback to burn-in.
- Thresholds are fractions, never pixel counts or raw cell counts. The same title ships at several
  resolutions, and `FEATURE_GRID` may yet change.
- Test names state the property under test, not the function under test.
- `unsafe` is forbidden workspace-wide. If the SIMD work in #10 needs it, that is a decision with a
  justification, taken then.
