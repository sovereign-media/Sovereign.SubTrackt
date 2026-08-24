# Which glyphs to fix first

[#118] built a hit list from 50 titles and ranked the families by errors contributed rather than by
error rate. Everything in it rested on the per-character census, and that census has since been
replaced: [#169] moved it off the cue-level alignment, which #116 had already shown pairs characters
that were never meant to correspond wherever two releases break dialogue differently.

So the ranking is re-derived here on the nine-track bench, with the census that never consults a
clock. It confirms one of #118's conclusions, corrects another, and turns up a third thing that
issue could not have seen.

## The families, on the bench

Substitutions only, from `xtask srt-score`, over the seven scored tracks:

| track | substitutions | `I`/`l` | case pairs |
| :--- | ---: | ---: | ---: |
| 10 Cloverfield Lane | 48 | 5 | 3 |
| Gone Girl | 710 | 139 | 384 |
| A Fish Called Wanda | 703 | **428** | 74 |
| King Kong | 132 | 32 | 22 |
| Airplane! | 281 | 42 | 25 |
| The Karate Kid | 996 | **747** | 149 |
| Training Day | 1,322 | **1,031** | 154 |
| **total** | 4,192 | **2,424** | 811 |

#118 had these two families nearly level — 2,780 against 2,677 — over PGS material. Here `I`/`l` is
three times the size of the case pairs, and the two VOBSUB tracks are why: they contribute 1,778 of
its 2,424. [#140] found the same thing independently, at 510 `l → I` on one VOBSUB title against 65
for the next most common substitution.

## The `I`/`l` family, and the feature that was supposed to fix it

[#109] added [`InkAspect`] — ink width as a fraction of the glyph's **own** height — specifically to
separate these two. Its justification is on the record: *"An `I` is 7% wider than an `l` in Arial's
outlines"*, about 123 permille against 131, and *"on a real disc it separates `l` from `I` at two
glyphs in 867"*.

Measured on the discs rather than in the font, over all three of the bench's Arial titles. **The
same feature behaves three different ways**, and the mean alone hides which:

| disc | `l` typical | `I` typical | best single threshold | `I → l` errors |
| :--- | ---: | ---: | ---: | ---: |
| 10 Cloverfield Lane | 5.00 px, sd **0.00** | 6.00 px, sd **0.00** | **0.0% wrong side** | 5 |
| Gone Girl | 5.00 px (p25=median=p75) | 6.00 px (p25=median=p75) | 17.0% | 139 |
| A Fish Called Wanda | 5.00 px (p25=median=p75) | **5.00 px** (p25=median=p75) | 15.2% | **428** |

**On Cloverfield the feature works exactly as designed.** Every `l` on the disc is five pixels wide
and every `I` is six — standard deviation zero on both — and no glyph is on the wrong side of the
threshold. #109's claim is not wrong; it is a description of this disc.

**On Gone Girl the axis is sound and the population is not.** The quartiles are disjoint at 5 and 6,
exactly as above, but `l`'s ink runs out to 27 pixels with a standard deviation of 3.92. A vertical
bar cannot be five times as wide as it is unless it is not one: those are components fused to a
neighbour. About a fifth of the class is contaminated, and it is the tail rather than the axis that
defeats the threshold.

**On Wanda the axis is simply absent.** The disc draws `l` and `I` at five pixels at the 25th, 50th
and 75th percentile alike — the typical glyphs are identical to the pixel. The best threshold's
15.2% is the class imbalance itself: with 1,282 `l` against 232 `I`, always guessing `l` scores
15.3%. **No threshold does better than ignoring the measurement**, and no amount of de-fusing would
change that, because the cores coincide.

So the answer to *is the axis dead or the population contaminated* is **both, and which one depends
on the disc** — which is more useful than either, because the two want opposite work. Gone Girl
wants [#106]'s de-fusing, which cannot reach it: that pass fires only where the matcher already
returned `unmatched`, and a fused `l` that matches `I` is not unmatched. Wanda wants something that
is not a glyph measurement at all.

What is left for Wanda is context, and it is not reaching either. The track carries 4,271 ambiguous
glyphs, and post-correction makes **11 corrections** against 428 `I → l` errors — the track
vocabulary arm adds none. On a disc that draws the two letters identically the matcher is not
hesitating between them; it is confident, and the one stage allowed to revisit a decision only
touches glyphs the matcher itself flagged.

## The case pairs, which are in better shape than expected

The same measurement over the pairs that are one shape at two sizes:

| pair | `lower` | `UPPER` | best single threshold |
| :--- | :--- | :--- | ---: |
| `o` / `O` | 84.85% ± 0.15 | 88.33% ± 1.04 | **0.3% on the wrong side** |
| `s` / `S` | 76.89% ± 2.37 | 75.64% ± 1.62 | 1.5% |
| `c` / `C` | 81.83% ± 0.25 | 82.92% ± 1.96 | 3.5% |

The aspect ratio separates these cleanly, which is not obvious — two scale copies of one letterform
have the *same* ratio by construction, and what makes them separable is that a face draws its
capitals proportionally a little wider than its lowercase. On Gone Girl `s → S` is 37 errors against
3,350 occurrences of `s`, which is 1.1% — at or below what the geometry supports.

So the case-pair family is closer to its floor than #118's table implied, and the effort is better
spent elsewhere.

## The measurement that was invisible

`Report::glyphs_without_metrics` counts glyphs standing on a line whose baseline and cap height could
not be found. [#37]'s whole term is off for every one of those, so an `o` and an `O` are compared on
shape alone. The counter existed from the day the feature did and **was never printed**, so nobody
could watch it. It is on the `--report` line now, as a share:

| track | unmeasured lines |
| :--- | ---: |
| 10 Cloverfield Lane | 0% |
| Gone Girl | 0% |
| A Fish Called Wanda | 1% |
| Airplane! | 4% |
| King Kong | **14%** |

King Kong is the one to watch. It is also the PGS track with the worst CER on the bench at 21.3%,
and one glyph in seven there is being matched without the feature that separates `o` from `O`.

## The library agrees, and says which population each ranking describes

#119 re-extracted the 47-title corpus and rebuilt its census on the track-level alignment too, so
the same question can now be asked of 2.18 million characters rather than of nine tracks. Grouped
into families and expressed as a rate, over the 21 titles whose sidecar corroborates:

| substitutions per 1,000 release characters | rate |
| :--- | ---: |
| case pairs | 2.96 |
| `I` / `l` | 2.92 |
| brackets | 0.72 |
| punctuation | 0.47 |
| other | 1.72 |

**Dead level** — which is #118's original finding, 2,780 against 2,677, arriving again on a corpus
five times the size. So the two rankings never disagreed: #118 measured PGS library material and
found them equal, this file measured a bench that had gained two VOBSUB tracks and found `I`/`l`
three times larger. The two VOBSUB tracks contribute 1,778 of that family's 2,424 errors. **The
family's size is a property of the codec, not of the ranking**, and a bench weighted differently
would move it again.

What survives both populations is that `I`/`l` is the *stable* family. Widening the corpus census
from the corroborated 21 to all 47 multiplies every other family by two to ten times — case pairs
from 2.96 to 16.88, `other` from 1.72 to 17.03 — and moves `I`/`l` from 2.92 to 4.31.
[`library-accuracy.md`](library-accuracy.md) §"What the other 26 titles are made of" has why the
rest moves: it is transcript divergence rather than misreading, and one title supplies 62% of it.

## Where this leaves the list

1. **`I` / `l`, 2,424 errors.** The largest family, and what is wrong with it differs by disc —
   see above. One disc needs nothing, one needs its fusions cleaned up, and one needed something
   that is not a glyph measurement. **#171 found what.** Four fifths of it is the English pronoun
   `I`, at a position neither existing corrector arm can reach: a one-character word has no
   context on either side, and the vocabulary arm cannot learn it because every `l` and `I` in a
   track is ambiguous by construction. `docs/post-correction.md` §"The one-character word" has the
   measurement and the arm that answers it — 395 cues improved across the bench, none made worse,
   and Wanda from 1.7% to 1.1% CER.
2. **Fused components.** Not a family in #118's table at all, and it turns up inside the first one:
   a fifth of Gone Girl's `l` population is glyphs several times too wide. #106's de-fusing fires
   only where the matcher already returned *unmatched*, so a fusion that matches something is
   invisible to it — which is exactly this case, since a fused `l` matches `I`.
3. **Unmeasured lines on King Kong**, 14%, now visible.
4. **Case pairs, 811 errors**, near their floor.

Two of #118's three total failures are fixed — `:` by [#130] and `"` and `♪` by [#168]. This file is
what is left of that issue.

[#37]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/37
[#106]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/106
[#109]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/109
[#118]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/118
[#130]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/130
[#140]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/140
[#168]: https://github.com/sovereign-media/Sovereign.SubTrackt/pull/168
[#169]: https://github.com/sovereign-media/Sovereign.SubTrackt/pull/169
[`InkAspect`]: https://github.com/sovereign-media/Sovereign.SubTrackt/blob/main/crates/subtrackt-core/src/glyph.rs
