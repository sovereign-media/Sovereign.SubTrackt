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

## Cutting the components that were two characters

[#106](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/106), built after the section
above measured it. **The disc reads at 2.1% character error, down from 2.8%, and 99.9% of its glyphs
are read.** 87 cues improved and **none got worse**.

| | before | after |
| :--- | ---: | ---: |
| character error, all cues | 2.8% | **2.1%** |
| upright | 2.7% | **1.9%** |
| glyphs read | 99.5% | **99.9%** |
| unread components | 105 | **16** |
| errors | 687 | **508** |

The `r` deletions are gone — 79 to zero — and so are `t`, `w` and most of `y` read as unread. 179 of
the 193 characters the census attributed to fusions came back, which is 93% of them.

### What it does

After matching an image's glyphs, any component the matcher returned `unmatched` for is offered to
[`split::cut_columns`](../crates/subtrackt-glyph/src/split.rs): the column projection's local minima,
lightest first, restricted to columns carrying less than 40% of the component's median ink and
leaving both sides at least 20% of its width. Each candidate cut is tried, both parts are cropped
back to their own ink, vectorized and matched, and the cut is kept **only if every part reads**. A
part that does not read gets one more cut, which is what a three-character fusion needs — the disc
has one, `ryw` in `everywhere`.

### Why it is on by default, which nothing else here is

The acceptance rule, not the size of the gain:

- It runs **only** on components the matcher already returned `unmatched` for. A glyph that read is
  never seen by it.
- A cut is kept **only** if every part matches within the ceiling. Half a fusion read and half not is
  refused outright, because that would be a wrong answer with a plausible shape — worse than the
  placeholder it replaced.

So the failure mode is bounded to **unread → read**, which is the direction the accuracy gate
measures anyway, and `srt-score --compare` confirms it on the material: 87 cues better, **0 worse**.
`docs/post-correction.md` sets the standard a recovery stage has to meet — "a corrector that fixes
three characters and invents one has still turned a detectable failure into a plausible wrong answer
once" — and this is what meeting it looks like.

That is also why the trigger can be as loose as it is. A wrong cut costs nothing: its parts fail to
match and the next candidate is tried, or the glyph stays unread exactly as before.

### Why the trigger is not a width test

#97 proposed cutting "a component whose width against its line's cap height exceeds any single
reference character's". The census above is why that does not work: a fused `rt` is **31×42, narrower
than it is tall**, 73% of its line's cap height, because `r` and `t` are both narrow. The disc's
fusions run from 73% to 200% of cap height and no threshold separates them from single characters.

The trigger is therefore just *unread*, and the acceptance rule carries the whole argument.

### What it costs

**2.4 seconds on a 5.5 GB rip**, 14.8 to 17.2. The mask is recomputed for an image only when
something in it failed to read — about a hundred images in a feature film — and each recomputation is
one pass over the composed object, not the plane.

Set size, format and dependencies are unchanged. The parts are matched through the ordinary
`match_glyph`, so they go through the session cache like everything else.

### The parts have to know what line they stood on

A part's line metrics are the one thing that is not obvious. `metrics::measure_all` ran over the
*old* segmentation and cannot be re-run — the glyph it measured no longer exists. But the parent
carries its height and descent as percentages of its line's cap height, so the cap height in pixels
is recoverable from the pair, and each part's metrics are derived from that.

It matters more than it sounds: `r` and `t` differ in height and in very little the shape vector
keeps. A part scored on shape alone would be a coin toss between them. And a glyph whose line had no
metrics at all is **refused** rather than given fabricated ones, which is the choice
`LineMetrics::UNKNOWN` makes everywhere else in the pipeline.

### Predictions, scored

- **1. The split recovers more than half of the 193 characters.** *Right.* 179 of 193, 93%.
- **2. It recovers nothing it should not: zero cues get worse.** *Right.* 87 better, 0 worse, across
  818 scored cues.
- **3. The disc goes from 2.8% to under 2.4%.** *Right.* 2.1%.
- **4. The ceiling fixture barely moves, and is the case where a fixture cannot see the bug.**
  *Right, and completely.* 6.4% before and after, 2 unread before and after — **the fixture does not
  move by a single character**, because text generated at 48px does not fuse. Every number in this
  section comes from the disc, and there was no other instrument that could have produced them.

### What is left

| release → read | errors | share |
| :--- | ---: | ---: |
| `l` → `I` | 330 | **65%** |
| word spaces never read | 66 | 13% |
| `"` → `'` | 20 | 4% |
| everything else | 92 | 18% |

`l` → `I` is now two thirds of it. It is the pair #10 measured at distance **zero** — the same
256-bit vector, the same height, so neither the shape representation nor #37's line metrics carries
a single bit that separates them. Post-correction's context arm already fires on it and nothing
else, and 330 still come out wrong.

## The evidence that separates `l` from `I`, and why nothing had seen it

[#109](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/109). The section above calls
`l` → `I` "the first accuracy issue this project has had where the problem is not evidence nothing
consults but **evidence that does not exist**". That is wrong, and the correction is worth more than
the finding: the evidence exists, it is in the font file, and the reason no instrument had ever
reported it is that **the one number that carries it is measured at too low a fidelity to survive**.

In Arial's outlines, `I` is **7% wider than `l`** — 13.11% of cap height against 12.30%. At
[`RENDER_PX`](../crates/subtrackt-glyph/src/font.rs), the 96 pixels a reference set is generated at,
that difference is **six tenths of one pixel**. The rasteriser rounds both stems to nine pixels wide
and they become the same shape, at distance zero, exactly as #10 measured. It reappears at 128px and
is stable from 256 up:

```
  thr 128     96px cap   69   i:  9px 13.04%   l:  9px 13.04%   I:  9px 13.04%
  thr 128    128px cap   92   i: 11px 11.96%   l: 11px 11.96%   I: 12px 13.04%
  thr 128    256px cap  183   i: 22px 12.02%   l: 23px 12.57%   I: 24px 13.11%
  thr 128    512px cap  366   i: 45px 12.30%   l: 45px 12.30%   I: 48px 13.11%
  thr 128   1024px cap  733   i: 90px 12.28%   l: 89px 12.14%   I: 96px 13.10%
```

Nothing is wrong with the typeface, the reference set or the matcher. One measurement is taken at a
resolution below the thing it would have to show, and everything downstream inherits the answer.

### The instrument

`xtask glyph-geometry <media> <set> <release.srt> [--font F] [--italic F]`. It needs something no
bench here had: a **label**. `xtask unread` describes glyphs the matcher would not call and
`xtask srt-score` counts characters the release disagrees with, but neither can point at a glyph and
say *the release says this one was an `l`*. This aligns the read text against the release cue by cue
and carries the alignment back to the glyph behind each character, so every measurement below is a
distribution over glyphs of known identity.

The alignment is [`disc::trace`], the same traceback the confusion census runs, read for its
*matches* rather than its errors — one pass, two readings, so a label and a census row can never
disagree about what happened in a cue.

[`disc::trace`]: ../xtask/src/disc.rs

### What the disc drew

818 paired cues, 20,328 labelled glyphs, upright act only:

| measured on the disc | `l` (n=612) | `I` (n=255) | best cut errs on |
| :--- | ---: | ---: | ---: |
| **ink width, pixels** | 5.00, sd 0.00 | 6.00, sd 0.00 | **0.0%** |
| ink width, % of cap height | 11.86, sd 0.22 | 14.29, sd 0.56 | 0.2% |
| advance to the next character | 27.43, sd 3.45 | 29.49, sd 3.31 | 14.0% |
| gap to the next character | 15.57, sd 3.42 | 15.08, sd 2.75 | 14.1% |

**Every `l` on this disc is exactly five pixels wide and every `I` is exactly six**, with no spread
at all in either class. The pair that sits at distance zero in the reference set is not ambiguous in
the material for a moment.

### The number that decides it is not the best cut

A threshold chosen on the disc's own two populations is guaranteed to separate them and says nothing
about whether anything could have *predicted* it — it is the absolute-pixel constant `CLAUDE.md`
forbids, fitted. The figure that matters is the cut halfway between what the **font** draws, which
is a number a reference set could carry and which nothing on the disc was consulted to choose:

```
  the font draws l at 12.30% of cap height, I at 13.11%
  the threshold halfway between them: 12.70%
    l: 2 of 612 on the wrong side (cue 144 line 1 at 15.6% of a 32px cap; cue 270 line 1, the same)
    I: 0 of 255 on the wrong side
  2 of 867 — 0.2%
```

**865 of 867 glyphs land on the side the typeface predicts.** Both misses are the same case and it
is named rather than swept up: a line whose cap height measured 32 pixels instead of the disc's usual
42, where a stem the renderer would not draw thinner than five pixels is 15.6% of a short cap rather
than 11.9% of a full one. The ratio is scale-free; the *rasteriser* is not, and there is a floor
under how thin a stem it will draw. That is a real hazard at small sizes and it costs two glyphs
here.

### Advance width — #109's candidate A — is the weak one, and it was the prediction

#109 predicted that the advance separates and that the ink cannot, on the grounds that the ink is
what the shape vector already sees. Both halves are wrong, and in the same way: the vector sees the
ink **letterboxed onto a 16-cell grid**, where a fifth of a pixel of cap height is far below one
cell. The ink was never the thing that could not carry it. The quantisation was.

Advance errs on 14% of the pair for a reason worth keeping: it is measured to the *next glyph's* ink,
so it carries that character's left side bearing as noise, and `l` is commonly followed by another
`l`. It is a real signal — 27.4% against 29.5% — buried under a spread three times its size.

### The italic act, where it does not work

| italic, measured on the disc | `l` (n=51) | `I` (n=6) |
| :--- | ---: | ---: |
| ink width, % of cap height | 31.44, sd 5.79 | 20.50, sd 9.86 |
| stem width (ink ÷ height) | 12.75, sd 0.33 | 13.66, sd 0.75 |

A slanted stem's bounding box is mostly slant: Arial Italic draws `l` at 33.06% of cap height and
`I` at 34.43%, so the *relative* difference collapses from 7% to 4% and falls under the pixel grid
again. Dividing the ink by the height takes the slant back out and recovers a stem width close to the
upright figure — but the two classes then sit 12.75 against 13.66 with the cut at 12.86, and 25 of 57
land on the wrong side.

This costs less than it sounds. The italic cut's own `l` and `I` are **3 cells apart** rather than
zero — the slanted boxes have different aspect ratios, so the shape vector already separates them —
and the census records 4 italic errors of this kind against 330 upright. The italic act is not the
problem and a width term must not be allowed to make it one.

### What it settles

`l` → `I` is 65% of what is left on this disc and it is **not** a case of missing evidence. It is one
measurement taken at 96 pixels that needs to be taken at 512, carried on the reference entry beside
the line metrics #37 added, and priced by a swept weight the way every other term here is. That is
[#110](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/110), with its prediction
recorded there.

### Reproducing

`xtask dump-sup` is what makes this affordable. The disc is 5.5 GB on a network share and its
subtitle track is 16 MB; the dump is a re-framing rather than a transcode, and extracting the rip and
extracting the dump produce byte-identical subtitles.

```console
$ cargo run -p xtask -- dump-sup movie.mkv clover.sup
$ cargo run -p xtask -- glyph-geometry clover.sup arial-ri.subtref release.eng.srt \
      --font C:/Windows/Fonts/arial.ttf --italic C:/Windows/Fonts/ariali.ttf
```

## Carrying the ratio the grid rounds away

[#110](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/110), built after the section
above found the evidence. **The disc reads at 0.8% character error, down from 2.1%**, and the
ceiling fixture at **1.2%**, down from 6.4%.

| | before | after |
| :--- | ---: | ---: |
| character error, all cues | 2.1% | **0.8%** |
| upright | 1.9% | **0.6%** |
| italic | 4.1% | 4.1% |
| `l` → `I` | 330 | **2** |
| ceiling fixture, CER | 6.4% | **1.2%** |
| ceiling fixture, WER | 22.1% | **2.9%** |
| post-corrections made | 363 | **3** |
| `I`/`l` distance in the set | 0 | 1 |

199 cues improved and 12 got worse. Extraction time and the pipeline's dependencies are unchanged;
a reference set grows by three bytes per entry, 21.4 KB to 22.3 KB.

### What it does

A reference entry carries the character's **ink aspect ratio** — its width as tenths of a percent of
its own height — measured at 512px rather than at the 96 a set is rendered at. The matcher adds
`difference × weight ÷ 1000`, omitted rather than defaulted when either side lacks it, exactly as
[#37's line metrics](../crates/subtrackt-core/src/glyph.rs) and #48's mark slope are. The cache key
and the cluster distance carry it too, and that half is not optional: an `l` and an `I` on one line
agree in vector, in height, in descent and in mark, so without it the first of the two scanned would
answer for both and the term would never be reached.

### Against the glyph's own height, not against the line's cap height

The first version measured width against the line's cap height, which is strictly more informative —
it says how big a character is as well as how wide. **It measured worse**, and the reason is worth
more than the version that worked:

| | cap-relative width | aspect ratio |
| :--- | ---: | ---: |
| CER | 1.1% | **0.8%** |
| cues worse | 31 | **12** |
| unread components | 43 | **17** |
| `o` → `C` | 32 | 0 |
| `s` → `S` | 20 | 20 |

Cap-relative width inherits every error in the line metrics. On a line whose cap height is found at
the **x-height** — which happens where few glyphs reach the cap line — every glyph measures a third
too tall *and* a third too wide, and the two wrong terms then agree with each other. `xtask
glyph-geometry` finds those lines by name: an `o` on a line whose cap measured 32 pixels instead of
42 reads 87.3% of cap height, and Arial's `C` is 88.25%.

An aspect ratio is a property of one component's own bounding box. It is right on a line nothing
else could measure, and #37's height term supplies the size the two of them together would have
carried anyway.

### The weight, and the window it sits in

`xtask width-sweep` runs the whole pipeline at each setting and scores it against the release,
because the pair this term exists for sits at distance zero in the set and a set-internal statistic
would call it fixed the moment the weight was non-zero — `docs/glyph-stability.md` records what
trusting that cost last time.

```
    weight      CER  upright   italic    l -> I   unread    worse
         0     2.1%     1.9%     4.1%       330       16        0
       196     2.0%     1.8%     4.1%       308       17        0
       340     0.8%     0.6%     4.1%         2       17       12
       440     0.8%     0.6%     4.1%         2       17       12
       540     0.8%     0.6%     4.1%         2       17       12
       580     1.1%     0.9%     4.1%         2       17       62
      1500     8.7%     8.3%    15.2%         2      102      603
```

**The window is 340 to 540 and the shipped setting is 440, its middle.**

The floor is integer rounding, and the arithmetic predicts it exactly. The disc draws an `l` at
11.9% of its own height; the entries say `l` is 12.3% and `I` 13.1%, so the two differences the
matcher compares are **4 points and 12 points**. A weight buys `points × weight ÷ 1000` cells, so
until 12 points reaches one whole cell the two candidates tie and the term does nothing — which
happens at 328, and the sweep's first working row is 340.

The ceiling is disagreement between the disc and the font. Above 540 the term starts overruling
shape for characters the disc draws a few points from what the outlines predict, and by 1500 it is
rejecting them outright rather than demoting them — the same failure `xtask mark-sweep` watched
#48's term produce above 78.

That the floor is set by *rounding* is why the unit is tenths of a percent. In whole percent the
pair is one point apart, and one point at any of these weights is zero cells.

### What it costs: 20 `s` read as `S`

Named rather than folded into the total, because it is the whole of what got worse and it has a
mechanism:

```
  the disc draws s at 75.76% of its own height and S at 75.00%
  the font draws  s at 79.42%                  and S at 77.04%
```

The disc's `s` sits **nearer the font's `S` than its own `s`**, so the term charges the right answer
more than the wrong one and a line whose metrics do not already settle it goes the wrong way. The
cause is a systematic bias, visible across the whole alphabet in `xtask glyph-geometry`'s
calibration table: the disc draws x-height letters 3 to 5 points *narrower* relative to their height
than the outlines do — `o` −5.0, `m` and `n` and `u` −4.3, `s` −3.6 — because at 33 pixels tall one
rounded-away column is 4% of the width. Cap-height characters, being taller, lose 1 to 3.

This is [#99](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/99)'s finding in another
form — the reference side and the runtime measuring the same quantity under different
quantisation — and the fix is likely to be the same shape: carry more than one sample per character
and let the nearer one win. That is
[#113](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/113), filed with its prediction
rather than guessed at here.

### Predictions, scored

- **1. The disc goes from 2.1% to under 1.0%.** *Right.* 0.8%.
- **2. The italic act does not improve and may get slightly worse.** *Half right.* It does not
  improve — 4.1% before and after, unchanged to the character — and it does not get worse either.
  The cap-relative version *did* make it worse, 4.1% to 4.7%, which is one more thing that decided
  between the two.
- **3. The ceiling fixture barely moves.** ***Wrong, and completely.*** 6.4% → **1.2%**, and word
  error 22.1% → 2.9%. The reasoning was that a fixture generated at 48px cannot see this, the way it
  could not see #106. But the fixture's text was written to include `- Is it 1 or l?` and
  `0123456789 O o I l 1` and `Follow the yellow line to Iowa` — it was *built* to exercise this pair
  and was never blind to it. A prediction about an instrument should be checked against what the
  instrument actually contains.
- **4. A weight sweep has a floor and a ceiling and they are far apart.** *Right.* 340 to 540, with
  a hard floor set by integer rounding and a soft ceiling set by rendering disagreement.

### What is left

194 errors, against 508 before:

| release → read | errors | share |
| :--- | ---: | ---: |
| word spaces never read | 66 | **34%** |
| `"` read as two `'` | 40 | 21% |
| `s` → `S` | 20 | 10% |
| everything else | 68 | 35% |

No class is a third of it any more, and the largest single one is a **word space** — [#49][issue-49]'s
decisiveness margin, of which 38 of 66 are in the italic act, on 6% of the track's characters. The
second is the documented `"`-reads-as-two-quotes case in
[`group.rs`](../crates/subtrackt-glyph/src/group.rs), which costs twice per occurrence: a
substitution and an insertion.

## Reading the reference at the size the material is drawn at

[#113](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/113), the residual #110 left.
**The disc reads at 0.7%**, and the 20 `s` read as `S` are gone — but the finding worth keeping is
not the tenth of a point. It is that #110 shipped a reference measurement taken at the wrong
resolution *for the second time in a row*, and that a second and third disc were needed to see it.

### Three discs, not one

Everything from #98 onward was measured on 10 Cloverfield Lane. Two more titles now fit Arial well
enough to score — `subtrackt fit` ranks it first on both, at 11.1 and 12.3 against 22.8 for the
runner-up — and they are what this issue turns on.

| | term off | #110, ratio read at 512px | now, read at 56px |
| :--- | ---: | ---: | ---: |
| 10 Cloverfield Lane | 2.1% | 0.8%, **12** cues worse | **0.7%**, **0** worse |
| Gone Girl | 4.0% | 2.8%, **62** cues worse | **2.8%**, **0** worse |
| A Fish Called Wanda | 4.8% | 4.3%, 232 cues worse | **4.2%**, 202 worse |

Against the pre-#110 baseline the shipped setting is now **196 cues better and 0 worse** on
Cloverfield and **551 better and 0 worse** on Gone Girl. Wanda is the exception and has its own
section below.

### Why 512px was the wrong target

#110 read the aspect ratio at 512px because that is where it converges on the outline's true value.
That is the wrong thing to converge on. A component is *thresholded ink at subtitle size*, and
thresholding at that size drops the partial columns at a glyph's edges — so a real disc draws its
letters narrower than the outline says, and narrower by a bigger *fraction* the shorter the glyph
is:

| | `o` | `m` | `n` | `u` | `s` | `a` | `H` | `B` | `l` |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| outline minus disc, points | +5.0 | +4.3 | +4.3 | +4.3 | +3.7 | +3.2 | +2.2 | +1.6 | +0.4 |
| at 56px, points | +2.2 | +1.9 | +1.7 | +1.7 | +1.7 | +2.2 | +3.8 | +1.2 | +0.6 |

Rasterising the reference at 56px **reproduces** the material's quantisation instead of correcting
for it. `xtask glyph-geometry` prints the whole sweep:

```
        px   mean gap    pairs   glyphs   which way the wrong ones point
       512       2.66     8/15     1909   C->c O->o s->S u->U v->V
       128       3.04     8/15     1909   C->c O->o s->S u->U v->V
        96       3.03     8/15     1909   C->c O->o s->S u->U v->V
        72       2.34     9/15     1889   C->c O->o s->S u->U v->V
        56       1.59    11/15      134   C->c O->o W->w x->X
        44       3.37     9/15     3287   C->c o->O s->S u->U W->w
```

**pairs** counts the confusable pairs — a letter against its own capital, and `l` against `I` —
where the disc's measured ratio sits nearer its own entry than nearer its partner's. At 512 the
disc's `s` is nearer `S`; at 56 it is not, and `s` → `S` disappears from the census.

The half that makes it free: reading at 56 **widens** the gap the term exists for rather than
narrowing it. `l` and `I` are 0.8 points apart in the outlines and **2.5 points** apart at 56px,
because at that size the rasteriser lands their stems on different whole pixels — which is exactly
what the material does. The weight window moves with it, from 340–540 down to **180–200**, and 190
is the middle of what all three discs agree on.

96px and 44px are in the table because they are where the term *stops working altogether*: at both,
`l` and `I` rasterise to the same width and `l` → `I` stays at 308, 679 and 921 across the three
discs. The size that reads this ratio is not free to be anything.

### Where the pair is not there to be read

A Fish Called Wanda is the same typeface at the same glyph height as Cloverfield — and it draws
`l` and `I` **both five pixels wide**:

| | `l` | `I` |
| :--- | ---: | ---: |
| Cloverfield | 5.00px, sd 0.00 | 6.00px, sd 0.00 |
| Wanda | 5.01px | 5.00px (its outliers are fusions) |

No reference set can separate them there, because the distinction is not in the ink. What the term
does on that disc is move a coin toss: before it, every bare stem read `I` and 679 `l` were wrong;
after it, every bare stem reads `l` and 420 `I` are wrong. CER improves by 0.6 points and 202 cues
get worse, which is what a coin landing the other way looks like.

It lands better because `l` outnumbers `I` in English by about five to one, and post-correction's
context arm then recovers the capitals it can. But it is a **guess either way**, and the honest
reading of Wanda is not that the term helped — it is that this pair is undecidable on that material
and the census now says so with a number.

### What is left on Cloverfield

179 errors, against 508 before #110 and 24,522 characters scored:

| release → read | errors | share |
| :--- | ---: | ---: |
| word spaces never read | 66 | **37%** |
| `"` read as two `'` | 40 | 22% |
| `I`, `!`, `i`, `l` read as `Í` | 16 | 9% |
| everything else | 57 | 32% |

Two of the three are segmentation rather than matching — a word space that was never split and a
quotation mark that arrived as two components — and neither is a character the matcher got wrong.

**38 of the 66 word spaces are in the italic act**, which is 6.1% of the track's characters and 34%
of what is left on it. [`italic-slant.md`](italic-slant.md) names the mechanism: a slanted box
overhangs the box after it, so a quarter of an italic line's gaps reach the spacing rule already
saturated at zero and the rule has no band left to cut in.

**#121 fixed that measurement and 31 of the 38 came back.** The disc reads **0.6%** with the italic
act at **2.0%** against 4.1%, and the table above becomes:

| release → read | errors | share |
| :--- | ---: | ---: |
| word spaces never read | 36 | **24%** |
| `"` read as two `'` | 40 | 27% |
| `I`, `!`, `i`, `l` read as `Í` | 16 | 11% |
| everything else | 55 | 37% |

The word space is no longer the largest class on this disc. **29 of the 36 that remain are on
upright lines**, where no amount of deskewing can help and [#49][issue-49]'s decisiveness margin is
the only lever left. On Gone Girl the same change took missed word spaces from 783 to 165 and the
whole disc from 2.8% to 2.0%.

### Predictions, scored

- **1. A works and B is not needed.** *Right, and for the stated reason.* The bias is quantisation
  and quantisation is reproducible: rendering the reference near the material's size reproduces it
  rather than modelling it. What was not predicted is that **one** sample at the right size would do
  it — the issue expected to have to carry several and let the nearer win.
- **2. A costs some ambiguity.** *Wrong — it costs none.* There is no extra entry: the same field is
  read at a different size, so set size, scan count and extraction time are all unchanged.
- **3. Neither reaches zero.** *Right.* `l` → `I` stays at 2 on Cloverfield, and on Wanda the pair
  cannot be read at all.
