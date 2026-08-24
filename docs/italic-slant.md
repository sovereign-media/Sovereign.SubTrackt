# The slant, and what it costs two stages apart

Answers the research half of [#115][issue-115]: the italic act is a third of the errors left on a
real Blu-ray, on 6% of its characters, and the issue proposes taking the slant out of a line before
the line is segmented.

Two things are measured here, and they are two stages apart. The **segmentation** half — a slanted
box overhangs the box after it, so the gap between them arrives at the spacing rule clamped to zero.
The **matching** half — [#14][issue-14] priced slant at 47 cells, the most expensive axis in
[`glyph-stability.md`](glyph-stability.md).

Both are confirmed, at very different sizes:

- **The gap mechanism is real and it is large.** 27% of an italic line's gaps arrive at the spacing
  rule clamped, against 0.7% of an upright line's — a factor of forty, replicated on three discs.
  The shipped rule then declines to place any space at all on **16%** of italic lines against 7.5%
  of upright ones, and measuring the gap between *deskewed* extents takes that to **4.8%**.
  [#121][issue-121] shipped that measurement: **Gone Girl went from 2.8% to 2.0% CER** and 618 of
  its 783 missed word spaces came back. See "What it was worth".
- **The matching half is worth 21 cells of the 47, and no shear buys more.** A sheared sampling
  takes the slant axis from 47 cells to 26, which is below the median distance to an entirely
  different character. It does not reach the 8-to-11 of the cheap axes, and a sweep proves the
  residual is not a bad angle: it is the characters a true italic **redraws** rather than leans.
  [#122][issue-122] then found what those 21 cells are worth on a disc, and it is *not* what the
  bench implied: a deskew and an italic reference cut are **alternatives**. Against a set with no
  italic entries the deskew takes Cloverfield's italic act from **47.1% to 8.1%**; against a set
  carrying #66's cut it makes it worse. See "The matching half, and what it is an alternative to".
- **Detection is nearly free.** One number per line separates the two acts at **99.4%** and
  **98.9%** per cue on the two discs whose release marks its italics, and the cues it gets wrong do
  not lean at all — the transcript and the disc disagree there, so this is close to the ceiling
  rather than a threshold in want of tuning. [#123][issue-123] shipped that as an `<i>` on the
  output, at 98.8% and 97.6% against a threshold nothing fitted, and **without moving CER by a
  character**.

[issue-14]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/14
[issue-40]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/40
[issue-48]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/48
[issue-49]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/49
[issue-66]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/66
[issue-97]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/97
[issue-113]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/113
[issue-115]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/115

## What the split actually is, re-measured

#115 was raised on arithmetic rather than a measurement: the post-[#113][issue-113] upright/italic
split was not printed anywhere, and the issue said so and asked for it. It is:

| 10 Cloverfield Lane, post-#113 | cues | characters | CER | words | WER |
| :--- | ---: | ---: | ---: | ---: | ---: |
| upright | 775 | 23,017 | **0.5%** | 4,565 | 2.1% |
| italic | 43 | 1,505 | **4.1%** | 263 | 24.7% |
| all | 818 | 24,522 | 0.7% | 4,828 | 3.4% |

**61 of the disc's 177 remaining errors are in the italic act — 34% of what is left, on 6.1% of the
characters.** 38 of them are a word space the assembler never placed, which is the largest single
error class on the disc. The issue's estimate was 62 errors and it was right to within one.

## The instrument

`xtask slant <media> <reference.subtref> <release.srt>` makes one pass over a disc and answers both
halves. It joins three things that already existed and were never joined: the extraction, which
knows when each cue is on screen; the glyph survey, which knows every component's box and ink; and
the release subtitle, which marks its italic cues. The join is the same one `xtask glyph-geometry`
makes, and it is asserted rather than assumed.

The slant estimate is #115's **candidate C** and it is here first for the reason the issue gives: a
Radon sweep hand-rolled in std is not free, and if a one-pass moment answers the question then the
sweep is bought for nothing. It does.

**The shear that stands a line upright is `k = Cxy / Cyy`**, pooled over the line's glyphs — the
shear that makes the ink's covariance cross term vanish, which is what "the stems now stand
vertical" means as an equation. Each glyph contributes its covariance about **its own** centroid.
Pooling the raw pixels instead would measure the line's *layout*: a row of letters is far wider than
it is tall, so its cross term is dominated by where the words sit and by a baseline that any
descender pulls off level. [#48][issue-48] read a diacritic's direction from the same second moment,
so this is a second reading of machinery that was already measured rather than new machinery.

It is a slope, so it is dimensionless and survives a resolution change without a cap height to
divide by — which is what `CLAUDE.md` requires of every threshold here. Its sign follows the plane's:
y grows downward, so an italic leaning right at the top reports a **negative** shear.

## The gap is clamped, and by a factor of forty

`layout.rs` measures the space between two glyphs as `next.x.saturating_sub(this.right())` — a gap
between **bounding boxes**. A slanted ascender's box is mostly slant, so it overhangs the box of the
letter after it and the subtraction saturates. [#40][issue-40]'s rule cuts at the widest jump between
consecutive sorted gaps, which needs the line's gaps to be bimodal; a run of clamped zeros collapses
the letter-gap mode onto the floor and takes the band with it.

| | lines | gaps | negative | zero | **clamped** | p50 | p90 |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Cloverfield, upright | 992 | 18,170 | 0.2% | 0.6% | **0.7%** | 6 | 20 |
| Cloverfield, italic | 63 | 1,213 | 15.3% | 12.0% | **27.4%** | 3 | 18 |
| Wanda, upright | 1,940 | 33,782 | 0.1% | 0.4% | **0.6%** | 6 | 21 |
| Wanda, italic | 85 | 1,372 | 11.4% | 11.7% | **23.1%** | 3 | 19 |

The **negative** column is the point and it is the column the runtime cannot see. Fifteen percent of
Cloverfield's italic gaps are not merely zero — the boxes genuinely overlap, and `saturating_sub` has
nowhere lower to go. As a fraction of the line's own median glyph width, an italic line's gaps run
from **-4%** at p10 to 0% at p25; an upright line's start at +8%.

### What it costs the rule

`split_threshold` returning `None` is the rule declining to place *any* space on the line — right
on a line holding one word, and on a line of dialogue it means the whole line arrives as one word.

| | lines | no cut | **deskewed** |
| :--- | ---: | ---: | ---: |
| Cloverfield, upright | 992 | 7.5% | 6.7% |
| Cloverfield, italic | 63 | **15.9%** | **4.8%** |
| Wanda, upright | 1,940 | 6.3% | 5.1% |
| Wanda, italic | 85 | **11.8%** | **4.7%** |

The **deskewed** column costs nothing to produce and is the remedy priced rather than built. Nothing
is shifted, sheared or resegmented: the line's own shear is applied to each glyph's ink to ask where
its leftmost and rightmost columns *would* stand once upright, and the gap is measured between those.
The result is then rounded and saturated exactly as the runtime's integer subtraction would be, so
the two columns differ in the gap that was measured and in nothing else.

**On both discs an italic line ends up better served than an upright one.** That is the shape of a
fix rather than a tuning: the rule was never broken, it was being handed a measurement that had been
destroyed before it arrived.

### Where the release lost the distinction, and the estimator did not

Both of Gone Girl's English sidecars contain **no `<i>` anywhere**. The release-labelled tables above
therefore have one column on that disc and can say nothing about the other — but the estimator never
opens a subtitle file, so the same split can be taken from the ink:

| split by the estimator | lines | clamped | no cut | **deskewed** |
| :--- | ---: | ---: | ---: | ---: |
| Cloverfield, upright | 981 | 0.7% | 6.2% | 6.1% |
| Cloverfield, leaning | 66 | 27.9% | 22.7% | 13.6% |
| **Gone Girl, upright** | 2,685 | 0.8% | 6.4% | 6.6% |
| **Gone Girl, leaning** | **592** | **28.3%** | **24.3%** | **4.4%** |
| Wanda, upright | 1,906 | 0.6% | 5.1% | 5.0% |
| Wanda, leaning | 95 | 22.1% | 11.6% | 6.3% |

**Gone Girl is 18% italic and its transcript says it is 0% italic.** The disc reads at 2.8% CER and
nothing in this repository could have told you which part of that was the italic act, because the
only instrument that splits by style is the one that reads the release's tags. The clamped rate on
those 592 lines — 28.3%, against 0.8% on the same disc's upright lines — is the estimator and the
gap measurement agreeing about a population neither of them was told existed.

This also corrects something, and the correction now runs the other way. When this was written,
[`library-accuracy.md`](library-accuracy.md) reported a cue-level italic CER of 25.04% against
22.41% upright over 47 titles. That corpus has since been re-extracted with the gaps measured as
described here, and the two styles are level: **22.18% italic against 21.85% upright**, and 33.32%
against 32.20% in words where the gap had been 14.6 points. This section is why.

The caveat on the split itself survives: it is taken from the release's tags, and a release that
marks none of its italics puts the whole italic act in the upright column. Gone Girl is the case in
point. So *both* columns above carry italic lines, and the remaining third of a point between them
is an underestimate of nothing in particular — which is a better problem than the one this
paragraph used to describe.

## The matching half: 21 cells of 47, and the rest is letterform

`xtask measure-stability --deskew` adds a deskewed reading of the italic faces. The shear is
estimated by the same moment, pooled over the whole charset at each rendering condition, and applied
to the **sampling** rather than to the pixels: each grid cell's preimage becomes a parallelogram, so
the glyph is never resampled and no interpolation or rounding stands between the ink and the grid.

That form was chosen before it was benched, for the reason #115 gives. The last three accuracy
findings in this repository — #99, #110 and #113 — were each one side of the pipeline quantising a
measurement the other side did not. A deskew that resampled a 33-pixel glyph and then normalised it
again would be the same mistake a fourth time, and the deskewed side is asked to match against the
*upright* reference vector, so the asymmetry would be exactly the one that has been costly before.

| movement from canonical, median cells | p5 | p25 | **p50** | p75 | p95 |
| :--- | ---: | ---: | ---: | ---: | ---: |
| anti-aliasing threshold | 1 | 3 | **8** | 18 | 35 |
| rendering size | 1 | 5 | **11** | 22 | 38 |
| outline / edge (1px) | 5 | 17 | **30** | 46 | 73 |
| weight (bold) | 11 | 27 | **38** | 54 | 81 |
| **slant (italic)** | 23 | 35 | **47** | 60 | 82 |
| **slant (italic), deskewed** | 4 | 15 | **26** | 40 | 65 |
| weight + slant | 28 | 43 | **55** | 69 | 93 |
| weight + slant, deskewed | 12 | 27 | **38** | 54 | 78 |

Slant stops being the most expensive axis and becomes the third, below bold's 38 and level with the
one-pixel edge shift's 30. The number worth holding onto is the comparison the same report makes on
the line above: **inter-character distance is 27 cells at p25.** A deskewed italic glyph sits, at the
median, closer to its own upright entry than a character sits to the nearest *different* character.
At 47 it did not.

### The residual is not a bad angle

"It moved the row" and "it moved the row as far as any shear could" are different claims, and only
the second lets a residual be read as a letterform. Sweeping a multiple of the estimate over the
same population:

| face | estimated shear | 0.00 | 0.50 | 0.75 | **1.00** | 1.15 | 1.25 | 1.50 | 2.00 |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Arial Italic | -0.173 | 47 | 35 | 29 | **26** | 26 | 27 | 31 | 45 |
| Arial Bold Italic | -0.177 | 54 | 44 | 40 | **38** | 38 | 39 | 42 | 53 |

The minimum is at the estimate. Arial Italic is drawn at 12 degrees and `tan 12°` is 0.213 — a
multiple of 1.23 — and shearing by the *design angle* is measurably worse than shearing by the
measured moment. A moment estimator is not obliged to agree with a design angle, and here it should
not: the estimator zeroes a cross term over the ink that is actually there, and a face whose round
letters were redrawn has ink no single shear stands upright.

The disc agrees with the font, which is the check that matters. Arial Italic's own renderings
estimate **-0.173**; the italic lines of two real Blu-rays estimate **-0.155** and **-0.160** at the
median. Same face, same number, one side rasterised here and the other thresholded by a studio a
decade ago.

### Which characters, which is #115's third prediction

Median distance from the upright vector, before and after:

| the shear helps least | | | the shear helps most | | |
| :--- | ---: | ---: | :--- | ---: | ---: |
| `a` | 49 → **43** | -6 | `r` | 32 → **18** | -14 |
| `e` | 49 → **41** | -8 | `f` | 29 → **14** | -15 |
| `m` | 65 → **40** | -25 | `t` | 33 → **14** | -19 |
| `s` | 49 → **40** | -9 | `J` | 35 → **11** | -24 |
| `R` | 71 → **39** | -32 | `L` | 44 → **11** | -33 |
| `W` | 88 → **38** | -50 | `j` | 22 → **10** | -12 |
| `n` | 60 → **37** | -23 | `1` | 37 → **9** | -28 |
| `M` | 71 → **35** | -36 | `i` | 21 → **7** | -14 |
| `Q` | 44 → **35** | -9 | `I` | 27 → **5** | -22 |
| `D` | 60 → **34** | -26 | `l` | 26 → **5** | -21 |

The right-hand column is a straight stem and nothing else, and a shear recovers it almost exactly:
`l` and `I` land 5 cells from their upright entries, which is inside the noise of the ink threshold.
The left-hand column is what Arial Italic **redraws** — the single-storey `a`, the different `e`, the
`m` and `n` with their arches re-cut — and 34 to 43 cells is above the 27-cell inter-character
distance, so those still need an italic entry to be read. [#66][issue-66]'s italic cut gets much
cheaper to justify and does not become removable, which is what the issue predicted.

## The bonus: telling the acts apart

Per cue, which is the granularity an `<i>` is written at, and pooling a cue's lines is free rather
than an assumption: #14 found slant constant within a stream.

| | measurable cues | italic | best cut | right | missed | false italic |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| Cloverfield | 810 | 43 | -7.9 | **99.4%** | 5 | 0 |
| Wanda | 1,323 | 59 | -6.9 | **98.9%** | 9 | 6 |

The distributions barely touch. An upright line reaches -2.7 at p10 on Cloverfield and -2.6 on
Wanda; an italic line reaches -12.3 and -10.6 at p75. The cut is chosen on the disc's own two
populations here and is therefore **not a shippable constant** — what it reports is the separability
the two populations have, which is the question a detector must clear before a threshold is worth
choosing at all. `reference-set.md` records the same distinction for a different measurement.

**Where it fails, it fails for a reason no threshold fixes.** The italic cues read upright sit
between -6.1 and -1.5 on Cloverfield and between -6.4 and +0.7 on Wanda: they do not lean. A release
marking a song lyric or a title card italic where the disc set it upright is the transcript and the
disc disagreeing, and moving the cut only trades those five for false italics elsewhere. 99.4% is
close to what this material allows rather than a number in want of tuning.

Lines that cannot be measured report **unknown** rather than upright — 0.8% of Cloverfield's upright
lines and none of its italic ones. That is the boundary `CLAUDE.md` requires: an unmeasurable line
and a line measured as upright are different facts, and only one of them may be written untagged.

## Predictions, scored

#115 recorded five before any of this ran.

- **1. The segmentation half is the larger win, and it is the half nothing else can reach.**
  *The mechanism is confirmed and the size is bounded, not yet cashed.* 27% of italic gaps arrive
  clamped against 0.7% upright, the rule refuses 16% of italic lines against 7.5%, and deskewed
  extents take that to 4.8%. Whether the 38 spaces come back needs the build. The half of the claim
  that is settled is that this is a failure **two stages before the matcher**, so no reference set
  and no #66-style italic cut could ever have reached it.
- **2. Deskewing collapses the slant axis toward the cheap axes.** *Half right.* 47 to 26, which is
  below the inter-character distance and below bold — but not to the 8-to-11 the prediction hoped
  for. The prediction's own escape clause said that if it did not fall, "the shear is wrong"; the
  sweep says the shear is not wrong, so the floor is the letterform.
- **3. The italic cut gets cheaper to justify but does not become removable.** *Right, and the list
  is now printed.* Ten characters fall to 5–18 cells and ten stay at 34–43.
- **4. Detection is far easier than correction, and should score above 95% per cue.** *Right.* 99.4%
  and 98.9%, and the misses are transcript disagreement rather than threshold placement.
- **5. Upright material does not move at all.** *Right, on the measurement that could be taken
  without building anything.* Deskewing every line on all three discs moves the upright refusal rate
  by 6.2 → 6.1, 6.4 → 6.6 and 5.1 → 5.0 points. The estimator reports a median shear of +0.6, -0.2
  and +1.0 on upright material, so it is reading slant and not something else. The end-to-end form of
  this prediction still needs the build and a CER on all three discs.

## What it was worth

[#121][issue-121] built the cheaper half of the remedy: nothing shears a bitmap, and every glyph
carries where its ink *would* begin and end once its line stood upright. `subtrackt-text` measures
the space between those instead of between bounding boxes.

| | before | after | |
| :--- | ---: | ---: | :--- |
| **10 Cloverfield Lane**, upright | 0.5% | **0.5%** | 11 cues better, 2 worse |
| 10 Cloverfield Lane, italic | 4.1% | **2.0%** | WER 24.7% → 8.4% |
| 10 Cloverfield Lane, all | 0.7% | **0.6%** | |
| **Gone Girl**, all | 2.8% | **2.0%** | **163 cues better, 4 worse** |
| **A Fish Called Wanda**, upright | 3.8% | **3.8%** | 6 cues better, 5 worse |
| A Fish Called Wanda, italic | 15.0% | **13.8%** | WER 44.7% → 39.9% |

**Gone Girl is the result, and it is the disc whose release cannot see it.** 18% of its lines lean
and neither English sidecar marks a single one, so every figure it has ever contributed has had that
act pooled into the upright column. Nought point eight of a point is the largest single accuracy
change since [#110][issue-110] halved the disc's error rate, and it came from a track nobody had
identified as having an italic problem.

### The word spaces, which is what this was for

`space` in the confusion census's **deletions** — a word break the release has that the extraction
never produced:

| missed word spaces | before | after | |
| :--- | ---: | ---: | :--- |
| 10 Cloverfield Lane, italic | 38 | **7** | 31 of 38 recovered |
| 10 Cloverfield Lane, upright | 28 | 29 | |
| A Fish Called Wanda, italic | 26 | **12** | |
| A Fish Called Wanda, upright | 50 | 53 | |
| **Gone Girl** | 783 | **165** | 618 recovered |

And the other direction — `space` in the **insertions**, a break put where the release has none,
which costs two word errors rather than one:

| invented word spaces | before | after |
| :--- | ---: | ---: |
| 10 Cloverfield Lane | 8 | **8** |
| Gone Girl | 20 | **13** |

Nothing was invented to buy this. On Gone Girl seven fewer spaces are invented as well.

### Two things had to be decided, and both were measured

**The estimate has to be gated.** Every non-zero shear *widens* an upright glyph's span, because
deskewing ink that was never skewed leans it — by about `|k|` times the glyph's height, which at a
shear of 0.03 over a 40-pixel capital is more than a pixel. That pixel comes off every gap on the
line, and a word gap clears the decisiveness test by two or three. Ungated, the estimator's ordinary
spread on upright material was enough to do real damage:

| | gated at 0.06 | ungated |
| :--- | ---: | ---: |
| Cloverfield, upright CER | **0.5%** | 0.7% |
| Cloverfield, upright WER | **2.2%** | 3.1% |
| Cloverfield, cues worse | **2** | 9 |
| Gone Girl, cues worse | **4** | 14 |
| Wanda, cues worse | **5** | 11 |
| italic CER, both discs | 2.0% / 13.8% | 2.0% / 13.8% |

**The gate is free.** The italic column is identical either way, because a real italic line reads
0.16 and is nowhere near the cut. What the gate buys is entirely on upright material, which is
prediction 5 arriving as a design constraint rather than as a result.

**The yardstick has to move with the gaps.** #40's first decisiveness test asks whether a cut
reaches half the line's median glyph *width*, and a slanted letter's box is mostly slant — 47 pixels
where its ink stands 40 wide, on the first italic line of a real disc. Measuring the gap one way and
the width the other is exactly the mismatch #99, #110 and #113 each made once. Taking both from the
deskewed span is worth 4 cues and 0.2 points of WER on Gone Girl and changes nothing anywhere else.

**Per line, not per cue.** Pooling a cue's lines into one estimate doubles the ink and was tried:
it is a wash on Gone Girl and *worse* on Wanda, whose italic CER goes 13.8% to 14.0% and whose WER
goes 39.9% to 40.6%. A cue can hold one italic line and one upright one, and the line is the unit
that leans.

### What it cost, and where it is still wrong

Eleven cues across three discs got worse. They share a shape: a descender — `j`, `y`, `g` — next to
a word break. A descender sits lowest on the line, so a shear moves it furthest, and where the
estimate is a little off that error is largest exactly where it is least affordable. `Burn itjust
the right amount` is the whole class in one line.

Two approximations are knowingly in the shipped path:

- **A recovered fusion reports its box.** #108 cuts a fused component in two, and a part is half of
  a labelled component — there is no label that names it, and under a shear the vertical cut maps to
  a slanted line, so "left of the column" and "left of the deskewed column" are not the same
  pixels. What makes the box tolerable is what put the part on that path: the two characters were
  *touching*, so the gap between them is near zero either way, and the outer edges a word break is
  measured against are the parent's own.
- **The gap is still saturated at zero.** An `UprightSpan` can express a negative gap and the
  spacing rules do not read one. That is deliberate: every rule ranks gaps, and a negative gap ranks
  below a zero one in exactly the way a clamped one does. What #121 changes is how *many* gaps are
  down there, not what happens to the ones that are.

### Predictions, scored

- **1. The 38 italic word spaces mostly come back — expect 25 to 38, not all.** *Right.* **31 of
  38**, and 618 of Gone Girl's 783.
- **2. Nothing is invented in their place.** *Right.* Cloverfield's invented-space count is
  unchanged at 8 and Gone Girl's falls from 20 to 13.
- **3. Upright material does not move.** *Right, and it took the gate to make it so.* 0.5% and 3.8%
  before and after, and the ceiling fixture and `xtask spacing-margin` are unchanged to the
  character — 1.2% CER, 44 of 48 breaks found, 46 of 50 single-word lines left alone.
- **4. Gone Girl gains the most and its sidecar cannot show it.** *Right on both halves.* 0.8 points,
  the largest of the three, and its italic column is still empty because the release has no `<i>` to
  put in it.
- **5. Wanda gains least.** *Right.* 6 cues better against 5 worse, and 1.2 points of italic CER on
  the 4% of its characters that lean.

[issue-110]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/110
[issue-121]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/121

## The matching half, and what it is an alternative to

[#122][issue-122] shipped the other insertion point: a leaning line's glyphs are sampled along the
line's own slant, so each grid cell's preimage is a **parallelogram** and the ink is never resampled.
The result is not the one the issue predicted, and the correction is worth more than the prediction
was.

**A deskew and an italic reference cut are two answers to the same question, not a stage and an
improvement to it.** Four configurations on 10 Cloverfield Lane's italic act:

| italic CER | no deskew | deskewed |
| :--- | ---: | ---: |
| **regular-only set** | 47.1% | **8.1%** |
| **with #66's italic cut** | **2.0%** | 5.4% |

Read either way round it says the same thing. Against a set with no italic entries the deskew is
worth **47.1% down to 8.1%** — a factor of six, and #14's most expensive axis paid back almost in
full. Against a set that carries the cut it makes things *worse*, because the cut already holds an
entry shaped like the ink and the deskew moves the glyph away from it.

The same table on the other two discs:

| whole disc, CER | regular-only | regular-only, deskewed | with the italic cut |
| :--- | ---: | ---: | ---: |
| 10 Cloverfield Lane | 3.4% | **1.0%** | 0.6% |
| Gone Girl | 11.6% | **3.2%** | 2.0% |
| A Fish Called Wanda | 5.5% | **4.2%** | 4.2% |

### What the bench got wrong, and it is the same mistake twice

`measure-stability --deskew` says slant falls from 47 cells to 26. Both numbers are distances to the
**upright** vector — and that is the right question only for a set with no italic entry. Where #66's
cut is present the baseline is not 47 at all; it is the *italic* entry's own intra-character spread,
which is far below 26. The bench answered a question the shipped configuration does not ask.

That is `docs/error-census.md`'s lesson arriving a third time. "Ambiguous" and "wrong" are not the
same set, and `xtask separability` only sees the first; a matchability statistic and a CER are not
the same question, and `docs/fit-confidence.md` is six statistics long on that; and now a distance
to the upright vector and a distance to the *nearest* vector are not the same question either. Each
time the instrument was sound and was measuring something adjacent to what shipped.

### So the reference set decides

Nothing here is a threshold or a heuristic. A set either holds an entry for a slanted rendering or
it does not, and it says so — so the deskew is on exactly when the set cannot read a slanted letter
as it is drawn:

| | cues better | cues worse |
| :--- | ---: | ---: |
| regular-only set, Cloverfield | **41** | 7 |
| regular-only set, Gone Girl | **416** | 20 |
| regular-only set, Wanda | **63** | 25 |
| **with an italic cut, all three discs** | **0** | **0** |

Zero on the last row is the whole safety argument: a user who followed
[`reference-set.md`](reference-set.md) and generated `arial-ri` gets a bit-identical extraction.

**Who this is for.** `subtrackt gen-reference arial.ttf out.subtref` — one font file, no `--italic` —
is the documented first invocation and it produces a regular-only set. Until #122 that set read
Cloverfield's italic act at 47.1% and Gone Girl at 11.6%. A user who never learned that this project
has an `--italic` flag was paying #14's slant axis in full on every leaning line.

### One disc says the deskew is better than the cut

A Fish Called Wanda reads its italic act at **13.8%** with Arial's italic cut and **12.9%** with a
regular-only set and the deskew. It is the only disc of the three where that holds, and it is the
disc whose italic act reads worst — which is what "the cut does not fit this material" looks like
from the inside.

It is not actionable and that is a finding rather than an omission.
[`fit-confidence.md`](fit-confidence.md) is six statistics long on exactly this: nothing in this
pipeline can tell a good reference fit from a bad one without ground truth, so nothing can choose
between the cut and the deskew per title. The shipped rule takes the cut where one exists because
two discs of three prefer it and because a user who supplied an italic face asked for it to be used.

[issue-122]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/122

## The tag on the output

[#123][issue-123] writes the estimate back out. A line shown to lean is written `<i>`, by both
writers, from a **per-line flag on the cue** rather than markup inside the string — post-correction,
`Cue::text`, `Cue::is_empty` and `xtask srt-score` all read those strings, and every one of them
would otherwise have to learn to strip a tag it did not put there.

| | paired cues | agree | italic in the release | read upright | read italic |
| :--- | ---: | ---: | ---: | ---: | ---: |
| 10 Cloverfield Lane | 818 | **98.8%** | 43 | 1 | 9 |
| A Fish Called Wanda | 1,336 | **97.6%** | 57 | 2 | 30 |
| Gone Girl | 2,442 | — | **0** | — | 442 tagged |

**CER does not move by a character on any of the three discs** — zero cues better, zero worse. The
scorer strips tags from both sides, so the tag is additive by construction and the table above is
the same extraction the two sections before it produced.

### It has to know which way an italic leans

The first version tagged any line the estimator would deskew, and scored 97.1% and **92.1%**. The
estimator is two-sided because *deskewing* is geometry — a line leaning either way is worth standing
upright — but an italic is **typography**, and no Latin emphasis face leans left. A positive estimate
is a line whose letters happen to carry diagonal ink, which is the sensitivity #115 named before any
of this was built: `A`, `V`, `w` and `y` are not slant.

| | tag on any lean | tag on an italic lean |
| :--- | ---: | ---: |
| 10 Cloverfield Lane | 97.1% | **98.8%** |
| A Fish Called Wanda | 92.1% | **97.6%** |
| upright cues tagged, Wanda | 104 | **30** |

One threshold, two questions, and only one of them has a direction.

### Where it is still wrong, named

The misses are the transcript disagreeing. What is left of the false tags is **short lines**:
Cloverfield's are three cues that each say `Please.` — six glyphs, where a moment over one line's ink
is at its noisiest and the composition of the alphabet is at its loudest. `MIN_GLYPHS` is four and
raising it would cost real short italic lines.

The figure is also a floor rather than an accuracy, for the reason `disc.rs` opens with: a release
marks what *that* release thought was italic, and this project has already measured a disc where the
answer is "none of it".

### Gone Girl again

**442 of its 2,442 cues are tagged italic and its release marks none.** That is the same 18% the gap
histogram found and the same act #121 gave 618 word spaces back to — three measurements, three
mechanisms, one population that no transcript of that film records.

### Predictions, scored

- **1. Per-cue agreement between 98% and 99.5% on the two discs with labels.** *Right on one, just
  under on the other* — 98.8% and 97.6%. The gap is per-line estimation where #120's figure pooled
  a cue's lines, and the residual is short lines rather than threshold placement.
- **2. Gone Girl gains tagged lines its release never had, and nothing can score them.** *Right.*
  442 cues, and the honest report is the count.
- **3. CER does not move.** *Right, exactly.* Zero cues changed on three discs.
- **4. The style byte and the estimator agree on more than 95% of cues.** *Not run, and the reason
  is the finding.* The byte reports which reference *entry* won, so it works only on a set carrying
  an italic cut — and #122 measured that such a set does not need the deskew at all. The second
  opinion would be unavailable in exactly the configuration the tag is most valuable in, which is
  argument enough not to build it.
- **5. The unknown rate stays under 2% of lines.** *Right*, unchanged from #120's 0.8% and 1.2%,
  because it is the same estimator.

[issue-123]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/123

## What this does not settle

The spaces are answered above. What is left:

- **The fusions.** #115's insertion point 1 is a shear applied before connected-component labelling,
  which would fix segmentation as well as spacing — a slanted ascender touching its neighbour is
  #106's class, in the act where slant makes it likeliest. Measuring the gap between deskewed
  extents does not touch a component that was already fused into one, and #121 did not try to.
- **Whether a set should carry an italic cut at all.** #122 measured the deskew against the cut and
  the cut wins on two discs of three — but it wins as a *whole configuration*, and the per-character
  table above says ten letters deskew onto their upright entry to within five cells. A set that
  carried italic entries only for the letters a shear cannot recover would be smaller than #66's and
  might read as well. Nothing has measured it.
- **Whether [#49][issue-49]'s decisiveness margin would have reached some of these anyway.** It is
  a smaller change than #121 and it is now measurable against a much smaller residual: 36 missed
  spaces on Cloverfield rather than 66, and 29 of the 36 are on upright lines this cannot help.

## Reproducing

```console
$ cargo build --release -p subtrackt-cli -p xtask --features subtrackt-glyph/font
$ subtrackt gen-reference /path/to/arial.ttf arial-ri.subtref \
      --name arial-ri --italic /path/to/ariali.ttf

$ cargo run --release -p xtask -- measure-stability \
      arial.ttf arialbd.ttf ariali.ttf arialbi.ttf --deskew

$ cargo run --release -p xtask -- slant movie.mkv arial-ri.subtref release.srt
```

`measure-stability` needs no disc and no media, and it is the cheapest kill in the whole proposal:
if the slant row does not fall, nothing after it is worth running.
