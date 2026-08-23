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

**These figures predate #121.** The run below was taken before word gaps were measured between
deskewed ink extents, which on three discs recovered most of an italic line's missing word spaces
and took one of them from 2.8% to 2.0% CER. The italic row in particular should be read as an upper
bound on today's error rather than a measurement of it; the corpus has not been re-scored.
[`italic-slant.md`](italic-slant.md) has what changed.

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
| **track** — one transcript, cue boundaries and timing ignored | 2,179,972 | **13.56%** | 403,277 | **24.99%** |
| cue-level, all | 1,923,974 | 22.55% | 367,718 | 34.52% |
| cue-level, upright | 1,819,552 | 22.41% | 347,559 | 33.71% |
| cue-level, italic | 104,422 | 25.04% | 20,159 | 48.34% |

The nine points between the first two rows are cue-boundary disagreement between releases, not
misreading. Italic remains the weaker style, and by more in words than in characters.

**The last two rows understate the gap, and by an unknown amount.** The style split is taken from
the release's `<i>` tags, and [`italic-slant.md`](italic-slant.md) found a title where **18% of the
lines lean and neither English sidecar marks a single one** — measured from the ink, without opening
a subtitle file. A release that loses the distinction puts its whole italic act in the *upright*
row. The direction of that error is certain even though its size is not.

Per title, the distribution matters more than the mean — the sample is strongly bimodal:

| | p5 | p10 | p25 | **p50** | p75 | p90 | p95 |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| CER | 1.63% | 2.05% | 3.25% | **8.21%** | 22.38% | 30.18% | 36.12% |
| WER | 6.53% | 7.52% | 11.74% | **20.61%** | 34.02% | 45.14% | 58.09% |

| titles under | 2% CER | 5% CER | 10% CER | 20% CER |
| :--- | ---: | ---: | ---: | ---: |
| count | 5 (10.6%) | 20 (42.6%) | 26 (55.3%) | 34 (72.3%) |

**Where a title has more than one English sidecar to choose from, the median CER is 3.63%. Where it
has only one, it is 15.15%.**

| | titles | CER | WER | median | p90 |
| :--- | ---: | ---: | ---: | ---: | ---: |
| two or more sidecars, best taken | 26 | 10.99% | 20.97% | **3.63%** | 24.20% |
| one candidate only | 21 | 16.50% | 29.55% | 15.15% | 33.28% |

The spread between candidate sidecars *on the same title* — the same extraction, scored twice — has
a median of **13.13 points** and reaches 81. A title with one sidecar is scored against whatever it
happened to ship with, matched or not, and that is most of what separates the two rows.

The same conclusion arrives independently from the extractor's own diagnostics. Mean match distance
says whether the reference set fits the title's typeface at all:

| mean match distance | titles | median CER | median WER | median glyphs read |
| :--- | ---: | ---: | ---: | ---: |
| under 12 | 34 | **4.13%** | 17.09% | 99.70% |
| 12 – 14 | 7 | 15.15% | 20.61% | 99.50% |
| 14 – 16 | 2 | 22.58% | 39.37% | 98.45% |
| 16 – 20 | 1 | 15.73% | 36.25% | 99.60% |
| 20 and over | 3 | 22.94% | 60.46% | 89.90% |

A single Arial set fits 34 of 47 titles well, and those read at 4.13% median CER. **But 12 of 47
titles read more than 99% of their glyphs confidently, at a good fit, and still score over 15% CER**
— *Insomnia* at 77.6% with a fit of 10.3 and 99.6% of glyphs read. A title read confidently and
scored badly is evidence about two transcripts, not about the matcher. Ten of those twelve were
scored against an SDH sidecar.

**So the upper tail of this distribution is mostly the instrument.** The measurable degradation
attributable to the pipeline is the fit table above: real, bounded, and concentrated in the handful
of titles whose typeface Arial does not fit.

## Table 2 — per-character error rate

Restricted to the 20 titles the sidecar corroborates (track CER under 5%), because the per-character
census is built from the cue-level alignment and inherits its faults: where the sidecar transcribes
something different, the aligner pairs characters that were never meant to correspond. This is a
conditional question — *among titles where the transcript agrees, which characters still fail?* — and
it is the only form of the question this instrument can answer.

Rate is substitutions and deletions of a character over its occurrences in the release. Insertions
are not charged here: an inserted character is one the release never had, so it has no denominator.

| character | in release | wrong | rate | read instead as |
| :--- | ---: | ---: | ---: | :--- |
| `:` | 1,369 | 1,369 | **100.00%** | `.` 1343, unread 7 |
| `"` | 746 | 746 | **100.00%** | `'` 732 |
| `♪` | 222 | 222 | **100.00%** | unread 76, `7` 32 |
| `C` | 3,363 | 1,041 | 30.95% | `c` 1016 |
| `I` | 11,627 | 1,973 | 16.97% | `l` 1681 |
| `!` | 3,402 | 231 | 6.79% | `Í` 174 |
| `l` | 26,870 | 1,417 | 5.27% | `I` 1099 |
| `f` | 9,875 | 375 | 3.80% | unread 85 |
| space | 145,339 | 5,435 | 3.74% | `l` 115, unread 43 |
| `i` | 36,625 | 1,358 | 3.71% | `Í` 531, unread 272 |
| `s` | 34,569 | 869 | 2.51% | `S` 576 |
| `.` | 27,733 | 592 | 2.13% | `-` 105 |
| `A` | 6,044 | 128 | 2.12% | unread 54, `a` 43 |
| `O` | 4,470 | 87 | 1.95% | `o` 13 |
| `c` | 12,717 | 242 | 1.90% | `C` 113 |
| `r` | 32,651 | 607 | 1.86% | `l` 16 |
| `B` | 2,173 | 36 | 1.66% | `b` 18 |
| `,` | 11,550 | 177 | 1.53% | unread 46 |
| `j` | 1,149 | 17 | 1.48% | unread 7 |
| `t` | 55,524 | 726 | 1.31% | `d` 123 |
| `?` | 5,359 | 32 | 0.60% | `.` 15 |

