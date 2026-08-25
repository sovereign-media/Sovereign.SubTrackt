# The space in front of a `j` measures a quarter narrower than it is

[#219][issue-219]. Gone Girl fuses `jag` to the word in front of it **80 times** in Swedish and
`jeg` **62 times** in Norwegian. Its English track — same disc, same typeface, same layout engine,
same authoring — fuses `you` **six times**.

Six instances is noise the bench cannot rank a change by, which is why this defect has been in the
pipeline the whole time and has never been measurable. It is not a Scandinavian bug. It is an
English bug that only a non-English track has enough of to see.

**The gap the assembler measures is between bounding boxes. The gap a reader sees is between ink.
For most letters those are the same number. For `j` they differ by 29 points.**

[issue-219]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/219
[issue-189]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/189
[issue-121]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/121
[issue-40]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/40
[issue-222]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/222
[issue-225]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/225
[issue-226]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/226

## The instrument

`xtask word-gap` needs no sidecar, no dictionary and no ground truth, which is what lets it run on
the Swedish and Norwegian tracks at all. For every pair of glyphs standing next to each other on a
line it records two distances:

- the **box** gap, `next.x - previous.right()`, which is what `SpatialAssembler` measures and what
  `split_threshold` classifies;
- the **ink** gap, the narrowest horizontal distance between the two glyphs' own ink over the rows
  they share — `None` where they share no row at all, because two glyphs that never face each other
  have no horizontal distance worth naming.

Each line supplies its own median glyph width and its own split threshold, so every gap is scored
against the line it came from, and a gap is a word break if that line's threshold says so. No
external evidence enters at any point.

## What a word space actually measures

Gone Girl's English track, 11,398 word breaks. Both columns are percentages of the line's median
glyph width; `left ink at` is the height of the letter's leftmost ink above the baseline, as a
percentage of cap height.

| letter | box | ink | within | breaks | left ink at |
| :--- | ---: | ---: | ---: | ---: | ---: |
| `j` | **62%** | **91%** | 35% | 66 | **−23%** |
| `w` | 67% | 75% | 4% | 819 | 71% |
| `y` | 69% | 80% | 8% | 573 | 71% |
| `A` | 70% | 76% | 4% | 102 | 2% |
| `t` | 75% | 81% | 12% | 1,424 | 68% |
| `a` | 76% | 80% | 16% | 901 | 20% |
| `o` | 76% | 83% | 14% | 423 | 37% |
| `T` | 80% | **126%** | 12% | 41 | 95% |
| `h` | 84% | 84% | 16% | 687 | 50% |
| `b` | 84% | 84% | 23% | 479 | 50% |
| `n` | 84% | 85% | 24% | 278 | 37% |
| `i` | 88% | 88% | 20% | 482 | 50% |
| `k` | 88% | 88% | 21% | 191 | 50% |
| `r` | 88% | 88% | 23% | 201 | 37% |
| `N` | 95% | 95% | 29% | 89 | 50% |
| `I` | 100% | 100% | 29% | 234 | 50% |

Every one of those break-class gaps is the same physical thing: the space a studio set between two
words. It does not get narrower in front of a `j`.

**Where `box` equals `ink`, the letter is safe.** `h`, `b`, `i`, `k`, `r`, `n`, `I`, `N` — upright
stems whose leftmost ink is a vertical edge — read 84% to 100% on both columns and never glue.

**Where `box` is smaller than `ink`, the space is being measured short**, and the four letters at
the top of the table are `j`, `w`, `y` and `A` — which are precisely the letters this defect was
found in. `ifyou`, `hearyou`, `knowyou`, `protectyou`.

`T` is the extreme case and it goes the same way: its box gap reads 80% while its ink gap reads
**126%**, a 46-point understatement, because its crossbar makes the box wide while at the baseline
there is nothing there at all.

Swedish and Norwegian put the same three letters at the top of their own tables:

| | `j` box / ink | `A` box / ink | `T` box / ink |
| :--- | :--- | :--- | :--- |
| English | 62% / 91% | 70% / 76% | 80% / 126% |
| Swedish | 65% / 91% | 68% / 74% | 82% / 115% |
| Norwegian | 65% / 88% | 65% / 73% | 73% / 103% |

## The mechanism, and where #219 had it half right

#219 predicted the ranking would follow "how high the leftmost ink starts". It does not, and the
correction is the useful part: **it is a property of the pair, not of the letter.**

The box gap and the ink gap agree exactly when both facing edges are vertical and at the same
height. They diverge when one glyph's box is widened by ink at a height the other glyph does not
occupy — and it does not matter which direction. `j`'s box reaches left because of a descender hook
below the baseline; `T`'s reaches right because of a crossbar at cap height. Both understate.

The pair table on the Swedish track says it directly. `right ink at` is the preceding letter's
rightmost ink, `left ink at` the following letter's leftmost:

| pair | box | ink | breaks | right ink at | left ink at |
| :--- | ---: | ---: | ---: | ---: | ---: |
| `e` `j` | 57% | 84% | 20 | 34% | −23% |
| `n` `j` | 65% | 92% | 29 | 27% | −23% |
| `r` `e` | 68% | 84% | 88 | 69% | 36% |
| `r` `d` | 65% | 80% | 312 | 69% | 37% |
| `t` `d` | 72% | 88% | 122 | 2% | 37% |
| `t` `o` | 72% | 88% | 34 | 2% | 37% |

`r` is a high shoulder meeting round letters that begin at x-height. `t` is a tail that curves at
the baseline meeting the same. `e j` is the worst pair on the track at 57% against 84% — a
27-point understatement, and it is the shape of `Närjag`.

### The ink is separated; the boxes are not

That answers #219's second question outright. This is a **layout** defect, not a segmentation one:
by the time `is_space` decides, the two glyphs are still cleanly apart — 91% of a glyph width apart,
in front of a `j` — and the number handed to the decision is 62%. Nothing needs to be re-segmented.
The measurement is looking at the wrong rectangle.

The `within` column is the other half of why it fires. The split threshold has to find a boundary
between two populations, and for `j` the break class falls to 62% while the within-word class rises
to 35%. The two ends close on each other at once, which is exactly the condition
[#40][issue-40]'s decisiveness tests exist to detect and cannot when the gap distribution is drawn
from a whole line.

## The fixture is nearly blind to it, and the one thing it catches is the mechanism

`xtask word-gap` also renders 676 lines — one per ordered pair of lowercase letters,
`HonP Lnop anan onon`, with the pair under test flanked by two control breaks in neutral company —
and reads them back against a reference set generated from the same font. Ground truth by
construction: the spaces are where they were put.

At 56px, **603 pairs keep their space and exactly one loses it.**

That one is `f` `j`. Its facing edges are 121 percentage points apart — `f`'s hook at 98% of cap
height, `j`'s hook at −23% — which is the **largest offset in the entire 676-pair grid**, and it is
the only cell in the grid where the offset exceeds 100.

| offset between facing edges | lost | of | rate |
| :--- | ---: | ---: | ---: |
| 0–19% | 0 | 257 | 0% |
| 20–39% | 0 | 208 | 0% |
| 40–59% | 0 | 81 | 0% |
| 60–79% | 0 | 51 | 0% |
| 80–99% | 0 | 6 | 0% |
| **120–139%** | **1** | 1 | **100%** |

So the fixture reproduces the mechanism and cannot price it. A clean render at a comfortable size
sets its word space wide enough that only the single most extreme pair in the alphabet falls
through, while the disc loses 142 spaces across two tracks. That is the same lesson
[`language-coverage.md`](language-coverage.md) records from the other end and the one the fixture's
own documentation states: **treat the generated fixture as a ceiling.** It kills proposals; it does
not accept them.

It is also worth recording *why* the fixture had to be rendered at 56px. At 33px — the library
survey's median glyph height — 471 of 676 pairs came back unreadable, not because of spacing but
because the matcher could not read the letters. The first version of the harness was worse still:
its line was `nonP Lnon anan onon`, all x-height, so the line had no cap line for
`metrics::measure_all` to work from, every glyph was matched on shape alone, and it read back as
`?O?? ??O? ???? O?O?` — 651 pairs discarded. **A synthetic line has to be given an ascender
deliberately**, because a real subtitle line always has one.

## The fix: two attempts, and the second one ships

[#222][issue-222] built it. `UprightBands` carries a glyph's deskewed ink in **four bands** down its
line — above the cap line, the two halves of the body, below the baseline — and a word gap becomes
the narrowest distance over the bands both glyphs reach. The bands are fractions of the line's own
measured cap height, never a typographic constant, and a line whose anchors were not found reports
them unknown and falls back to the box.

**It removed 69% of the defect and made 600 cues worse.** Every scored track on the bench.

The mechanism was the one #222 predicted in advance, down to the character. A full stop has ink in
**one band only**; the band it shares with the letter before it holds that letter's narrow foot; and
the honest distance there is genuinely wide.

```
He said they were God's second blunder.   ->   ... second blunder .
So, you're Wanda's brother.               ->   ... Wanda's brother .
```

A *third* population had appeared in the line's gap distribution. #40's two decisiveness constants
sit where two populations separate, and there were now three.

### No setting of the constants works

[#225][issue-225] swept both, whole pipeline at each setting, scored against a release subtitle —
`xtask gap-sweep`. Wanda, against its SDH sidecar, with the shipped 50/200 in the top row:

| shared bands | width floor | cluster floor | CER | worse |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 50 | 200 | 1.8% | 124 |
| 1 | 50 | 300 | 2.4% | 250 |
| 1 | **80** | 200 | **6.8%** | 846 |
| 1 | 110 | 200 | 14.3% | 1,180 |

**Prediction 1 held.** Raising a floor to suppress the punctuation gaps also refuses real word
breaks: at 80% the whole line stops splitting, and the glued-word count goes *up*. There is no
window.

### What works is removing the population, not moving the threshold

`band_gap_min_shared` — how many bands two glyphs must share before the ink measurement answers for
them at all. A full stop shares one. Requiring **two** hands that pair back to the box, which had it
right by accident.

| shared bands | worse on Wanda | Swedish glued `jag` | Norwegian glued `jeg` |
| ---: | ---: | ---: | ---: |
| box gap (before) | — | 80 | 62 |
| 1 | 124 | 25 | 19 |
| **2** | **8** | **22** | **19** |
| 3 | 1 | 71 | 53 |
| 4 | 0 | 80 | 62 |

Two is the whole window. **Three gives the defect back**, because `j` shares exactly two bands with
the letter before it — the letter has no ink below the baseline, which is precisely why `j`'s hook
widens its box unopposed.

### What ships

`--band-gaps` **on**, `band_gap_min_shared` at 2:

| track | CER before | CER after | better | worse |
| :--- | ---: | ---: | ---: | ---: |
| cloverfield | 0.4% | **0.3%** | 8 | 4 |
| gonegirl | 1.4% | **1.3%** | 25 | 3 |
| wanda | 1.3% | 1.3% | 7 | 8 |
| kingkong | 21.3% | 21.3% | 10 | 0 |
| karate-kid | 1.8% | **1.7%** | 13 | 2 |
| training-day | 1.9% | 1.9% | 3 | 0 |

**66 cues better, 17 worse.** Word error falls on every track that moves — Cloverfield 1.8% to 1.6%,
Gone Girl 6.1% to 5.7%, Karate Kid 7.3% to 7.0%. The ceiling fixture stays at 0.0%. And the
Scandinavian tracks, which are the only place the gain is large enough to see, drop from 80 glued
`jag` to 22 and from 62 glued `jeg` to 19.

That is not #110's shape, which is why it ships: #110 gained character error *while* making 232 cues
worse, and this improves both columns at once.

### The 17 that are left

Two classes, and one of them is not a regression at all.

```
before  Ken, somebodyjust called!      <- the extraction
after   Ken, somebody just called!     <- the extraction, corrected
want    Ken, somebodyjust called!      <- the sidecar, which has the glue
```

The rest are `T` followed by a letter — `T reasUre`, `T o Us`, `T Wo`.

[#226][issue-226] asked whether moving the band boundary reaches them, and the answer is **no, and
not for a reason about resolution.** Asking the same instrument for the pairs *inside* words, on Gone
Girl's English track:

| pair | box | ink | seen | right ink at | left ink at |
| :--- | ---: | ---: | ---: | ---: | ---: |
| `- T` | 12% | **68%** | 20 | 37% | 95% |
| `Y e` | 8% | **60%** | 34 | 100% | 36% |
| `Y o` | 7% | **53%** | 197 | 100% | 37% |
| `r .` | 15% | 57% | 161 | 69% | 8% |
| `y .` | 19% | 53% | 158 | 72% | 8% |
| `k e` | 12% | 36% | 195 | 2% | 36% |

These are **within-word** pairs measuring 53% to 68% of a glyph width between their ink, against real
word breaks at 76% to 88%. A `Y` followed by an `o` nests the `o` under its arm — and the arm is at
100% of cap height while the `o` reaches 37%, so **they never share a row where their near edges
are.** The honest distance at the rows they do share is between `Y`'s stem and `o`'s body, and that
is genuinely wide.

No banding recovers that, at any resolution, including row-exact: the ink is not close anywhere. The
model — *distance between ink over shared rows* — is simply the wrong one for a kerned pair, and the
same property that makes it right for `j` makes it wrong here. `j`'s hook is far from the letter
before it **and looks it**; `Y`'s arm is far from the `o` and does not.

What would work is a **closest approach in two dimensions** rather than a horizontal distance at
shared rows, which is a different measurement with a vertical weighting to choose — and #225 is the
standing evidence about choosing a constant here. It has not been built. The residue is 17 cues
against 66 better, and the two-shared-band filter already removes every case where the two glyphs
face each other over one band only, which is where the effect is largest.

**Prediction 2 is refuted in the good direction** — it said the two-shared-bands rule would remove
more than half the regression and still not reach parity. It removes 97% and clears parity.
**Prediction 3 is refuted too**: it said the English bench could only ever refuse this change, and
the English bench is where 66 of the better cues are.

One thing #222 got wrong in the useful direction: **the memory cost is nothing.** Four bands is 40
fixed bytes per glyph and the bench's resident glyph figures do not move — Gone Girl reads 220.4 MiB
either way. #222 had priced a row-exact profile at 8 MB and a full mask copy at more. Storage was
never the obstacle; the tuning was.

## What this does not claim

- **It does not say how many spaces are lost.** The box-versus-ink table is over the breaks that
  *survived*; the ones that did not are not in it, because a lost break is not classified as a
  break. The 80 and 62 glued instances are counted separately, by looking for `jag` and `jeg` fused
  to a preceding word, and that count is specific to two words in two languages.
- **The four bands are an approximation of the row-exact measurement**, and the two have not been
  compared cell by cell. `xtask word-gap` computes the exact answer off the glyph masks; the shipped
  bands compute a coarse one off the label map. Everything the fix gains and everything it costs is
  measured with the coarse one — though the residue above shows the approximation is not what limits
  it, since the row-exact answer is wrong for those pairs too.
- **`ink` is a horizontal distance over shared rows**, which is a model rather than a fact. It is
  right for `j` and wrong for `Y o`, and nothing in this document distinguishes the two except by
  naming them.
- **One disc.** Three tracks of it, which is the right shape for the question — the language is the
  only variable — and still one disc.

## Reproducing

```console
$ cargo run --release -p xtask -- dump-sup "...Gone Girl...mkv" swe.sup --stream 13
$ cargo run --release -p xtask -- word-gap C:/Windows/Fonts/arial.ttf --px 56 \
      --media swe.sup --reference arial-ri.subtref
$ cargo run --release -p xtask -- gap-sweep wanda.sup arial-ri.subtref wanda.eng.SDH.srt \
      --glued you --widths 50,65,80 --clusters 200,300 --shared 1,2,3
$ cargo run --release -p xtask -- gap-sweep swe.sup arial-ri.subtref --glued jag --shared 1,2,3
```

`gap-sweep` runs the whole pipeline once per setting and scores each result, which is the shape
`xtask width-sweep` established. Its `glued` column takes no sidecar, which is the only reason the
Swedish and Norwegian tracks can be scored at all.

The fixture half runs on the font alone and takes a second. The disc half needs a `.sup` and a
reference set, and turns `Config::glyph_masks` on for the pass — it is the only command in the tree
that has a reason to.
