# Accuracy across a library

What the pipeline reads on real material, measured over a sample of 50 titles spanning 1950 to 2025
rather than over the three discs the project had been tuned against.

Everything before this measured one of two things: a fixture this project rendered itself, which is
the ceiling case by construction, or a handful of Blu-rays chosen because they fit Arial. Neither
answers the question a user actually has — *what happens when I point this at my library?*

## What this can and cannot claim

**A release subtitle is not ground truth.** It is an independent transcript of the same dialogue,
produced by someone else, from a different release. It can rank two extractions of one track and it
can tell 3% from 30%. It cannot certify an absolute figure, and nothing here should be read as
doing so.

That caveat is not boilerplate. Three quarters of the way through this measurement it turned out
that most of what looked like error was the instrument, and the instrument had to be rebuilt twice
before the numbers below meant anything:

- **Releases do not share a timeline.** A sidecar authored against another cut sits whole seconds
  from the disc. `srt-score` paired cues within a 2 s window, so a sidecar running 1.9 s ahead
  stopped pairing partway through and a *near-perfect* read reported a CER above 100%. `--align`
  now searches a constant offset and the standard frame-rate ratios. Cinderella went from 60.5% to
  22.7% on that change alone.
- **Releases do not break dialogue into cues at the same places.** One carries "Yeah, I know" and
  "Miss, you have to stay calm" as two cues where another carries them as one. Pairing by time then
  compares a cue against half of its counterpart and charges correctly-read words as errors.
  Scoring the whole track as a single transcript removes this. On *Meet the Fockers* it is the
  difference between 43.1% and 22.1% — same extraction, same sidecar.
- **Releases disagree about SDH convention.** The disc writes `(SINGING)`; the sidecar writes
  `[singing]`. That divergence reads as a flood of `i→I`, `n→N`, `[→(` confusions attributable to
  nothing the matcher did.

The **track** row is therefore the headline, and the cue-level rows are kept because they are the
only ones that can separate upright from italic.

**Everything below was re-measured on 2026-08-24**, over the same fifty titles, with the pipeline
as it stands. The first run predated #121's word gaps, #130's colons and #168's quotation marks, and
it scored against a per-cue alignment #169 has since replaced. **45 of the 47 titles improved and
neither of the other two moved by half a point**; pooled track CER went from 13.56% to 12.88% and
WER from 24.99% to 22.62%, a median of 0.66 points per title.

Where that landed is the interesting part, and it is the italic row — see Table 1.

## Method

50 titles, drawn deterministically from the 823 in the library that carry both a bitmap subtitle
track and at least one English SRT sidecar, allocated across decades in proportion to how the
library is actually distributed and ordered within each decade by a hash of the folder name — so
the sample is not selected on year, size, codec, or on anything the pipeline does.

One reference set for every title: Arial regular plus its italic cut, the set
[`reference-set.md`](reference-set.md) selected. No per-title font fitting. That is the
configuration a user gets by default, and measuring it is the point.

Each title is scored against *every* English sidecar in its folder, and the best-agreeing one is
taken. Which release a rip's sidecar came from is unknown, and an SDH transcript scored against a
dialogue-only track differs for reasons the matcher has no part in. The spread between candidates is
reported below, because that spread is precisely the part of a title's score that belongs to the
sidecar rather than to us.

**47 of 50 scored.** The three exclusions are correct rather than failures: *The Martian* ships only
a *forced* English sidecar, and *Raya and the Last Dragon* and *Halloween Kills* only French ones.

## Table 1 — accuracy over the sample

Pooled over every scored character in 47 titles: 2.18 million characters and 403,000 words.

| | characters | CER | words | WER |
| :--- | ---: | ---: | ---: | ---: |
| **track** — one transcript, cue boundaries and timing ignored | 2,179,972 | **12.88%** | 403,277 | **22.62%** |
| cue-level, all | 1,923,974 | 21.87% | 367,718 | 32.26% |
| cue-level, upright | 1,819,552 | 21.85% | 347,559 | 32.20% |
| cue-level, italic | 104,422 | 22.18% | 20,159 | 33.32% |

The nine points between the first two rows are cue-boundary disagreement between releases, not
misreading.