Three characters fail *every single time*, and they are the most actionable result here.

### The colon is never read. It is read as two characters.

`ERIK:` comes out as `ERIK<unread>.` — on all 1,369 colons, in every title that has any. Across
twelve well-corroborated titles the releases carry between 39 and 279 colons each and **the pipeline
emits exactly zero**.

The colon's two dots are being segmented as two connected components: the lower dot matches `.`, and
the upper dot matches nothing. So each colon costs two errors, a substitution and an insertion, not
one.

This is the same shape of failure as `!` → `Í` and `i` → `Í`: **multi-component characters are being
assembled wrongly.** `:`, `!`, `i` and `j` are the four characters in ordinary English text made of
more than one connected component, and all four head the table or carry unread counts. Nothing in
the reference set is at fault; the components are being grouped before they are matched.

Colons are frequent in exactly the material a user is most likely to feed this — SDH tracks, where
every speaker label ends in one.

### The other two are coverage, not assembly

`"` is read as `'` 732 times of 746, and `♪` matches nothing at all. Neither is a subtle failure:
one is a straight-versus-curly quote question and the other is a character the reference set never
contained. Both are cheap to fix and neither needs new machinery.

### `C`/`c` is the cost of scale invariance

`C` reads as `c` 1,016 times, and `s`/`S`, `o`/`O`, `u`/`U`, `v`/`V`, `w`/`W` all show the same
confusion in both directions further down the table. These are the letters whose upper and lower
case differ only in *size* — and §7's normalisation is scale-invariant by design, which is what lets
one reference set read a title at 21 px and another at 50 px.

The property that makes the matcher work across resolutions is the same property that makes these
pairs indistinguishable. That is a real trade-off rather than a bug, and resolving it needs the
line's x-height, not a better vector.

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
| Pearson *r* | **0.063** |
| Spearman *ρ* | **0.044** |

| fit score | titles | median CER | median WER |
| :--- | ---: | ---: | ---: |
| under 11 | 12 | 6.12% | 18.79% |
| 11 – 12 | 19 | 6.11% | 19.82% |
| 12 – 14 | 11 | **3.74%** | 17.76% |
| 14 and over | 5 | 15.73% | 36.25% |

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
| Batman & Mr. Freeze: SubZero | arialbd | 25.6 | 45.90% → **17.81%** | 77.72% → 39.23% |
| Excision | arialbd | 21.7 | 22.94% → **11.40%** | 60.46% → 42.67% |
| Outland | corbel | **0.5** | 9.42% → **19.14%** ✗ | 24.38% → 56.29% |

**The margin is the whole signal.** Both wide-margin calls were right and worth 11 and 28 points of
CER. The one that made a title twice as bad won by half a cell — a tie, dressed as a decision. This
is exactly the failure [`reference-set.md`](reference-set.md) records for Liberation Sans: a
systematically wrong set is a *low*-distance set by construction, so a narrow win certifies nothing.

Adopting the fit winner only where it beats the default by more than 5 cells moves the corpus figure
from **13.56% to 13.15% CER** and 24.99% to 24.39% WER. Small, because it changes 2 titles of 47 —
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

| | |
| :--- | ---: |
| Wall clock, whole sweep | **21.5 min** |
| Mean per title | 94.1 s |
| Median per title | 84.2 s |
| p90 | 168.5 s |
| Slowest title | 272.9 s |
| Mean file size | 5.38 GB |
| Total CPU | 1.23 core-hours |
| Throughput, per worker | 61 MB/s |
| Throughput, aggregate | ~207 MB/s |
| Cues per second | 18 |

**The work is I/O-bound, not CPU-bound.** Time per title tracks file size and nothing else — the
demuxer reads the container to find subtitle packets, and a 10 GB remux costs proportionally more
than a 1.8 GB encode carrying the identical subtitle track. Four workers saturated the link, so
adding more would not help; a local disc would change this figure completely.

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

1. **The colon is the single largest addressable defect on real material** and it is a component
   assembly bug, not a matching one. **Fixed in #130**, which found the cause was a single
   threshold: stacked punctuation was allowed a gap of 200% of a mark's height and a colon's dots
   sit 225–450% apart, so the rule written to hold a colon together had never once fired on one.
   Three SDH discs recover 309 colons with no other line changed. This paragraph's claim that it is
   invisible to `xtask accuracy` was wrong — the fixture *does* carry two colons, and closing them
   took it from 1.2% CER to 0.0%. What it is invisible to is the three-disc bench, whose tracks are
   all non-SDH.
2. **A fixed Arial set is good enough for most of the library** — 34 of 47 titles at 4.13% median
   CER — and mean match distance identifies the ones it is not good enough for, before any ground
   truth is consulted.
3. **The corpus method has a floor set by the sidecars, not by the pipeline.** Half of the apparent
   error at scale is release divergence. Any future run of this should quote the track row, take
   the best of several sidecars, and treat titles with only one as the weaker evidence they are.
