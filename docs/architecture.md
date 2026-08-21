# Architecture

Implementation notes for the design sketched in [issue #1][issue-1]. Read that first — this
document records how the sketch is laid out in code and which decisions are still open, it does not
restate the design.

[issue-1]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1

## The pipeline

```
 demux ──► decode ──► binarize ──► segment ──► vectorize ──► match ──► assemble ──► write
   │         │           │           │            │           │           │           │
  (done)    (done)     (done)      (done)      (done)      #9/#10    (done)/#12   (done)

Everything but matching is built. The pipeline reads a Blu-ray rip end to end and emits timed cues
with a confidence tally; what it cannot do is name the characters, because #9 has no reference set
worth embedding and #10 needs redesigning — see "What the measurements changed" below.
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
- **Matroska demuxing, complete**: native EBML parser, streaming, with zlib-compressed block
  payloads handled. Reads a 5.5 GB Blu-ray rip in 22 seconds over a network filesystem.
- **Text reconstruction, complete**: glyphs ordered by position within their line, word spacing
  derived per line from the median observed gap, unmatched glyphs rendered as a placeholder and
  counted. A leading dash keeps its space so a speaker marker does not read as a hyphen.
- **Glyph normalization, complete**: each glyph is cropped to its component box, letterboxed onto
  the 16-cell grid preserving aspect ratio, and thresholded at 50% area coverage. Coverage rather
  than point sampling, which is a deliberate departure from the architecture document — see below.
- **Line assignment and diacritic grouping, complete**: text lines from the mask row projection,
  marks attached to bodies above or below them, and stacked punctuation clustered on its own. What
  separates a diaeresis from a colon is not the marks — those are geometrically identical — but
  whether a full-height letter body sits beneath them.
- **Connected component labelling, complete**: two-pass with a union-find, 8-connected, with area
  and coverage filters. 8-way connectivity is deliberate — a diagonal stroke in a `V` is one pen
  movement, and a 4-connected pass would hand the matcher two half-glyphs.
- `FeatureVector`: 256-bit vectors, Hamming distance, cache keys.
- The reference-set scan, the ambiguity margin, and the session cache.
- The accuracy gate and the extraction report.
- SRT and WebVTT writers.
- The pipeline wiring, end to end.

Stubbed, each returning `Error::Unsupported` naming its issue: MP4 and MPEG-TS demuxing (#4, and
see below), and reference data (#9).

### What the library actually contains

A survey of the 1,328 titles carrying bitmap subtitles, which settled several open questions:

| Question | Answer |
| :--- | :--- |
| Container | **1,326 Matroska**, one `.m2ts`, one `.iso`. A native parser covers 99.8%; `ffmpeg-next` would buy 0.2% |
| Codec | 1,268 titles PGS, 60 VOBSUB (4%). PGS was the right thing to build first |
| Compression | **83% of PGS tracks are zlib-compressed** inside Matroska. Not a corner case |
| Worst-case track count | **70 tracks** in one file, not the 37 #1 assumed. 11 files exceed 37; 6,044 bitmap tracks in total |
| Era | 1950s–2020s, concentrated 1990s–2010s |
| Resolution | Overwhelmingly `Bluray-1080p`; 22 at 720p, 8 from DVD |

The track-count figure matters for #16: `MaxConcurrentExtractions` and `TimeoutMinutes` were sized
against an assumed worst case that is roughly half the real one.

**The pipeline is connected end to end**, and has been run against a real 5.5 GB Blu-ray rip:
1,111 cues, 35,516 glyphs, a 99% session-cache hit rate, in 22 seconds. Every glyph comes back
unmatched because the reference set is empty, so the structure is proven and the content is not —
that is #9, which waits on #8.

 Running the CLI over a `.sup` produces timed cues with a
confidence tally. Because the reference set ships empty, every glyph comes back unmatched, so the
default `FailTrack` policy refuses the track — which is the designed behaviour, not a failure.
`--on-unmatched placeholder` shows the cues and their timings. What stands between this and real
text is #9, and #9 waits on #8.

The reference set ships **empty**, deliberately. A guessed set is worse than none: a title in an
unlisted typeface would degrade to confident garbage rather than to a clean failure. #8 decides what
belongs in it.

### A hand-rolled Matroska reader, not symphonia

`symphonia-format-mkv` was proposed as a replacement for the reader in `subtrackt-demux::matroska`,
and the reasoning behind the proposal was sound: the hand-rolled parser shipped two serious bugs,
one of them a hang that meant a file never finished opening. A maintained demuxer has seen far more
files than 600 lines written here ever will.

It was evaluated at version 0.6.1 against real media from the library. Version 0.5 is not a
candidate at all — it has no subtitle codec IDs, so a PGS track is invisible to it.

**Where it agrees with us.** Track discovery is correct: `SubtitleCodecId(770)` is `HDMV_PGS`, with
the language attached. Packet count matched exactly — 2,222 on the test file, the same number our
reader and an independent Python walk both found. Timestamps matched to the millisecond.

**Why it was not adopted.**

| Finding | Consequence |
| :--- | :--- |
| `ContentCompAlgo` appears in its schema table but is never acted on; there is no inflate anywhere in the crate | 83% of PGS tracks in the library are zlib-compressed, so we would still write and own that code — which is where two of the three bugs actually were |
| `Track` does not expose the compression declaration | We would have to sniff the `0x78` zlib magic instead of reading what the file says |
| `Track` has no name field | Loses the track titles that make one of up to 70 tracks pickable |
| `next_packet()` returns every packet of every track | Materialises gigabytes of video into buffers that are immediately discarded |

The last point is the substantive one. Measured cold, on files neither tool had touched:

| | Work performed | Throughput |
| :--- | :--- | ---: |
| This reader | demux, inflate, PGS decode, segment, vectorize, match, write | **177 MB/s** (8.82 GB in 51s) |
| `symphonia-format-mkv` | demux only | **77 MB/s** (3.97 GB in 53s) |

Warm, both finish in about a second; the cold numbers are the ones that matter, since extraction is
a one-shot pass over a file nothing else has read. Measure cold or not at all — an early comparison
here was wrong by 50x because the page cache was holding the file.

`next_packet()` returning everything is the wrong shape of API for pulling one subtitle track out of
a large file. It is the same mistake `take_block` made before it was fixed to read a block's track
number before its payload.

**Revisit this if** the compression gap closes upstream, or if the reader starts failing on files
this library does not contain — that would be evidence the 600 lines are undercovering the format,
which is exactly the risk the proposal identified.

**Still on the table:** symphonia as a `dev-dependency` only, behind an opt-in ignored test that
cross-checks track discovery, packet counts and timestamps against real files. That buys an
independent oracle for demuxer regressions at no runtime cost, and does not require adopting it.

### Area coverage, not bilinear interpolation

The architecture document specifies bilinear interpolation for normalising a glyph onto the grid.
The implementation uses area coverage instead, and the reason is the direction of the resampling.

Bilinear is the right tool for magnifying. Here the usual case is the reverse — a 40px glyph
collapsing onto a 16-cell grid, a 2.5x reduction — and point-sampling a binary mask at that ratio
aliases, so whether a thin stroke survives depends on where the sample points happen to land. Each
cell instead measures the fraction of its source rectangle that is foreground, weighting partial
pixels. Area is preserved under scaling; stroke hits are not.

That is what makes the acceptance criterion hold: the same character rendered at 480p and at 1080p
lands within a tenth of the vector of itself, while two different characters stay several times
further apart. Both are asserted.

## What the measurements changed

Three questions §4 of #1 left open were measured against the real library rather than reasoned
about. Two are answered and both moved the design.

**[Typeface coverage](library-survey.md) (#8), which #1 calls "the whole risk".** A dominant glyph
family runs through the library — one cluster covers 43 of 56 sampled titles from 1950 to 2025 — so
the fixed reference set §2D wants is worth having. Fitting rendered fonts against the measured
shapes identifies the typeface as Arial or very close (`i` at distance 0, `t` at 6, `a` at 10). But
a fixed set covers only 46% of glyph instances.

**[Glyph stability](glyph-stability.md) (#14), which #1 calls "the first thing to measure".** The
reason for that 46%. Two renderings of the same character are typically further apart (median 46
cells) than two different characters are (median 31), and a one-pixel shift in the binarization edge
costs 30 cells on its own — as much as character identity. Rendering size and anti-aliasing cost 11
and 8, which independently confirms the normalisation in #7 absorbs the axes it was built for.

**The consequence for §2D.** One reference vector per character cannot work, and per-variant entries
do not rescue it. The session cache becomes the mechanism rather than an optimisation — the second
of the two outcomes §4 anticipated. It works because the expensive axes are constant *within* a
stream: one encoder, one palette, one typeface, so a title's own glyphs vary only along the cheap
axes and clustering them cancels exactly what defeats a fixed set.

That makes #10 a redesign rather than an implementation, and makes reducing edge sensitivity in
binarization the largest cheap-to-attack term left.

## Decisions still open

These are the §4 questions from #1, with where they live in the code.

| Question | Where | Issue |
| :--- | :--- | :--- |
| 16×16 versus 32×32 grid | `subtrackt_core::glyph::FEATURE_GRID` | #7 (measure again once #9 lands) |
| What happens to a cue with an unmatched glyph | `subtrackt::UnmatchedPolicy` | #13 |
| Session cache scope, and the redesign it now needs | `subtrackt-glyph::cache` | #10 |
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
