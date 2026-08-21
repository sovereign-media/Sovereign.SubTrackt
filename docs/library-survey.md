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
- **The reference was pooled from the library itself**, so this is partly measuring self-similarity
  and is optimistic by an unknown amount.

## Recommendations

**1. Keep the fixed reference set. Do not abandon it.** A dominant glyph family covers most of the
library across seventy years, which is the outcome §2D of #1 needs and was not guaranteed.

**2. Stop naming typefaces and go the other way round (#9).** The Arial/Helvetica/Trebuchet/Tiresias
list should not be built from. Instead: render candidate fonts, vector them, and match them against
the dominant cluster measured here. Whichever font aligns *identifies* the typeface and supplies the
character labels the cluster lacks. That turns #9 from a guess into a fit against evidence, and the
survey data is the fixture it fits against.

**3. `FailTrack` cannot stay the default (#13).** It rejects a track on a single unmatched glyph.
At 96.5% median coverage a typical film has hundreds of unmatched glyphs, so the current default
would reject essentially every track in the library. `Threshold { min_ratio }` is the only variant
these numbers support, and the floor should be set from the corpus in #15 rather than picked.

**4. The tail needs the session cache to be load-bearing (#10, #14).** Around a fifth of titles
would be poorly served by any fixed set. For those, matching by self-consistency within a stream —
clustering a title's own repeated shapes and labelling clusters rather than individual glyphs — is
the mechanism that degrades gracefully. #14 should be read with this in mind.

**5. Survey VOBSUB once #3 lands.** Nothing here describes DVD-era subtitles at all.

## Reproducing

```console
$ scripts/survey/collect.py --csv image-based-subs-report.csv --out shapes/
$ scripts/survey/analyse.py shapes/
```

Sampling is deterministic, so a re-run against the same library reproduces these numbers.
