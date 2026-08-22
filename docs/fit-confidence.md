# Telling a good fit from a bad one

Answers [#63][issue-63], the research half of [#43][issue-43]: given a reference set fitted to a
title, can anything say whether the resulting read is *good* — without ground truth?

**No.** Five statistics, measured over three years of issue numbers, and none of them separates a
good read from a bad one. The fitted set therefore ships permanently as a **proposal a user accepts**
rather than a decision the tool makes, which is [#62][issue-62].

That is a worse product than the alternative and an honest one. It is also not the interesting part.
The five failed for **two** different reasons, and the second one is a new fact about this pipeline
rather than a restatement of the first.

[issue-1]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1
[issue-43]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/43
[issue-62]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/62
[issue-63]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/63

## Why it matters

§4 of [#1][issue-1] rejected general OCR because its failure mode is a confident wrong answer. Every
other stage in this pipeline honours that: an unmatched glyph is a *fact*, counted rather than
estimated, and a caller can gate on it.

Reference-set choice is the one place that was not true. A mismatched set does not fail loudly. It
produces roughly **73% correct text and 27% confidently wrong**, with no counter saying which
quarter is which — Tesseract's failure mode, arrived at from the other direction.

So this is not a tuning question. It is the last place in the pipeline where a failure is not yet a
fact.

## The four that ask the matcher about its own answer

All four are in [`reference-set.md`](reference-set.md), each with its prediction recorded before the
run.

| statistic | result |
| :--- | :--- |
| **Mean match distance** | Calibri fits closest at 14.7 cells and reads at 11.5% CER; Trebuchet is 1.4 cells further out and reads at 2.6%. No floor ships the good read without the bad one. |
| **Distance charging unmatched glyphs the ceiling** | Every value shifts about two cells; no ordering changes. |
| **Winner against runner-up** | Smallest gap with the answer present 0.9 cells; largest with it withheld 6.4. Overlaps in the wrong direction. |
| **Agreement between top candidates** | Overlaps by 13.8 points at track level, and **inverts** per character on Arial, where the committee agrees on `I` for `l` 108 times. |

They failed for one mechanism, stated twice:

> A systematically wrong set is **by construction** a low-distance one — the matcher chose `I` for
> `t` precisely because they were close.

> A systematically shared confusion is **by construction** an agreed one — every sans-serif reads
> `l` as `I` because the letterforms are identical, and the *correct* set makes that error too.

Both times, the thing that makes the answer wrong is the thing that makes the evidence look right.
Each of the four is a function of the matcher's assignment — its cost, its cost plus a constant, the
shape of its argmin, several argmins compared — so each inherits the bias that produced it.

## The fifth, which escapes that and fails anyway

From *Font Representation Learning via Paired-glyph Matching* (BMVC 2022). The model is a ResNet18
and did not arrive in this workspace; the **criterion** is what was worth taking. It trains on pairs
that are the same font and *different characters*, and its evaluation never lets a character be
compared with itself — query `Ci`, gallery `Cj`, `i != j`. Font identity survives not knowing what
letter you are looking at.

A character-agnostic style score never consults the assignment. It does not ask whether this glyph
is an `I` or an `l`; it asks whether the ink in this track is *shaped* the way this typeface shapes
ink, pooled over thousands of glyphs with the labels thrown away. The 108 `I`-for-`l` errors are
invisible to it, which is exactly why they cannot corrupt it.

### Method

```console
$ cargo run --release -p xtask -- font-id --retrieval-only <font.ttf>...   # font files only
$ cargo run --release -p xtask -- font-id [--continue] <font.ttf>...       # all five steps
```

Seven hand-crafted axes, each a fraction of something measured rather than an absolute cell count:
**slant** (the second-moment cross term `mark::slope_of` uses, applied to the body), **stroke
weight** (median horizontal run ÷ box height), **contrast** (widest run against narrowest),
**terminal energy** (ink in the edge rows against the middle band), **density**, **aspect** and
**roundness**. Pooled per track as a median and an interquartile spread, compared by weighted L1.

Two decisions in the harness are load-bearing:

- **Pooling is over distinct shapes, not glyph instances.** A track's glyphs follow English letter
  frequency; a font's charset is uniform. Pooling instances would compare an `e`-heavy distribution
  against a flat one and read the difference as style. Deduplicating by feature vector leaves about
  one entry per character on both sides — as close to the same population as this gets without
  asserting a language, which is the prior [`post-correction.md`](post-correction.md) refuses.
- **Weights are fitted on pooled descriptors, not on per-character vectors.** Between-font scatter
  over within-font scatter, per axis. Fitting per-character measures the difference between an `i`
  and an `M` as within-font noise, which swamps the between-font term for every axis at once; the
  symptom was a fitted weighting scoring *worse* than equal weights, which a correct Fisher ratio
  cannot do. Fixing it is worth 17 to 33 points.

**Distances are only comparable within one run.** The weights are normalised over whichever fonts
were passed, so a distance from a 14-font run and one from a 24-font run are in different units.
Every comparison below stays inside a single run.

### The axes work

`i != j` retrieval, query and gallery characters disjoint by construction, off the font files alone:

| measured on | 24 typefaces | 39 fonts incl. cuts |
| :--- | ---: | ---: |
| the 16×16 feature vector | 50% | 64% |
| **the 96 px raster** | **79%** | **85%** |

#63 predicted above 70%. It clears that on the raster, and the harder list — which puts `arialbd` in
the gallery beside `arial` — scores *higher*, not lower.

**The normalised feature vector loses 46 to 54 points of it**, and it should. That vector letterboxes
onto a 16×16 grid and thresholds each cell because it exists to make two renderings of one character
*converge*; [`glyph-stability.md`](glyph-stability.md) records it as built to absorb rendering size
and anti-aliasing, and those are the axes style lives on. At that resolution a stem is one to three
cells, so stroke weight and contrast are quantised away before anything measures them. A
representation designed to hide rendering differences is the wrong place to look for what separates
two typefaces — it is doing its job.

That gap is why `Config::glyph_masks` exists: a survey can now keep each glyph's un-normalised ink,
so the track side can be measured at the resolution that works.

### And it still cannot separate

The bar is separation, not correlation: **every** good read scoring better than **every** bad one.
Fourteen materials, leave-one-out, with the font side measured the same way as the track side:

| | 16×16 grid | glyph mask |
| :--- | ---: | ---: |
| argmin picks the material's own font | 9/14 | 9/14 |
| separation | overlaps | overlaps |

Giving the descriptor a track's actual ink rather than its normalised vector improves nothing that
matters. Five of fourteen materials still score *closer to a foreign font than to their own*.

## Why: the channel, not the statistic

With both sides finally measured alike, the mechanism is arithmetic (14-font run, mask side):

- a decoded track sits **0.37 to 0.76** from its own font
- different fonts sit **0.38** apart at p10, **1.07** at the median
- the closest candidate pair, `arial`↔`calibri`, sits **0.22** apart

**The drift between a track and its own typeface is larger than the distance between the typefaces
it must be told apart from.** A floor placed anywhere in that range either refuses a track reading
its own typeface or admits one reading a neighbour's.

This is the style-level echo of what [`glyph-stability.md`](glyph-stability.md) found per glyph: two
renderings of the same thing differ more than two different things do. The same fact one level up,
defeating a descriptor the same way it defeated one reference vector per character.

So the fifth statistic did escape the mechanism that killed the first four, and broke on a second
one instead:

1. **Four broke on the matcher's own bias.** A wrong set is by construction a low-distance one.
2. **The fifth broke on the channel.** Font identity is real and measurable *in the font file*, and
   does not survive being rendered at subtitle size, decoded off a subtitle plane and binarized.

That second mechanism generalises past this proposal, which is the reason to write it down:
**anything measured off decoded subtitle ink inherits that noise floor, whatever it measures.**

## Prediction 4, refuted backwards

#63 predicted the deciding result would be blindness *inside* a typeface family — a floor that
answers "is this family present at all" and never "is this the right cut", leaving it useless inside
the "Arial **or very close**" population [`library-survey.md`](library-survey.md) found. It predicted
the wrong direction.

24 fonts spanning nine families, on the raster:

```
within a family:  mean 1.287 over 44 pairs (closest 0.493)
between families: mean 1.041 over 508 pairs (closest 0.062)
```

Within-family distance is **larger** than between-family. The neighbour table needs no summary:

```
font         three nearest, by style distance
arial        calibri 0.06  trebuc 0.09  tahoma 0.10
arialbd      calibrib 0.08  consolab 0.22  trebucbd 0.29
ariali       calibrii 0.31  georgiai 0.38  verdanai 0.39
verdanab     tahomabd 0.06  arialbd 0.29  calibrib 0.34
calibrii     verdanai 0.14  ariali 0.31  georgiai 0.34
georgia      times 0.17  verdana 0.27  trebuc 0.35
```

Regulars sit with regulars, bolds with bolds, italics with italics. **One font in twenty-four has a
member of its own family as its nearest neighbour.**

The fitted weights say the same thing without the table. Over the same charsets the Fisher ratio puts
**1.833 on stroke weight** and no more than 0.056 on any other axis:

| axis | weight |
| :--- | ---: |
| **weight** | **1.833** |
| density | 0.056 |
| aspect | 0.027 |
| round | 0.022 |
| slant | 0.012 |
| contrast | 0.007 |
| terminal | 0.006 |

Which is what these axes measure, read back honestly. Slant, stroke weight, contrast, terminal
energy, density, aspect and roundness are the dimensions a *cut* varies along. What separates Arial
from Calibri at the same weight is letterform construction, and seven coarse moments barely see it.

**The descriptor is a cut detector that was being asked to be a typeface detector.**

## The sentence worth keeping

Retrieval is 79–85% while `arial` and `calibri` sit 0.06 apart. Both are true, and reconciling them
is the most portable thing this measurement produced: retrieval compares a font's own half-charset
against other fonts' halves, and same-font halves differ only by sampling noise.

> **Retrieval asks for a ranking. A floor needs a margin.**

The ranking is good. The margin is 0.06 against 0.37–0.76 of decode drift — three to twelve times
more noise than signal. A statistic can rank candidates well and still be useless as a gate, and the
paper's own metric flatters it because ranking is all that metric asks for.

Any future proposal here should be judged on its margin against decode noise, not on its accuracy at
picking a winner. Picking a winner is [#62][issue-62], and mean match distance already does that at a
cost of 0.2 points of CER on average.

## What this settles, and what it does not

**Settled.** No statistic computed from the candidate sets, the matcher's output, or the decoded
ink's style separates a good fit from a bad one. #63 closes on the outcome it named in advance: the
fitted set is a proposal a human accepts.

**A standing filter for any future proposal**, from #63 and still worth applying: *does the statistic
consult the reference glyph the matcher chose for this query glyph?* If yes, it owes an argument for
why it is not a sixth instance of the first mechanism. If no, it owes a margin against the second.

**A correction to how "independent" gets tested.** #63's original falsification was that correlating
with mean match distance convicts a statistic of sharing its bias. That is too strong: two genuinely
independent statistics that both track the right answer *must* correlate. The measured rho of 0.72 is
equally consistent with both readings and convicts nothing. The discriminating test is whether a
candidate goes wrong in the *same places* — Calibri at 14.7 cells reading 11.5%, Trebuchet further
out reading 2.6%. Agreement where match distance is right is evidence of nothing; agreement where it
is wrong is the signature of a shared mechanism.

**Not measured.** Liberation Sans was not available on the machine that ran this, so the canonical
metric-compatible clone — priced at **+11.0 CER** against Arial in
[`reference-set.md`](reference-set.md) — has not been put through the descriptor. The instrument
takes one font file. On the evidence above, Arial↔Liberation should land in the same 0.06–0.10 band
as Arial↔Calibri, making the clone an instance of the general finding rather than a special case.
**That is a prediction recorded before the measurement, not a result.**

**Still open, and untouched by this.** Evidence from outside the candidate set entirely: a lexical
prior, which [`post-correction.md`](post-correction.md) refuses by design and which would be the
project's first dependency of consequence; or the track's own vocabulary, which is
[#60](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/60) and aims at characters rather
than at a verdict on the whole fit.