**Italic is no longer the weaker style, and that is #121 arriving at corpus scale.** The first run
put italic 2.6 points of CER and **14.6 points of WER** behind upright; it is now 0.33 and **1.12**.
That fix — measuring a word gap between deskewed ink extents rather than between upright bounding
boxes — was justified on three discs, where it recovered an italic line's missing word spaces and
took one disc from 2.8% to 2.0% CER. Over 104,422 italic characters from 47 titles it closes the gap
outright. A slanted line is now read about as well as an upright one, which is the answer #14
wanted and did not have.

**Neither row is a clean split, and that was true when the gap was wide too.** The style is taken
from the release's `<i>` tags, and [`italic-slant.md`](italic-slant.md) found a title where **18% of
the lines lean and neither English sidecar marks a single one** — measured from the ink, without
opening a subtitle file. A release that loses the distinction puts its whole italic act in the
*upright* row, so both rows carry italic lines and the third of a point between them is an
underestimate of a gap that may not exist at all.

Per title, the distribution matters more than the mean — the sample is strongly bimodal:

| | p5 | p10 | p25 | **p50** | p75 | p90 | p95 |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| CER | 1.11% | 1.39% | 2.02% | **8.02%** | 22.13% | 29.85% | 35.48% |
| WER | 4.48% | 5.19% | 7.71% | **17.94%** | 32.85% | 45.69% | 58.33% |

| titles under | 2% CER | 5% CER | 10% CER | 20% CER |
| :--- | ---: | ---: | ---: | ---: |
| count | 12 (25.5%) | 21 (44.7%) | 26 (55.3%) | 34 (72.3%) |

The lower half moved and the upper half did not: **titles under 2% CER went from 5 to 12** while the
count under 20% is unchanged at 34. That is the shape a genuine improvement has against this
instrument — the titles whose sidecar agrees get better, and the ones whose sidecar is a different
transcript stay exactly where they were, because nothing about them was ever the pipeline.

**Where a title has more than one English sidecar to choose from, the median CER is 2.92%. Where it
has only one, it is 14.27%.**

| | titles | CER | WER | median | p90 |
| :--- | ---: | ---: | ---: | ---: | ---: |
| two or more sidecars, best taken | 26 | 10.19% | 18.01% | **2.92%** | 23.55% |
| one candidate only | 21 | 15.96% | 27.85% | 14.27% | 33.20% |

The spread between candidate sidecars *on the same title* — the same extraction, scored twice — has
a median of **13.38 points** and reaches 81. A title with one sidecar is scored against whatever it
happened to ship with, matched or not, and that is most of what separates the two rows.

The same conclusion arrives independently from the extractor's own diagnostics. Mean match distance
says whether the reference set fits the title's typeface at all:

| mean match distance | titles | median CER | median WER | median glyphs read |
| :--- | ---: | ---: | ---: | ---: |
| under 12 | 33 | **3.80%** | 14.29% | 99.90% |
| 12 – 14 | 8 | 9.65% | 14.76% | 99.75% |
| 14 – 16 | 2 | 22.34% | 41.23% | 98.50% |
| 16 – 20 | 1 | 14.79% | 34.74% | 99.90% |
| 20 and over | 3 | 22.94% | 60.51% | 90.10% |

A single Arial set fits 33 of 47 titles well, and those read at 3.80% median CER. **But 12 of 47
titles read more than 99% of their glyphs confidently, at a good fit, and still score over 15% CER**
— *Insomnia* at 77.3% with a fit of 10.3 and 99.9% of glyphs read. A title read confidently and
scored badly is evidence about two transcripts, not about the matcher. Eight of those twelve were
scored against an SDH sidecar, and §Table 2 takes *Insomnia* apart to show what the other kind looks
like.

**So the upper tail of this distribution is mostly the instrument.** The measurable degradation
attributable to the pipeline is the fit table above: real, bounded, and concentrated in the handful
of titles whose typeface Arial does not fit.

## Table 2 — per-character error rate

Rate is substitutions and deletions of a character over its occurrences in the release. Insertions
are not charged here: an inserted character is one the release never had, so it has no denominator.

