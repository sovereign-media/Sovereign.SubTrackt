# Where the 5.5% actually is

[#98][issue-98]. The pipeline has read a real Blu-ray at 5.5% CER since #66, and until this was built
nothing in the repository said **which characters** the missing 5.5% were. Every candidate in
[#97][issue-97] was ranked on inference from that gap.

It is one character. **Half the errors on the disc, and every unread glyph in the ceiling fixture,
are the full stop.**

Since this was written, [`reference-rendering.md`](reference-rendering.md) fixed that one character
and halved the disc's error rate. The last section here is what the census says *after* that, and it
includes a correction to one of the conclusions below.

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
the unread population**. **That sentence is wrong**, and the section below has the correction: a
fused `rt` is *square* and a fused `ry` is narrower than it is tall, so an aspect-ratio test finds
neither. Read on before believing it.

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
- **3. Fewer than a third of the unread glyphs are fusions.** *Right on the number and wrong on the
  measurement, which cancelled out.* The figure quoted above, 2.8%, came from an aspect-ratio test
  that misses square fusions; the honest figure at the time was about 13%, still under a third.
  After [`reference-rendering.md`](reference-rendering.md) removed the full stops it is **90%**,
  because the denominator was the thing that got fixed.

[issue-100]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/100

## What it settles

**Candidate C — split/merge recovery of unmatched components — looked closed and is not.** Its
bench was this census and its condition was that the unmatched population is actually fusions and
shattered punctuation. At the time it was neither: it was 87% one missing reference shape. Two
things then changed that, and the section below is the correction — the fusion test used here was
wrong, and [`reference-rendering.md`](reference-rendering.md) removed the full stops that were
crowding the population. **C is now 28% of what is left.**

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

## The four candidates #97 held, settled on counts — and one of them was closed wrongly

[#97][issue-97] promoted four proposals to sub-issues and **held** four more, each for the same
reason: the bench is a count, and if the count comes back small the count is the whole write-up.
All four counts are now in. Three close. **One was closed above on a bad measurement and reopens as
the second-largest opportunity left on the disc.**

[issue-97]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/97

### C — recovering an unmatched component by splitting it. **Reopened.**

The section above closed C on the sentence "components that could be fusions — wider than tall, on a
measured line — total 22 glyphs, 2.8% of the unread population". **That test was wrong**, and it was
wrong in a way worth recording rather than quietly fixing.

A fused `rt` at 1080p is **42×44 — square**, because `r` and `t` are both narrow. A fused `ry` is
31×42, *narrower than it is tall*. An aspect-ratio test finds neither, and it was measuring
punctuation against nothing.

Width against the line's own **cap height** is the scale-free measure and it is no better: the same
disc's fusions run from **73% to 200%** of it. `rt` is 73% because both letters are narrow.

What settles it is reading the text where the components sat, which the size groups make cheap.
Forty-one components of exactly 42×44, each 82 cells from the nearest entry in the material's own
typeface, are forty-one instances of one recurring *pair*:

```
  want: Please don't hurt me.        got: PIease don't hu?  me.      31x42 = rt
  want: I'm sorry.                   got: I'm sor?                   42x44 = ry
  want: One year, maybe two.         got: One year, maybe ?o.        57x42 = tw
  want: She never went anywhere      got: She never went an?here     69x43 = yw
  want: You know, everywhere.        got: You know, eve?here.        84x44 = ryw
  want: Yeah, that's awful.          got: Yeah, that's a?uI.         60x42 = wf
```

**Of 105 unread glyphs, 6 are full stops, about 5 are a graphic in two cues, and the remaining ~94
are fusions.** Each costs two characters — a placeholder where the pair belonged and the second
letter missing entirely — and the confusion census agrees to within a few characters:

| the same events, counted from the text | errors |
| :--- | ---: |
| `y`, `t`, `w`, `v`, `f` read as unread | 95 |
| `r`, `t`, `y`, `f`, `w` never read at all | 98 |
| **total** | **193** |

**193 of the 687 errors left on the disc — 28% of them.** C is the second-largest class after
`l` → `I`, and after [`reference-rendering.md`](reference-rendering.md) removed the full stop it is
what the unread population now *is*. It is
[#106](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/106), with its prediction
recorded there.

Two things it changes about C as #97 specified it:

- **The split trigger cannot be width.** #97 proposed cutting "a component whose width against its
  line's cap height exceeds any single reference character's". `rt` at 73% never reaches that
  trigger. The trigger has to be simply *unread, on a measured line, at ordinary glyph size* — which
  is safe, because C's acceptance criterion does the work: cut at the column projection's minima and
  accept **only** if every part matches within the ceiling.
- **The merge half is separately justified.** `"` read as two single quotes is still 20 errors and
  still the documented case in `group.rs`.

`r` is the common factor in almost all of it, and `docs/glyph-stability.md` records why 8-connected
labelling fuses characters that touch at a corner as *accepted behaviour with no de-fusing pass
anywhere in the tree*. An `r`'s arm reaches over the letter after it. That is the pin, and this is
the measurement that says what it costs.

### E — zero-radius cross-instance voting. Closed on the count.

**32 glyphs in 20,524.** `xtask shape-votes` groups every glyph in a stream by its shape vector
alone, scans each distinct cache key the way the runtime does, and counts the shapes that receive
more than one answer.

```
  20524 glyphs, 154 distinct shapes, 354 distinct cache keys
  12 of 154 distinct shapes received more than one answer

    glyphs   majority  the answers this shape received
      1563          o  unread x1   o x1562
      1358          a  unread x1   a x1357
       982          i  i x979     Í x3
       965          I  I x956     Í x9
       676          .  unread x6  . x670
       465          y  unread x1  y x464
```

The proposal was sound, and it is not #10: the radius is exactly zero, no two reference entries are
ever merged, and the objection that killed clustering cannot apply. The majority is also *right* in
every case listed. There is simply almost nothing to aggregate — **32 glyphs sit on the losing side
of a split, 0.16% of the stream**, and only if the majority is right every time.

Worth keeping from the count: 154 distinct shapes produce **354 distinct cache keys**, so the
reference set is scanned 2.3 times per shape. That is the price of keying the cache on metrics and
mark as well as shape, and it is the number to look at if scan count ever matters.

### F — restricting candidates to the line's own typographic cut. Closed on the census.

The hold was that #66 had already taken the italic act from 35.7% to 4.7% by putting both cuts in
one set, so the remaining headroom was small — and to promote F if the census said otherwise.

It does not. After [`reference-rendering.md`](reference-rendering.md) the disc reads **2.7% upright
and 4.3% italic**, and the italic act is 1,505 of 24,522 characters. Taking italic all the way to the
upright rate would recover **24 characters, 0.1 points of CER**. #97's own figure for what carrying
an extra cut costs is **0.2 points and 17% more ambiguous glyphs** — so the price of the machinery
exceeds the whole of what it could win.

### G — a faded-in cue keeping the palette it opened with. Closed at zero.

`pgs/mod.rs` drops a repeat composition when the bitmap and position match, and the open cue keeps
whichever palette it opened under. During a fade-in that would be the most transparent step in the
sequence, and the binarizer rejects anything under half alpha — so such a cue could reach the
matcher thin or empty.

Measured two ways, because the direct count and the operational effect are different questions:

- **Instrumented count.** Not one dropped repeat, on either of two discs, carried a palette more
  opaque than the one retained. **Literally zero**, over 2,588 cues.
- **The change, made and measured.** Keeping the most opaque palette a composition is ever seen
  under produces a **byte-identical** extraction on both discs.

So the hazard is real in the format and absent from this material: these discs do not author fades
as palette-only updates over a fixed bitmap. The change was written, measured, and **reverted**,
which is what #97 pre-committed to. It is four lines whenever a disc turns up that needs it, and the
instrumented count is how to tell.

### What is left on the disc

| release → read | errors | share of what is left |
| :--- | ---: | ---: |
| `l` → `I` | 330 | **48%** |
| **fusions** (`rt`, `ry`, `tw`, `yw`, `wf`) | **193** | **28%** |
| word spaces never read | 66 | 10% |
| everything else | 98 | 14% |

Two classes are three-quarters of it, and they are completely different problems. Fusions are a
segmentation fault with a bounded, checkable fix — C, above. `l` → `I` is the pair #10 measured at
distance **zero**: the same 256-bit vector, the same height, so neither the shape representation nor
#37's line metrics carries a single bit that separates them. Post-correction's context arm already
fires on it and on nothing else — all 363 corrections on this track are `I` → `l` — and 330 of the
pair still come out wrong.
