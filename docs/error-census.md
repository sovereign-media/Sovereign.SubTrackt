# Where the 5.5% actually is

[#98][issue-98]. The pipeline has read a real Blu-ray at 5.5% CER since #66, and until this was built
nothing in the repository said **which characters** the missing 5.5% were. Every candidate in
[#97][issue-97] was ranked on inference from that gap.

It is one character. **Half the errors on the disc, and every unread glyph in the ceiling fixture,
are the full stop.**

[issue-97]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/97
[issue-98]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/98
[issue-99]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/99
[issue-49]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/49

## Why this was missing, and why that is the interesting part

`xtask separability` measures which pairs sit **close** in the reference set. #48 read that as
"accent direction is the dominant residual confusion", built a mark-slope term, swept it across
eight conditions and shipped it off at weight 0. `docs/glyph-stability.md` records the lesson:

> "Ambiguous" and "wrong" are not the same set, and `xtask separability` only sees the first.

Everything since has had the same shape. Coverage says how many glyphs found *a* reference; mean
match distance says how well they fitted it; `unread: N` says how many found nothing. Not one of
them names a character. So a proposal could be ranked plausible for a year without anything being
able to say whether the failure it addressed happens.

## The two instruments

**`xtask srt-score` now emits a confusion census.** The score keeps its rolling-row Levenshtein
untouched and the census runs a second, traceback-capable alignment over the same pair of cues. #98
named both options — widen the scoring routine, or run a second pass — and this is the second,
because a census that changed how a distance is computed could move a CER figure while claiming to
be additive. The two are pinned together by
`a_census_accounts_for_exactly_the_errors_the_score_counted`: the operation count equals the
distance, on every input, which is the only thing that makes a census row correspond to a scored
error.

The buckets are named from the release's point of view, and it is worth stating because #98
predicted two of them the other way round:

- a **substitution** is a character the release has that was read as something else;
- an **insertion** is a character the extraction produced that the release does not have — a
  shattered glyph read as two characters, a space put where none belongs;
- a **deletion** is a character the release has that the extraction never produced — two characters
  fused into one component, a word space never split.

A missed word space is therefore a **deletion**, not the insertion #98 filed it under. The buckets
are labelled by what they are and the prediction is scored against the labels.

**`xtask unread` names the glyphs that matched nothing**, on real media, and `xtask accuracy` prints
the same table for the ceiling fixture. Grouped by component size rather than listed one per glyph,
with the nearest reference distance each size ever achieved — because a glyph rejected at 60 cells
against a 51-cell ceiling is a threshold question and one rejected at 410 is not in the set at all.

## The disc

10 Cloverfield Lane, Arial with an italic cut, post-correction on. 818 scored cues, 24,522
characters, 5.5% CER — the same figures as before, because the census is additive.

```
                                          upright  italic     all
  substitutions                              1135      22    1157
  insertions (read, not in the release)        32       0      32
  deletions (in the release, not read)        117      47     164
```

| release → read | upright | italic | all | share of all errors |
| :--- | ---: | ---: | ---: | ---: |
| `.` → unread | 656 | 4 | **660** | **48.8%** |
| `l` → `I` | 330 | 0 | **330** | 24.4% |
| `y` → unread | 42 | 3 | 45 | 3.3% |
| `t` → unread | 27 | 0 | 27 | 2.0% |
| `"` → `'` | 20 | 0 | 20 | 1.5% |
| `w` → unread | 16 | 0 | 16 | 1.2% |
| *31 other substitutions* | | | 59 | 4.4% |

| deleted — in the release, never read | upright | italic | all |
| :--- | ---: | ---: | ---: |
| `r` | 73 | 6 | **79** |
| space | 28 | 38 | **66** |
| *everything else* | 16 | 3 | 19 |

Insertions are 32 in total, of which 14 are `'` — the documented `"`-reads-as-two-quotes case in
[`group.rs`](../crates/subtrackt-glyph/src/group.rs).

**Two error classes are 73% of the disc.** The full stop, which matches nothing at all, and `l`
read as `I`, which is the pair that sits at distance zero and has been named in this repository
since #10.

### The unread glyphs behind it

`xtask unread` against the same track and set, 775 unread of 20,524 glyphs:

```
   count     w x h  aspect   nearest   metrics  first seen
     667       5x5    1.00        60  measured  cue 18 line 0 at (157, 39)
      41     42x44    0.95        82  measured  cue 31 line 0 at (172, 12)
      27     31x42    0.74        59  measured  cue 21 line 0 at (397, 3)
       9       5x5    1.00        61   unknown  cue 16 line 0 at (76, 39)
       9     57x42    1.36        70  measured  cue 73 line 0 at (455, 4)
       5     69x43    1.60        75  measured  cue 155 line 0 at (606, 14)
       3     49x44    1.11        65  measured  cue 7 line 1 at (418, 86)
       3     43x32    1.34        68  measured  cue 150 line 0 at (410, 12)
       2     33x44    0.75       410  measured  cue 221 line 0 at (2, 2)
       2     28x33    0.85       311  measured  cue 221 line 0 at (40, 13)
       2     49x32    1.53        54  measured  cue 814 line 0 at (323, 12)
       ... six more sizes, one glyph each
```

**676 of 775 — 87% — are a 5×5 square block.** The confusion census counted 668 full stops going
wrong by three different routes. Two instruments that share no code agree on the same character.

The distance is the finding. A 5×5 period is **60 cells** from the nearest entry in a reference set
built from the typeface the material was actually authored in, against a 51-cell ceiling. It is not
a near miss on a marginal glyph; it is a shape that the reference set does not contain, in a set
generated from the right font.

Components that could be fusions — wider than tall, on a measured line — total **22 glyphs, 2.8% of
the unread population**.

## The ceiling fixture

`xtask accuracy`, 9.8% CER, unchanged:

```
  15 unread glyphs in 1 distinct sizes; 0 sat on a line with no metrics
   count     w x h  aspect   nearest   metrics  first seen
      15       4x4    1.00        60  measured  cue 1 line 0 at (330, 35)
```

Every unread glyph in the ceiling case is one size — a 4x4 square block at the fixture's smaller
render, against the disc's 5x5 — and it lands at the same 60-cell distance. Fixture and reference set are rendered from the **same font** here, which is
what makes this the useful half: typeface mismatch is excluded by construction, so a period 60 cells
from its own font's period is a fault in how the two sides are rendered, not in which font was
chosen.

The sentence this replaces — "11 of the 13 unmatched are punctuation" — was directionally right and
had no instrument behind it. It is now 15 of 15, and it is one character rather than a class.

## Predictions, scored

#98's three, one right and two wrong, both misses informative:

- **1. The substitution table is dominated by `l`/`I` and by accented base-letter pairs.** *Half
  right.* `l` → `I` is second at 24%. Accented pairs are **absent**: not one appears anywhere in the
  table. The disc is English and its accented characters are a handful of cues, so the census cannot
  see the effect at all — which is a fact about the material, not a refutation of [#100][issue-100].
  What dominates is a character nobody predicted.
- **2. Insertions outnumber deletions.** *Wrong, and backwards twice.* Deletions outnumber
  insertions five to one, 164 against 32 — and the missed word spaces the prediction reasoned from
  are deletions, so the reasoning was pointing at the bucket it said would lose.
- **3. Fewer than a third of the unread glyphs are fusions.** *Right, by a wide margin.* 2.8%.

[issue-100]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/100

## What it settles

**Candidate C — split/merge recovery of unmatched components — closes.** Its bench was this census
and its condition was that the unmatched population is actually fusions and shattered punctuation.
It is neither: it is 87% one missing reference shape. A de-fusing pass would fire on 22 glyphs out
of 20,524 and could at best recover 0.1% of the track.

**[#99][issue-99] moves to the front, and its third prediction is already confirmed.** #99 predicted
before its own bench that "a period is a few pixels square at 21–50px and letterboxes to a solid
block, while a 96px period letterboxes to a disc; the two are far enough apart to explain why `.`
matches nothing." Both censuses say exactly that, from opposite directions, and the 60-cell distance
puts a number on it. **48.8% of the disc's errors and 100% of the ceiling fixture's unread glyphs
are a single reference-side rendering mismatch.**

**[#49][issue-49] gets its corpus figure.** The word-spacing decisiveness margin has 66 missed spaces
on real material to play for — 28 upright and 38 italic, which is 2.5% of the italic act's
characters against 0.1% of the upright, so spacing is an italic problem on this disc rather than a
general one.

**`l` → `I` at 24% is the second thing worth a candidate**, and it is not currently one. #10 named
the pair at distance zero, #37's line-relative metrics separate `o` from `O` and cannot separate
these two because they are the same height, and post-correction's context arm already fires on them
— **all 363 corrections on this track are `I` → `l` and nothing else**, and 330 of the pair still
come out wrong afterwards. Whatever comes after #99 should be aimed here.

## Reproducing

```console
$ cargo run -p xtask -- gen-reference C:/Windows/Fonts/arial.ttf arial-ri.subtref \
      --name arial-ri --italic C:/Windows/Fonts/ariali.ttf
$ subtrackt extract movie.mkv --reference arial-ri.subtref --format srt -o disc.srt --post-correct
$ cargo run -p xtask -- srt-score disc.srt release.eng.srt
$ cargo run -p xtask -- unread movie.mkv arial-ri.subtref
$ cargo run -p xtask -- accuracy
```
