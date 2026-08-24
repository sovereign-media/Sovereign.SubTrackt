# Architecture

Implementation notes for the design sketched in [issue #1][issue-1]. Read that first — this
document records how the sketch is laid out in code and which decisions are still open, it does not
restate the design.

[issue-1]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1

## The pipeline

```
 demux ──► decode ──► binarize ──► segment ──► vectorize ──► match ──► assemble ──► write
   │         │           │           │            │           │           │           │
  (done)    (done)     (done)      (done)      (done)      (done)      (done)      (done)
```

Every stage is built, and the pipeline reads a Blu-ray rip end to end. What it cannot do is name
characters *out of the box*, because nothing is embedded to match against — a decision rather than
a gap, and the reason is in [reference-set.md](reference-set.md). Given a reference set built from
the material's own typeface it reads a real disc at 0.6% character error and a library of 47 titles
at 12.88%; given one built from a near-identical typeface, it loses 11 points. #43 and #62 closed
that gap by fitting the set to the title instead of to the binary. #63 asked the part that survived
— can anything tell a good fit from a bad one without ground truth — and the answer is no, now
across six statistics and three distinct mechanisms. So the choice is reported to the user rather
than made by the tool, permanently rather than for now. See [fit-confidence.md](fit-confidence.md)
and [library-accuracy.md](library-accuracy.md).

Each stage is a trait in `subtrackt-core::stage`, implemented in a stage crate, and wired together
by `subtrackt::Pipeline`. No stage crate depends on another stage crate — the dependency graph is a
fan with `subtrackt-core` at the hub. That is what lets an unimplemented stage be a stub returning
`Error::Unsupported { issue }` without any of its neighbours knowing.

## Crates

| Crate | Contains |
| :--- | :--- |
| `subtrackt-core` | Types every stage speaks: `Timestamp`, `IndexedBitmap`, `Palette`, `FeatureVector`, `Cue`, `Confidence`, `Error`, and the stage traits |
| `subtrackt-demux` | `.sup` reader, `.idx`/`.sub` reader, native Matroska reader, native MPEG-TS reader; MP4 is a stub |
| `subtrackt-decode` | PGS and VOBSUB packet decoders |
| `subtrackt-glyph` | Binarization, connected components, diacritic grouping, feature vectoring, the reference set, the matcher and the session cache |
| `subtrackt-text` | Layout reconstruction, post-correction, SRT and WebVTT writers |
| `subtrackt` | Pipeline orchestration, configuration, the accuracy gate, the report |
| `subtrackt-cli` | The `subtrackt` binary |

### The library crates take one dependency, and it had to argue for itself

`subtrackt-core` through `subtrackt` depend on nothing outside the standard library except
`miniz_oxide`, which `subtrackt-demux` uses for zlib. `clap`, `anyhow`, `tracing` and the
`anstyle`/`anstream`/`anstyle-query` colour stack live in `subtrackt-cli` and nowhere else.

This is not asceticism. Issue #1 asks for a single static binary, and #16 left open whether the
deliverable was a CLI at all or a `cdylib` behind P/Invoke from `Sovereign.Media`. A near
dependency-free library core meant that decision cost a new crate rather than an audit of a
transitive tree, and it keeps cross-compilation to `linux/arm64` uneventful. The one place it costs
something is `Error`, whose `Display` and `Error` impls are written out by hand instead of derived
with `thiserror`.

**The rule is "justify it", not "never".** Two have. `miniz_oxide` because 83% of the library's PGS
tracks are zlib-compressed inside Matroska, so refusing it meant failing on most of the library and
hand-rolling inflate — not this project's problem domain, and a subtle Huffman bug produces garbage
bitmaps. `fontdue` in `subtrackt-glyph`, behind an **off-by-default `font` feature**, so a
downloaded binary can render its own reference sets (#80) while a consumer who does not opt in keeps
the tree above. Both are pure Rust with no build script.

Feature unification makes that second property breakable from a manifest that never mentions
`subtrackt-glyph`, so `scripts/check.sh` and CI both assert the `subtrackt` library tree is exactly
`adler2 miniz_oxide`. `CLAUDE.md` has the full reasoning.

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
- **VOBSUB decoding, complete**: control sequences, out-of-band palette, nibble RLE.
- **Glyph matching, complete**: linear scan by Hamming distance with a line-relative metric term,
  the runner-up margin that flags an ambiguous read, and the session cache.
- **Post-correction**: ambiguous reads resolved from the characters either side of them, and from
  the track's own vocabulary, off by default. [post-correction.md](post-correction.md) records the
  measurement behind that default.
- **Reference-set generation and fitting**: `gen-reference` renders a font — or a directory of them
  — through the same normalisation the runtime applies, carrying separate entries for italic and
  bold cuts; `fit` scores a directory of candidates against a title and proposes a winner.
  [reference-set.md](reference-set.md) and [fit-confidence.md](fit-confidence.md).
- **The ink aspect ratio on a reference entry**, read at the size subtitles are drawn at rather than
  at the outline's converged value. It is what separates `l` from `I`, which are the same 256-bit
  vector at the same height, and it halved a real disc's error rate. [error-census.md](error-census.md).
- **De-fusing**: a component the matcher cannot read is retried as two characters that touched, and
  the cut is kept only if every part reads. On by default; it recomputes a foreground mask only for
  images that failed, so it costs about 2.4 s on a 1.7 GB rip.
- **The line's own slant**, estimated per line from its ink and used twice: word gaps are measured
  between deskewed extents rather than between bounding boxes, and a leaning line is tagged `<i>` on
  the output. A regular-only set additionally *samples* along the slant, which switches itself off
  the moment the set carries an italic cut. [italic-slant.md](italic-slant.md).
- Colour, a spinner and a progress bar on stderr, all off when stderr is not a terminal.

Stubbed, returning `Error::Unsupported` naming its issue: MP4 demuxing (#86 landed MPEG-TS, and see
below). Nothing else is a stub — the empty reference set is a decision, not a placeholder.

### What the library actually contains

A survey of the 1,328 titles carrying bitmap subtitles, which settled several open questions:

| Question | Answer |
| :--- | :--- |
| Container | **1,326 Matroska**, one `.m2ts`, one `.iso`. Native parsers now read both of the first two, 99.92%; `ffmpeg-next` would have bought 0.08% |
| Codec | 1,268 titles PGS, 60 VOBSUB (4%). PGS was the right thing to build first |
| Compression | **83% of PGS tracks are zlib-compressed** inside Matroska. Not a corner case |
| Worst-case track count | **70 tracks** in one file, not the 37 #1 assumed. 11 files exceed 37; 6,044 bitmap tracks in total |
| Era | 1950s–2020s, concentrated 1990s–2010s |
| Resolution | Overwhelmingly `Bluray-1080p`; 22 at 720p, 8 from DVD |

The track-count figure matters for #16: `MaxConcurrentExtractions` and `TimeoutMinutes` were sized
against an assumed worst case that is roughly half the real one.

**The pipeline is connected end to end**, and has been run against a real 5.5 GB Blu-ray rip:
1,111 cues, 35,516 glyphs, a 99% session-cache hit rate, in 22 seconds.

Given a reference set it reads text; without one every glyph comes back unmatched and the default
floor refuses the track, naming the numbers behind it — which is the designed behaviour, not a
failure. `--version` says which reference data a binary carries, so a user seeing that can tell it
apart from a broken decoder.

The reference set ships **empty**, and #9 closed by measuring that this is the right answer rather
than a temporary one. A set built from a *near-identical* typeface — Liberation Sans against
Arial-authored material — costs 11 points of character error, which is Verdana's cost to within
noise, and neither coverage nor match distance detects it. Shipping one would trade a detectable
failure for an undetectable one. [reference-set.md](reference-set.md) has the table; #43 was the
answer, which is to fit the set to the title rather than to the binary.

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

That made #10 a redesign rather than an implementation. **It was redesigned, measured, and shipped
off**: `ClusterRules::default()` uses a radius of zero, because no radius exists that groups a
stream's variation without first merging characters the vector never separated — `I`, `l` and `|`
sit at distance *zero* from one another. Reducing edge sensitivity in binarization was likewise
tried and measured neutral-to-worse. Both write-ups are in [glyph-stability.md](glyph-stability.md).

**What did pay was aiming at separation rather than at variance.** Measuring each glyph against its
own text line — how tall it stands relative to that line's cap height, and how far it sits below the
baseline — took 5.8 to 8.1 points off the character error rate and is the first change to improve
the *distance between different characters* rather than the spread within one.

## Where §4's questions landed

This table used to be headed "decisions still open". Every one of them has now been measured and
closed, which is what closed #1 — kept here so the answers are findable from the code they live in.

| Question | Where | Answered by |
| :--- | :--- | :--- |
| 16×16 versus 32×32 grid | `subtrackt_core::glyph::FEATURE_GRID` | #7 — no consistent gain; 16 stays. `docs/glyph-stability.md` |
| Where a reference set comes from | `subtrackt_glyph::reference` | #9, then #43 — embed nothing, fit to the title. `docs/reference-set.md` |
| Word spacing | `subtrackt_text::layout::is_space` | #40 — the median-gap threshold missed half of them; #121 — measure the gap between *deskewed* ink. `docs/italic-slant.md` |
| CLI versus `cdylib`, and where it runs | `subtrackt-cli` | #16 — CLI. `docs/distribution.md` |
| A cue with an unmatched glyph | `subtrackt::config::UnmatchedPolicy` | #13, below |
| Session cache scope, and cluster-then-match | `subtrackt_glyph::cache` | #10 — clustering measured worse and ships off |
| Whether one vector per character survives style variation | — | #14 — it does not. `docs/glyph-stability.md` |
| What studios actually author against | — | #8 — PGS surveyed, VOBSUB not. `docs/library-survey.md` |

**One thing outlived the epic.** §4 warned that a title in an unexpected typeface "degrades to
garbage rather than to nothing, which is worse than the status quo". Fitting the set per title
answered which set to use; it did not answer how to know the fit was right. A mismatched set reads
~73% correct and ~27% confidently wrong with no counter saying which is which — the one place in
this pipeline where a failure is not a fact. That is **#63**, and it closed by measuring that
nothing can make it one: six statistics now, of which the fifth escaped the bias that killed the
first four and broke on decode noise instead, and the sixth — #101's language prior over the output
text — escaped both and failed for a third form of the first. The fitted set is a proposal the user
accepts rather than a decision the tool makes (#62), and that is now the permanent answer rather
than a placeholder. [fit-confidence.md](fit-confidence.md) has the six and the three mechanisms.

### The accuracy gate

Decided in #13. `UnmatchedPolicy` has four variants — `Drop`, `Placeholder`, `FailTrack`,
`Threshold { min_ratio }` — and defaults to **`Threshold { min_ratio: 0.90 }`**.

The gate is worth having because of a property this design has and a general OCR engine does not:
an unmatched glyph is a **fact**, not a confidence score. `Confidence` therefore counts glyphs
rather than estimating probabilities.

#### Why not `FailTrack`

It was the default and it rejects a track on a *single* unmatched glyph. The library survey scored
56 titles against a pooled reference set at the matcher's operational threshold:

| Coverage at ≤51 cells | Median | p10 | Worst | ≥90% | ≥50% |
| ---: | ---: | ---: | ---: | ---: | ---: |
| Fraction of glyph instances matched | 96.5% | 78.9% | 43.4% | 48/56 | 53/56 |

A median title at 96.5% carries hundreds of unmatched glyphs, so `FailTrack` refuses essentially
every track ever authored. That is not conservatism; it is a gate that never opens. `Threshold` is
the only variant these numbers support.

#### Why 0.90, and why not tighter

A floor against a track that could not be *read*, not a standard for one read *well*. Two
measurements bound it from opposite sides and they do not leave much room.

**From above: the pipeline's own ceiling.** A fixture read with a reference set built from the very
font that rendered it matched **93.9%** of its glyphs when this was chosen — the rest punctuation
the segmenter shattered, not typeface mismatch. Any floor above that refuses the best read this
pipeline can produce, which is precisely how `FailTrack` failed, one order of magnitude less
obviously.

**From below: the corpus.** 48 of 56 titles sit at or above 90%. That is the one value the survey
reports a title count for rather than one interpolated between its rows, which is worth more here
than a rounder-sounding number would be.

**Both bounds have since moved, and the floor has not been re-cut.** #99 and #110 took the ceiling
fixture to **99.7%** coverage — one unread glyph, half a colon — and a real disc to 99.5%, so the
upper bound is no longer anywhere near 0.90. [library-accuracy.md](library-accuracy.md) then
supplied the corpus re-survey #13 was waiting on: across 47 titles the median glyph read rate is
99.7% where the reference set fits and 89.9% where it does not, which is the first evidence that
0.90 sits at a real boundary rather than at a convenient one. Raising it is still not worth much,
for the reason the next paragraph gives.

Nine unread glyphs in a hundred is not good text, and 0.90 is not a claim that it is. It is the
point below which the burn-in fallback is unarguably the better answer.

It is deliberately not tuned finer, because **coverage is a weak predictor of correctness** and
fitting the figure would be fitting it to the wrong quantity. `docs/reference-set.md` measures this
directly against ground truth: a Segoe UI reference set matches the *same* 93.9% of glyphs as an
Arial one and reads at 37.8% character error against Arial's 15.9%. Coverage barely moved; accuracy
went 2.4×.

How weak, concretely. Running the five wrong-typeface sets from that write-up past this floor:

| reference set | coverage | 0.90 floor | CER |
| :--- | ---: | :--- | ---: |
| segoeui | 93.9% | **accepted** | 37.8% |
| verdana | 92.4% | **accepted** | 27.4% |
| LiberationSans | 89.3% | rejected | 26.8% |
| trebuc | 87.8% | rejected | 36.0% |
| tahoma | 84.0% | rejected | 29.3% |

The floor rejects the **best** of the five and accepts the **worst**. Nothing about that is a
tuning problem — a different value reorders which ones slip through without making coverage any more
informative about correctness.

So the gate catches tracks that could not be **read**. It does not catch tracks that were read
**wrongly**, and no count in `Confidence` can. Mean match distance (`Report::mean_match_distance`)
is the better signal for the second question and is now reported — Arial fits its own material at
13.0 cells against 20.9–22.7 for the wrong typefaces — but the same measurement shows it is not
sufficient either: a systematic substitution is by construction a *low*-distance one, so Liberation
Sans sits at 14.8 while reading as badly as Verdana at 21.7.

#### Per-cue or per-track: both, and they answer different questions

"One unread glyph in a feature" and "40% of the track unread" deserve different answers, and only
the second is visible at track level. So `Drop` and `Placeholder` act per cue as the track is built,
and the floor runs afterwards over the accumulated tally. A rejection returns
`Error::TrackRejected` carrying the policy, both counts and the floor, because the caller is being
told to fall back to burn-in and that is expensive enough to deserve a reason:

```
track rejected by the threshold gate: 0 of 131 glyphs read (0.0%), floor is 90.0%
```

#### Still open: where the counts live

Unchanged and not this repository's to decide. §4 of #1 notes the confidence tally has nowhere to
live in Sovereign's `ExtractedSubtitle`, and that is a schema question on the Sovereign side. It
shapes whether a partially-read track can be stored at all or whether the gate has to stay
all-or-nothing; the extractor produces the numbers either way.

## Build times

Measured on the scaffold (~4k lines, 7 crates): clean workspace build 5.7s, no-op 0.03s,
rebuild after touching a leaf crate 1.4s, after touching `subtrackt-core` 1.8s.

The shape of the problem here is unusual and worth knowing before reaching for the standard advice.
The usual Rust CI caching guidance exists to avoid recompiling *dependencies* — but the library
crates have one between them, and almost every third-party crate in the lockfile is pulled in by
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

The MSRV row is kept because it is part of the measurement; **that job no longer exists** — #71
retired it, for the reason at the end of this document. The matrix has since gained a Windows test
job, which these figures predate.

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
| `cargo-nextest` | 616 tests execute in milliseconds; the runner is not the bottleneck | Test *execution* becomes visible against compile time, or flaky-test retries are wanted |
| `lld` / `mold` | Linking is not dominant with this little code and no C dependencies | A demuxer backend (#86) brings in native libraries |
| A separate `dist` profile | `release` should keep meaning "what we ship" | Release-build time on main becomes an obstacle |

The one to watch is `sccache`: if #86 lands `ffmpeg-next`, dependency build time stops being
negligible and the calculus changes.

## Checks before pushing

`scripts/check.sh` runs everything CI runs, in the same order: fmt, clippy, tests, dependency
discipline, docs.

Run it. The gate easiest to skip locally is the one that catches the most: clippy runs at pedantic
with warnings denied and has already caught breakage that a plain `cargo test` waved through.

```console
$ scripts/check.sh
```

There is no MSRV job. `rust-version` is a declared floor rather than an enforced one — #71 retired
the job that checked it, because nothing downstream builds from source and the job's only possible
finding was a violation of a constraint it alone imposed.

## Conventions

- Every stub returns `Error::Unsupported { issue }` rather than `todo!()`. A panic in a media
  pipeline is a crashed worker; an error is a fallback to burn-in.
- Thresholds are fractions, never pixel counts or raw cell counts. The same title ships at several
  resolutions, and `FEATURE_GRID` may yet change.
- Test names state the property under test, not the function under test.
- `unsafe` is forbidden workspace-wide. #10 anticipated SIMD needing it; #10 shipped without any,
  and the session cache answers 99–100% of glyphs without a reference scan at all, so the case for
  it is weaker than when the rule was written. If it ever returns, it is a decision with a
  justification, taken then.
- **Never invent data to avoid an error.** A truncated object is rejected rather than padded, an
  unmeasurable line's slant is reported as unmeasurable rather than defaulted to zero, and a metric
  term is omitted rather than filled in when either side lacks it. `CLAUDE.md` has the reasoning:
  an unmatched glyph is a *fact*, and that is the whole argument for this over general OCR.