The census behind this table is built from the **track-level** alignment now (#169) rather than
the cue-level one, so it no longer inherits the cue-boundary mispairing #116 measured. #119 asked
what that buys, and named the prize: whether the table could widen from the 20 titles the sidecar
corroborates to all 47, on 2.5× the material.

**It can be computed over all 47, and it should not be published that way.** The wider table is not
a per-character error rate; it is a portrait of three titles. That result is worth more than the
table would have been, and §"What the other 26 titles are made of" is the measurement.

### The 21 titles the sidecar corroborates

Titles whose track CER is under 5%. One more than the first run, and every figure in it recomputed.

| character | in release | wrong | rate | read instead as |
| :--- | ---: | ---: | ---: | :--- |
| `C` | 3,635 | 1,300 | **35.76%** | `c` 1273 |
| `]` | 2,162 | 364 | **16.84%** | `)` 285, `ì` 25, space 21 |
| `[` | 2,163 | 356 | **16.46%** | `(` 324, `í` 25 |
| `I` | 12,459 | 1,959 | **15.72%** | `l` 1802, `Í` 86, `i` 8 |
| `!` | 3,598 | 252 | **7.00%** | `Í` 179 |
| `l` | 28,001 | 1,269 | **4.53%** | `I` 1106, `Í` 77, `ì` 34 |
| `i` | 38,219 | 1,154 | **3.02%** | `Í` 558, unread 268, `ì` 146 |
| `f` | 10,382 | 311 | **3.00%** | unread 94, `í` 19, `d` 11 |
| `O` | 4,758 | 91 | **1.91%** | `o` 10 |
| `s` | 36,201 | 655 | **1.81%** | `S` 589, unread 32 |
| `A` | 6,515 | 109 | **1.67%** | unread 54, `a` 41 |
| `.` | 29,065 | 482 | **1.66%** | `-` 105, unread 87 |
| `U` | 2,133 | 33 | **1.55%** | |
| `R` | 4,606 | 69 | **1.50%** | `c` 11 |
| `-` | 6,308 | 82 | **1.30%** | |
| `r` | 34,228 | 437 | **1.28%** | `l` 13, `I` 10, `î` 8 |
| `B` | 2,319 | 27 | **1.16%** | `b` 18 |
| `H` | 4,488 | 49 | **1.09%** | |
| `:` | 1,573 | 17 | **1.08%** | `.` 5 |
| `c` | 13,242 | 142 | **1.07%** | `C` 113, unread 22 |
| `,` | 11,920 | 125 | **1.05%** | unread 52 |
| `W` | 4,771 | 48 | **1.01%** | `w` 37 |
| `T` | 6,191 | 54 | **0.87%** | `t` 24 |
| `y` | 21,260 | 168 | **0.79%** | unread 138, `Y` 8 |

### Two of the three total failures are gone

The first run's headline was three characters that failed *every single time*. Measured again on the
same corpus:

| character | first run | now |
| :--- | ---: | ---: |
| `:` | **100.00%** of 1,369 | **1.08%** — 17 wrong of 1,573, over the 19 of 21 titles that carry one |
| `"` | **100.00%** of 746 | **5.21%** — 40 wrong of 768 |
| `♪` | **100.00%** of 222 | 48.21% — 108 wrong of 224, and still the worst row on the corpus |

The colon was a component-assembly bug, not a matching one, and #130 found the cause was a single
threshold: stacked punctuation was allowed a gap of 200% of a mark's height, and a colon's dots sit
225–450% apart, so the rule written to hold a colon together had never once fired on one. The
quotation mark was two apostrophes, fixed by #168 on position rather than distance. Neither had
been measured at this scale before; both hold.

**`♪` is what is left, and it is still a coverage gap.** Its 108 failures are almost entirely
deletions — the reference set does not contain the character, so nothing can match it — and the
handful of substitutions go to `)`, `-` and `J`. It is cheap to fix and needs no new machinery.

### The multi-component characters that remain

`:`, `!`, `i` and `j` are the four characters in ordinary English text drawn as more than one
connected component. Two of them are now clean — `:` at 1.08% and `j` at 0.66% — and the other two
are not, in the same way:

- `!` → `Í`, 179 times of 252 failures;
- `i` → `Í`, 558 times of 1,154.

An `Í` is a capital `I` with a mark above it, which is exactly what a dot and a stem look like when
they are grouped as one component and matched as one glyph. So the remaining defect is not the
reference set and not the matcher; it is the same grouping decision #130 fixed for the colon,
reaching a mark that sits *above* a stem rather than beside a second dot.

### `C`/`c` is the cost of scale invariance

`C` reads as `c` 1,273 times, and `s`/`S`, `o`/`O`, `c`/`C`, `w`/`W` and `T`/`t` all show the same
confusion further down the table, in both directions. These are the letters whose upper and lower
case differ only in *size* — and §7's normalisation is scale-invariant by design, which is what lets
one reference set read a title at 21 px and another at 50 px.

The property that makes the matcher work across resolutions is the same property that makes these
pairs indistinguishable. That is a real trade-off rather than a bug, and resolving it needs the
line's x-height, not a better vector. [`glyph-hit-list.md`](glyph-hit-list.md) measures how much of
that height is available on real discs.

### What the other 26 titles are made of

The point of moving the census off the clock was to stop restricting it to titles whose sidecar
agrees — a restriction that is *circular*, since it selects titles by the outcome being measured.
So: group every substitution the census saw into families, and compare the three populations by rate
rather than by count.

| substitutions per 1,000 release characters | corroborated 21 | all 47 | 47 less *Insomnia* |
| :--- | ---: | ---: | ---: |
| case pairs (`C`/`c` and its kin) | 2.96 | 16.88 | 6.49 |
| **`I` / `l`** | **2.92** | **4.31** | **4.39** |
| brackets | 0.72 | 1.77 | 1.76 |
| punctuation | 0.47 | 4.81 | 4.11 |
| other | 1.72 | 17.03 | 13.74 |
| **every substitution** | **8.79** | **44.81** | **30.49** |

**`I`/`l` is the only family that survives the widening.** It moves by half; every other family
multiplies by two to ten, and `other` — substitutions between characters that share no shape, which
is what a *different transcript* looks like rather than a misread — multiplies by eight and becomes
the largest thing in the table. A matcher does not read `q` as `w`. Two releases do.

And most of the rest is not spread across 26 titles. It is two of them:

- ***Insomnia* is 62% of every case confusion in the corpus** — 22,985 of 36,792. Its sidecar is
  ALL CAPS from a WEBDL release and the disc is mixed case from a Blu-ray, so every capital in the
  transcript meets a lowercase letter that was read correctly. Both of its English sidecars are that
  release's, so there is no better choice to take. One title, 426 case substitutions per 1,000
  characters, against 2.96 in the corroborated set.
- ***Nosferatu* is 98–100% of `E`→`F`, `R`→`P` and `G`→`C`** — 2,308 substitutions, all of them the
  shape of a capital that has lost a stroke. That is a genuine misread rather than a transcript
  difference, and the extractor already says so without a sidecar: its fit is **19.4**, the worst of
  any title Arial won, in the band §Table 3 marks as not fitting.

Both are diagnosable, and neither is diagnosable *from the wide table* — they arrive there as `A`
failing 30% of the time and `E` failing 57%, which is a statement about no character in particular.

**So the restriction stays, and its justification changes.** It was "the census inherits the
cue-level alignment's faults"; that is no longer true. It is now this: the per-character census
measures the *difference between two transcripts*, and on a title where those transcripts disagree
about case, about SDH convention, or about which cut is being transcribed, the difference is not
about the matcher and cannot be made to be. Corroboration is a proxy for "the two transcripts are of
the same thing", and it remains the only proxy this instrument has.

What the widening did buy is the evidence above: the noise is now **named and attributed** rather
than assumed, and the one family that stands up to it is `I`/`l`.

## Table 3 — typeface matchability, measured without a sidecar

Everything above depends on a release subtitle, and §"What this can and cannot claim" is a long list
of the ways that dependence distorts a number. This table has no such dependence. `subtrackt fit`
scores a directory of candidate reference sets against a sample of 400 of the title's *own* glyphs
and reports mean match distance. Nothing in it reads a transcript, so it answers a strictly narrower
question — *can the matcher find a home for these shapes, and which typeface is it?* — and answers it
for all 50 titles including the three with no usable sidecar.

Twenty-one candidate sets, from the fonts on an ordinary Windows install.

| winner | titles |
| :--- | ---: |
| **arial-ri** (Arial regular + italic) | **46** |
| arialbd | 2 |
| verdanab | 1 |
| corbel | 1 |

| | p10 | p25 | **p50** | p75 | p90 |
| :--- | ---: | ---: | ---: | ---: | ---: |
| winner score (mean distance, 51-cell ceiling) | 10.48 | 11.00 | **11.50** | 12.17 | 14.60 |
| winner glyphs read | 98.79% | 99.10% | **99.40%** | 99.50% | 99.70% |
| margin to the next *typeface family* | 9.42 | 10.03 | **10.75** | 12.68 | 13.57 |

**47 of 50 titles separate their winner from the next typeface by more than 5 cells.** The library
really is one typeface, and the matcher establishes that on its own evidence. The same conclusion
falls out of the extractor's own report over the full tracks rather than a 400-cue sample: across
**1,887,369 glyph instances**, 0.71% went unmatched and 11.97% were ambiguous, at a median mean
distance of 11.40.

### Matchability does not predict CER

Correlating the fit score against the track CER of the same title, over the 47 with both:

| | value |
| :--- | ---: |
| Pearson *r* | **0.062** |
| Spearman *ρ* | **0.034** |

| fit score | titles | median CER | median WER |
| :--- | ---: | ---: | ---: |
| under 11 | 12 | 5.56% | 15.99% |
| 11 – 12 | 19 | 5.04% | 17.94% |
| 12 – 14 | 11 | **2.61%** | 12.65% |
| 14 and over | 5 | 14.79% | 34.74% |

Not merely weak — not even monotonic. **Within the range where Arial fits, which is 46 of 50 titles,
matchability carries no information about measured CER.** Matchability is uniformly excellent and
near-constant across the sample, so it cannot be what the CER spread is made of. That is the same
conclusion §"Table 1" reached from the sidecar-choice data, arriving independently from an
instrument that never opens an SRT.

### Where it *is* actionable, and where it lies

Three of the four non-Arial winners have a scored sidecar. Re-extracting each with the set `fit`
chose:

| title | fitted set | margin over arial-ri | CER before → after | WER before → after |
| :--- | :--- | ---: | ---: | ---: |
| Batman & Mr. Freeze: SubZero | arialbd | 25.6 | 46.28% → **17.98%** | 79.39% → 39.97% |
| Excision | arialbd | 21.7 | 22.94% → **11.43%** | 60.51% → 42.66% |
| Outland | corbel | **0.5** | 9.39% → **19.16%** ✗ | 24.21% → 56.31% |

**The margin is the whole signal.** Both wide-margin calls were right and worth 11 and 28 points of
CER. The one that made a title twice as bad won by half a cell — a tie, dressed as a decision. This
is exactly the failure [`reference-set.md`](reference-set.md) records for Liberation Sans: a
systematically wrong set is a *low*-distance set by construction, so a narrow win certifies nothing.

Adopting the fit winner only where it beats the default by more than 5 cells moves the corpus figure
from **12.88% to 12.47% CER** and 22.62% to 22.01% WER. Small, because it changes 2 titles of 47 —
and that smallness is the point. Per-title font selection is not what stands between this pipeline
and a good library-wide number.

Bold faces winning twice is worth noting on its own: `arialbd` beat Arial by more than 20 cells on
two titles, so **stroke weight, not letterform, is the axis a fixed set most often gets wrong.**

One title is an outlier on a different axis entirely: *Batman & Mr. Freeze* produced 2,158 distinct
shapes from 7,848 glyphs, against a sample median of 161. A 27% shape-to-glyph ratio means almost
every glyph binarised differently — the intra-character variance
[`library-survey.md`](library-survey.md) §"And yet a fixed set is not sufficient" describes, at its
worst. It is also the title the bold set rescued most.

## Runtime

All 50 titles were extracted — 268 GB of Matroska — and 47 of them scored. Four extractions ran in
parallel over SMB from a NAS. The per-title figures below are over the 47 that scored; the
throughput figures are over all 268 GB actually read.

| | first run | 2026-08-24 |
| :--- | ---: | ---: |
| Wall clock, whole sweep | 21.5 min | **18.7 min** |
| Mean per title | 94.1 s | 84.6 s |
| Median per title | 84.2 s | 80.1 s |
| p90 | 168.5 s | 138.6 s |
| Slowest title | 272.9 s | 221.8 s |
| Mean file size | 5.38 GB | 5.38 GB |
| Total CPU | 1.23 core-hours | 1.10 core-hours |
| Throughput, per worker | 61 MB/s | **176 MB/s** |
| Throughput, aggregate | ~207 MB/s | ~239 MB/s |
| Cues per second | 18 | 34 |

**The work is I/O-bound, not CPU-bound.** Time per title tracks file size and nothing else — the
demuxer reads the container to find subtitle packets, and a 10 GB remux costs proportionally more
than a 1.8 GB encode carrying the identical subtitle track. Four workers saturated the link, so
adding more would not help; a local disc would change this figure completely.

Per-worker throughput nearly tripled between the two runs and the aggregate barely moved, which is
what being link-bound looks like: #146 stopped the Matroska reader allocating a `Vec` per cluster
and each worker now waits on the network instead of on itself. `docs/cost-baseline.md` has that
measurement on its own.

## Reproducing

```console
$ cargo build --release -p subtrackt-cli -p xtask --features subtrackt-glyph/font
$ subtrackt gen-reference /path/to/arial.ttf arial-ri.subtref \
      --name arial-ri --italic /path/to/ariali.ttf
$ scripts/accuracy/sample.py --inventory inventory.json --out sample.json --count 50
$ scripts/accuracy/sweep.py --inventory sample.json --out results/ --reference arial-ri.subtref
$ scripts/accuracy/analyse.py results/
```

The sidecar-free table is a separate pass, and needs only a directory of candidate sets:

```console
$ subtrackt gen-reference /c/Windows/Fonts ./sets
$ scripts/accuracy/fit.py sample.json ./sets fit.json
```

The sweep keeps each extracted SRT, so re-scoring after a change to `srt-score` costs seconds rather
than another pass over the library. Sampling is deterministic: a re-run against the same library
picks the same fifty titles.

`xtask srt-score` gained four things for this measurement, all of them additive and off by default
so no figure published before it moved:

- **WER**, sharing one edit distance with CER so the two cannot drift apart;
- **per-character occurrence counts**, which turn the confusion census from counts into a rate;
- **`--align`**, which puts two releases on one timeline, choosing by *cues paired* rather than by
  score — an alignment chosen by score could buy a lower CER by sliding badly-read cues out of
  range;
- **`--json`**, so a sweep aggregates numbers rather than scraping a column layout.

## What this changes

1. **The colon was the single largest addressable defect on real material**, and it was a component
   assembly bug rather than a matching one. **Fixed in #130**, which found the cause was a single
   threshold: stacked punctuation was allowed a gap of 200% of a mark's height and a colon's dots
   sit 225–450% apart, so the rule written to hold a colon together had never once fired on one.
   The rerun above is the confirmation at scale: 100.00% of 1,369 colons wrong, then 1.08% of 1,573.
   The same grouping question is still open one mark up — `!` and `i` read as `Í` — which is what
   §"The multi-component characters that remain" is about.
2. **A fixed Arial set is good enough for most of the library** — 33 of 47 titles at 3.80% median
   CER — and mean match distance identifies the ones it is not good enough for, before any ground
   truth is consulted.
3. **The corpus method has a floor set by the sidecars, not by the pipeline.** Half of the apparent
   error at scale is release divergence. Any future run of this should quote the track row, take
   the best of several sidecars, and treat titles with only one as the weaker evidence they are.
4. **Italic is no longer a weaker style than upright**, and that is #121 measured on 47 titles
   instead of three. The word-error gap between them was 14.6 points and is 1.12.
5. **A per-character rate needs corroborated titles, and now for a stated reason.** #169 took the
   census off the clock and #119 asked whether that let the table widen to all 47. It does not: two
   titles supply most of the difference, one of them because its sidecar is ALL CAPS. The restriction
   survives its own re-examination, which is worth more than the wider table would have been.
