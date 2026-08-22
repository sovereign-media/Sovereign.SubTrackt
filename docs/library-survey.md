# Typeface survey

Answers [#8][issue-8], which the architecture document calls "the whole risk": the reference set in
§2D of [#1][issue-1] names Arial, Helvetica, Trebuchet MS and Tiresias, and that list is a guess. A
title using anything else degrades to garbage rather than to nothing, which is worse than the status
quo.

[issue-1]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1
[issue-8]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/8
[issue-26]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/26

## Method

`subtrackt glyphs <file> --limit 100` segments a file into normalised 256-bit feature vectors and
stops before matching. Vectors are comparable across titles even though nothing can yet name the
character any of them stands for, which is what makes the question answerable now.

Sampling was deterministic rather than random, so the survey re-runs to the same result: titles
sorted by folder name, then an even stride taken within each decade. Nine PGS titles per decade plus
twelve VOBSUB titles were requested from the 1,328 in the library inventory.

| | |
| :--- | ---: |
| Titles requested | 73 |
| **PGS titles analysed** | **56** |
| VOBSUB titles analysed | **0** — see below |
| Glyph instances | 149,604 |
| Distinct shapes per title | median 148 |
| Glyph height | 21–50 px, median 33 |
| Decades covered | 1950s–2020s |

Two gaps, both of which limit what this can claim:

- **All twelve VOBSUB samples failed**, because #3 is unimplemented. The DVD-era half of the library
  is entirely unsurveyed. It is only 4% of titles but it is the part most likely to differ, being
  lower resolution and older. **This survey is about PGS only.**
- **Four PGS titles failed to parse** with a malformed composition segment — 6.6% of the PGS files
  attempted — so the numbers below are over the files readable at the time. That was [#26][issue-26]
  and is now fixed: the composition object's cropped and forced flag bits were the wrong way round.
  All four are forced-subtitle tracks, which is why they were the ones that broke. Re-running the
  survey would now include them; the headline figures are unlikely to move, since four titles
  against fifty-six is within the noise, but they have not been re-measured.

## Is there a dominant glyph set?

Yes, and it is not subtle. Taking each title's 60 most frequent shapes and measuring how many have a
close counterpart in another title:

| Nearest-neighbour distance between titles | |
| :--- | ---: |
| p5 | 0 cells of 256 |
| p25 | 2 |
| p50 | 19 |
| p75 | 36 |
| p95 | 65 |

A quarter of glyphs from one title have an essentially exact counterpart in another. Clustering
titles by mutual alphabet similarity:

| Threshold | Clusters | Largest | Top 3 cover | Singletons |
| ---: | ---: | ---: | ---: | ---: |
| ≤ 16 cells (6%) | 19 | 25 | 39/56 (69%) | 15 |
| ≤ 24 cells (9%) | 14 | 27 | 43/56 (76%) | 10 |
| ≤ 32 cells (12%) | 10 | **43** | 49/56 (87%) | 7 |
| ≤ 40 cells (15%) | 5 | 51 | 54/56 (96%) | 3 |

The largest cluster at ≤32 spans **1950 to 2025**. Era is not the organising principle; something
closer to a house style is.

This also validates #7 independently. Glyph heights across the sample range from 21 to 50 pixels,
and titles at opposite ends of that range still cluster together — which only happens if the
normalisation really is scale-invariant.

## What would a reference set actually cover?

The number that decides #9. A 120-shape reference was pooled from the 13 titles closest to the
densest part of the largest cluster, then every title was scored on the fraction of its glyph
*instances* (weighted by how often each shape occurs) that find a match within a given distance.

| Match threshold | Median | p10 | Worst | ≥90% | ≥50% |
| ---: | ---: | ---: | ---: | ---: | ---: |
| ≤ 24 cells | 81.7% | 22.0% | 7.0% | 21/56 | 43/56 |
| ≤ 32 cells | 88.8% | 31.7% | 15.5% | 27/56 | 50/56 |
| ≤ 40 cells | 93.2% | 52.9% | 24.7% | 35/56 | 51/56 |
| ≤ 51 cells (the matcher's current ceiling) | 96.5% | 78.9% | 43.4% | 48/56 | 53/56 |

**The median is good and the tail is not.** At the operational threshold, half of all titles would
be read at better than 96%, and the worst title in a 56-title sample sits at 43%.

Two things this table does *not* say, and both matter:

- **Coverage is not correctness.** These are unlabelled shapes matched against other unlabelled
  shapes. A match at 51 cells says two glyphs look similar; it does not say the matcher would pick
  the right character. §7's measurements put the same character across resolutions within ~25 cells
  and two different characters more than three times further apart — so 51 is uncomfortably close to
  the distance at which characters start being confusable. Correctness cannot be measured until #15
  supplies ground truth.

  **#15 landed, and this caveat was an understatement.** Scored against ground truth, a Segoe UI
  reference set matches the *same* 93.9% of glyphs as an Arial one on Arial-authored material and
  reads at 37.8% character error against Arial's 15.9%. Coverage barely moved; accuracy went 2.4×.
  Every coverage figure on this page should be read as an upper bound on something it does not
  measure. See [reference-set.md](reference-set.md).
- **The reference was pooled from the library itself**, so this is partly measuring self-similarity
  and is optimistic by an unknown amount.

## Fitting a font to the measurement

Recommendation 2 below said to stop naming typefaces and fit one to the evidence instead. That was
done: `cargo run -p xtask -- gen-reference` renders a font through *the same* normalisation the
runtime uses, and the result was scored against all 149,604 sampled glyph instances.

**The typeface is Arial or something very close to it.** Taking the most frequent extracted shapes
from one title and asking which Arial character each is nearest to:

| Extracted shape | Instances | Nearest Arial character | Distance |
| ---: | ---: | :--- | ---: |
| 1 | 28 | `i` | **0** |
| 2 | 39 | `t` | 6 |
| 3 | 43 | `a` | 10 |
| 4 | 31 | `r` | 12 |
| 5 | 36 | `o` | 13 |
| 6 | 48 | `e` | 16 |

Those are English's most frequent letters, in roughly the order you would expect, one of them an
exact bit-for-bit match. The pipeline is extracting real letterforms and the reference generator
produces vectors comparable with them.

**And yet a fixed set is not sufficient on its own.** Scored over every glyph instance rather than
over the common shapes:

| Match threshold | Arial | Tahoma | Verdana |
| ---: | ---: | ---: | ---: |
| ≤ 16 cells | 13.1% | 13.3% | 13.5% |
| ≤ 32 cells | 24.7% | 27.4% | 26.2% |
| ≤ 51 cells (current ceiling) | 46.3% | 49.1% | 44.9% |
| ≤ 64 cells | 58.0% | 56.8% | 55.9% |

Two candidate explanations were tested and rejected:

- **Fused characters.** 71% of extracted glyphs have single-character aspect ratios and the widest
  is 69px against a 33px line height, so segmentation is not gluing letters together.
- **Stroke weight.** Extracted glyphs carry median popcount 82 against Arial's 64, so bold faces
  were tried. Matching the ink weight (Arial Bold at 89, Verdana Bold at 96) made coverage
  *worse*, not better.

What is left is **intra-character variance**. A character rendered at slightly different subpixel
offsets, with different anti-aliasing, binarizes to a slightly different mask each time. One title
of 120 cues produced 143 distinct shapes for perhaps 70 distinct characters, and the long tail of
those variants sits 51–80 cells from any canonical rendering. §7 measured ~25 cells of movement for
the same character across resolutions using synthetic renders; real material is considerably worse
than that, which is a finding in its own right and squarely what #14 exists to quantify.

**So #9 cannot be finished by embedding a font.** The fixed set identifies the typeface and labels
roughly half the glyph instances outright. Reaching the rest needs recommendation 4 below — cluster
a title's *own* repeated shapes, then match cluster centroids against the reference — which turns
the session cache from an optimisation into the mechanism. It is no longer optional.

## Recommendations

**1. Keep the fixed reference set. Do not abandon it.** A dominant glyph family covers most of the
library across seventy years, which is the outcome §2D of #1 needs and was not guaranteed.

**2. Stop naming typefaces and go the other way round (#9).** ~~The Arial/Helvetica/Trebuchet/Tiresias
list should not be built from.~~ Instead: render candidate fonts, vector them, and match them against
the dominant cluster measured here. Whichever font aligns *identifies* the typeface and supplies the
character labels the cluster lacks.

**Done, and the mechanism works — but not as a shipped set.** The fit identified Arial. Scoring
candidate sets against ground truth then showed that "or very close" is not close enough: Liberation
Sans, metric-compatible with Arial and drawn to match it, costs 11 points of character error, which
is Verdana's cost to within noise. So the fitting has to happen against the *title*, not once
against the library — #43.

**3. `FailTrack` cannot stay the default (#13).** ~~It rejects a track on a single unmatched
glyph.~~ **Done.** The default is `Threshold { min_ratio: 0.90 }`, and the 90% column of the table
above — 48 of 56 titles — is half of what set it. The other half is the pipeline's own ceiling case
at 93.9%, above which the floor would be `FailTrack` again.

**4. ~~The tail needs the session cache to be load-bearing (#10, #14).~~ Wrong, and measured
wrong.** The recommendation was to match by self-consistency within a stream — cluster a title's own
repeated shapes and label clusters rather than individual glyphs. It was built and swept, and it is
**worse at every radius**. The premise was that a character's own renderings are closer to each
other than to the nearest different character; #14 found the nearest different character at distance
*zero* (`I`, `l`, `|`), so no radius exists. Clustering ships off. See
[glyph-stability.md](glyph-stability.md).

**5. Survey VOBSUB once #3 lands.** #3 landed — control sequences, out-of-band palette and nibble
RLE all decode — but the survey was never re-run. Nothing here still describes DVD-era subtitles, and
that remains the largest unmeasured corner of the library at 60 titles.

## Reproducing

```console
$ scripts/survey/collect.py --csv image-based-subs-report.csv --out shapes/
$ scripts/survey/analyse.py shapes/
```

Sampling is deterministic, so a re-run against the same library reproduces these numbers.
