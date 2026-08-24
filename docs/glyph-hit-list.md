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

Measured on the disc rather than in the font, over two of the bench's three Arial titles:

| | population | `l` | `I` | best single threshold |
| :--- | ---: | :--- | :--- | ---: |
| Gone Girl | 2,364 / 738 | 16.76% ± 9.33 | 15.45% ± 4.62 | **17.0% on the wrong side** |
| A Fish Called Wanda | 1,282 / 232 | 11.94% ± 1.26 | 12.66% ± 8.16 | **15.2% on the wrong side** |

**The two classes overlap, and on Gone Girl they are the wrong way round** — the disc draws `l`
*wider* than `I` on average, where the font predicts the reverse. No threshold on this axis can do
better than misclassifying one glyph in six.

Absolute ink width says the same thing more plainly. On Wanda the disc draws `l` at a mean **5.01
pixels** and `I` at **5.33** — where #109's note records *"every upright `l` on a real Blu-ray is
five pixels wide and every `I` is six"*. On this disc they are the same width.

The spreads say where to look. `l` on Gone Girl ranges from 11.90% to **64.29%** — four times its own
median — and `I` on Wanda reaches 100%. A vertical bar cannot be four times as wide as it is unless
it is not a vertical bar: those are components fused to a neighbour, which is [#106]'s territory and
the reason the standard deviations are what they are.

**This does not say #109 was wrong to ship.** It says the axis was validated on the reference side
and on one disc, and does not hold on these. The term costs nothing where it carries no signal —
`InkAspect::difference` is a fraction of a percent either way — so nothing is being actively harmed;
it simply is not buying what the top family needs.

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

## Where this leaves the list

1. **`I` / `l`, 2,424 errors.** The largest family, and the axis meant to separate it does not, on
   two discs. Whatever replaces it has to be measured on discs rather than on outlines.
2. **Fused components.** Not a family in #118's table at all, and it turns up inside the first one:
   the `l` population is contaminated by glyphs four times too wide. #106's de-fusing fires only
   where the matcher already returned *unmatched*, so a fusion that matches something is invisible
   to it.
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
