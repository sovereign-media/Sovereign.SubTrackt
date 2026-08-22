# Glyph vector stability

Answers [#14][issue-14], which §4 of [#1][issue-1] calls "the first thing to measure": does one
reference vector per character survive bold, italic, anti-aliasing and outline variation, or does
it not?

**It does not.** Not even close, and not even for upright regular text.

[issue-1]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1
[issue-14]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/14

## Method

```console
$ cargo run -p xtask --release -- measure-stability \
    arial.ttf arialbd.ttf ariali.ttf arialbi.ttf
```

62 alphanumeric characters × 4 faces × 5 rendering sizes (24–96 px, bracketing the 21–50 px the
library survey measured) × 3 ink thresholds × 3 edge treatments = **180 renderings per character**,
just under a million intra-character pairs.

Every variant goes through `subtrackt_glyph::feature::vectorize`, the same normalisation the runtime
applies to decoded subtitle bitmaps, so these distances are the ones the matcher would actually see.

The two distributions that decide it:

- **Intra-character** — distance between two renderings of the *same* character.
- **Inter-character** — distance from each character to the *nearest different* character. This is
  the budget: a variant that moves further than this is closer to some other letter than to itself.

## Result

| Distribution | p5 | p25 | p50 | p75 | p95 | max |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| Intra-character, all styles | 11 | 30 | 46 | **65** | 101 | 228 |
| Intra-character, regular upright | 2 | 16 | 31 | **51** | 92 | 198 |
| Inter-character (nearest other) | 9 | **27** | 31 | 40 | 65 | 76 |

Median intra-character distance across all styles is **46**; median inter-character is **31**. Two
renderings of the same letter are typically *further apart* than two different letters are.

Restricting to upright regular — which is most dialogue, with italics reserved for emphasis and
foreign speech — improves it but does not rescue it: p75 of 51 against an inter-character p25 of 27.

## Which variation actually costs

| Axis | p50 | p95 |
| :--- | ---: | ---: |
| Anti-aliasing threshold | 8 | 35 |
| Rendering size | 11 | 38 |
| **Outline / edge, ±1 px** | **30** | 73 |
| Weight (bold) | 38 | 81 |
| Slant (italic) | 47 | 82 |
| Weight + slant | 55 | 93 |

Two things stand out.

**Scale and anti-aliasing are cheap.** Rendering size moves a glyph 11 cells at the median across a
4× range, and threshold variation 8. That independently confirms the normalisation in #7 works:
these were the axes it was designed to absorb, and it absorbs them.

**A one-pixel edge shift costs as much as character identity.** Nudging the boundary in or out by a
single pixel moves a vector 30 cells at the median — indistinguishable from the 31-cell median
distance to an entirely different letter. This is the axis nobody would have guessed was the
expensive one, and it is not a rendering artefact: it is exactly what varies when a subtitle glyph
is thresholded out of an anti-aliased, outlined source, which is every glyph this tool will ever
see.

## Conclusion

**One reference vector per character does not work.** §2D of #1 assumed it would.

**Per-variant reference entries do not rescue it either**, so the set-size estimate #14 asks for is
moot. The overlap is present within upright regular text alone, so enumerating styles multiplies the
set without separating the distributions — and it would make things worse, since more entries mean
more chances for a wrong character to sit nearer than the right one.

**The session cache becomes the mechanism, not an optimisation.** This is the second of the two
outcomes §4 of #1 anticipated, and it is the one the evidence supports:

> match by self-consistency within a stream, using the reference set only to seed labels for cluster
> centroids

The reason this works where a fixed set does not is visible in the table above. The expensive axes —
edge thickness, weight, slant — are *constant within a stream*. One title is authored by one encoder
with one palette and one typeface, so its own glyphs vary only along the cheap axes. Clustering a
title's repeated shapes cancels precisely the variation that defeats a fixed set, and matching
cluster centroids against the reference only has to survive what is left.

This corroborates the library measurement in [library-survey.md](library-survey.md) from an
independent direction: a fitted Arial set covered 46% of real glyph instances, and this explains
why.

## Attempting to fix it at the binarizer, and failing

The table above makes edge sensitivity look like the obvious thing to attack: at 30 cells it is the
largest term that is not inherent to the source material, and unlike weight and slant it is an
artefact of *our* thresholding. Two ways of attacking it were implemented and measured against six
real titles, using distinct shapes per glyph as the variance proxy — fewer distinct shapes for the
same glyphs means the same character is landing on the same vector more often.

**Palette-adaptive thresholding.** Instead of a fixed luma of 128, split the palette at the widest
gap between its drawn entries, so the threshold sits in the empty space between the outline cluster
and the fill cluster on every stream regardless of how it was authored.

| | Distinct shapes |
| :--- | ---: |
| Fixed threshold | 1,270 |
| Adaptive | 1,319 (**+3.9%**) |

Neutral on four titles, worse on two. The reason is visible once measured: subtitle palettes put
fill near luma 235 and outline near 16, so a fixed 128 is *already* comfortably in the gap. The
adaptive split sometimes picks a different gap — between two anti-aliasing entries — and does worse.

**Hysteresis.** Decide borderline pixels by connectivity instead: ink above the high threshold
outright, ink between the low and high thresholds only when touching ink, so an edge follows the
stroke it belongs to rather than the exact place the threshold falls.

| | Distinct shapes |
| :--- | ---: |
| Hard threshold | 1,270 |
| Hysteresis | 1,290 (**+1.6%**) |

Better on one title, worse on two, unchanged on three.

**Neither shipped.** A knob that measures worse is worse than no knob.

### What the failure says

The variance is not caused by *where* the threshold sits. It is caused by there being a threshold at
all. Two renderings of the same glyph differ in their anti-aliasing ramp, and any binary decision
turns that difference into a different set of pixels — moving the decision point, or making it
context-sensitive, just moves which pixels flip.

That points somewhere specific. `vectorize` already computes per-cell *area coverage* and only then
thresholds each cell at 50%. If the mask handed to it carried grey coverage instead of a binary
decision, the per-cell figure would vary smoothly with the ramp rather than in steps, and the 50%
decision would be correspondingly more stable. That is a change to the feature representation in #7,
not to binarization — and it is the experiment worth running next.

## Carrying grey coverage into the feature vector, and failing differently

The experiment the previous section pointed at. `vectorize` computes per-cell area coverage and then
thresholds each cell at 50%; the input to that had always been a binary mask, so a pixel was one
unit of ink or none. `vectorize_coverage` takes a `CoverageMask` instead — opacity times brightness,
parameter-free, no threshold anywhere on the path — and each source pixel contributes its ink
*fraction*. Connected components still run on the binary mask, because a component is a yes-or-no
thing; only the vector changed.

Measured two ways, both of which had to agree before shipping it.

### Head to head on the stability harness

`xtask measure-stability` now renders each variant once and vectorizes it **both** ways, so the two
columns describe the same sample and any difference belongs to the representation. The anti-aliasing
axis moved into coverage space to make this exact: the ink levels rescale the ramp rather than
moving the threshold, which selects precisely the same pixels — so the binary column is bit-for-bit
the one measured above, and only the grey column is new.

Arial regular, bold and italic, 62 alphanumerics, 90 variants each:

| Distance | Binary | Grey | |
| :--- | ---: | ---: | :--- |
| Intra-character p75 (regular upright) | 51 | 52 | worse by 1 |
| Inter-character p25 (nearest other) | 27 | 24 | **worse by 3** |
| Margin | −24 | −28 | worse by 4 |

Per axis, the picture is sharper — and it is not uniformly bad:

| Axis of variation | Binary p50 / p95 | Grey p50 / p95 |
| :--- | ---: | ---: |
| Rendering size | 11 / 38 | **7 / 32** |
| Anti-aliasing ramp | 8 / 35 | **7 / 30** |
| Outline / edge (1px) | 30 / 73 | 30 / 76 |
| Weight (bold) | 38 / 81 | 37 / 77 |

**Grey coverage does exactly what it was designed to do.** It cuts sensitivity to rendering size by
roughly a third and to the anti-aliasing ramp with it — the two axes that are purely about how a
ramp gets sampled. It does nothing for the ±1px edge term, which is the largest of the four, because
a glyph one pixel fatter is genuinely a different shape rather than a differently-sampled one.

And it costs something the earlier experiments did not: characters move *closer together*. Softening
the representation blurs small shape differences, and small shape differences are exactly what
separates one character from its nearest neighbour.

### End to end on accuracy

`xtask accuracy` now scores both representations over one fixture, each against a reference set
built through its own normalisation — a reference generated the other way would be compared against
a different transform and every distance would be meaningless.

| Representation | CER | WER |
| :--- | ---: | ---: |
| Binary mask | **16.9%** | **62.1%** |
| Grey coverage | 24.2% | 69.0% |

The failure mode names itself:

```
binary: The quiCk brown foxjumps
grey:   The qUiCk brOwn fOxjUmps
```

Every collapse is a case collapse — `o`→`O`, `u`→`U` — between letters that differ only in small
details of shape, since letterboxing has already normalised away the size difference that would
otherwise separate them. That is the inter-character column of the stability table showing up as
misread characters. **The two instruments agree, and they agree about the mechanism, not just the
direction.**

**Not shipped.** `Config::grey_coverage` defaults off and is deliberately not exposed as a CLI flag:
it exists so the harnesses can drive the pipeline both ways, not as something to tune. The
measurement is the deliverable here.

### What this one says

Two experiments have now failed in the same direction, and together they bound the problem.
Sensitivity to *sampling* — threshold placement, ramp shape, rendering size — is real but small, and
grey coverage genuinely reduces it. The dominant term is sensitivity to *shape*: one pixel of extra
weight on a stroke, which no representation choice can argue away because the shape really is
different.

So there is no fixed vector per character that survives the variation, and no way to normalise the
variation away before matching. What is left is the observation that within a single stream the
variation is small — one authoring tool, one typeface, one resolution. **That is #10, and it is why
#10 has to become cluster-then-match rather than match-per-glyph.**

## Clustering the stream's own shapes, and failing for a reason that changes the project

The redesign #10 became after the section above. Group a stream's own shapes, match one consensus
vector per group, and give every glyph in a group that answer. The reasoning: a title is authored
once, so within a stream the expensive axes — weight, slant, typeface — are constant, and only the
cheap ones vary. Clustering should cancel exactly the variation a fixed set cannot survive.

It is implemented in `subtrackt_glyph::cluster`, it does what it says, and `xtask cluster-sweep`
measured it across four conditions and eight radii. **Every cell in the grid is neutral or worse.**

| radius | plain, exact | varied, exact | plain, near miss | varied, near miss |
| :--- | ---: | ---: | ---: | ---: |
| 0 (no clustering) | **16.9%** | **23.9%** | **32.3%** | **28.5%** |
| 2% = 5 cells | 21.0% | 24.7% | 32.3% | 28.8% |
| 4% = 10 cells | 21.0% | 28.2% | 32.3% | 29.5% |
| 6% = 15 cells | 24.2% | 33.0% | 32.3% | 28.0% |
| 8% = 20 cells | 26.6% | 32.7% | 37.9% | 33.0% |
| 12% = 30 cells | 31.5% | 35.4% | 40.3% | 31.7% |
| 16% = 40 cells | 48.4% | 39.7% | 47.6% | 33.7% |

CER, lower is better. *Varied* repeats the cue set at five rendering sizes, which is what a stream
looks like and a fixture does not; *near miss* renders the material in a different typeface from the
reference set, which is the realistic case rather than the ceiling one. The best cell anywhere is
−0.5 points, which is noise.

### Why, in one measurement

The sweep prints the closest pairs in the reference set before it runs, and they settle the question
on their own:

```
I / l   0 cells apart
I / |   0 cells apart
l / |   0 cells apart
I / Í   3 cells apart
! / I   4 cells apart
I / i   4 cells apart
```

**`I`, `l` and `|` are the same vector.** Not close — identical. Letterboxing normalises a vertical
bar to a centred vertical bar whatever its height, and that is every one of them.

So a radius has a ceiling it can never reach. Clustering needs a character's own renderings to be
closer to each other than the nearest *different* character is, and the nearest different character
is at distance zero. The smallest radius tried, five cells, already merges fifteen pairs; at twenty
cells it merges 172. Meanwhile a stream's own variation runs to a median of 11 cells for rendering
size alone. **The two distributions do not merely overlap — the inter-character one starts at zero.**

This also explains the previous experiment. Grey coverage failed by collapsing `o`→`O` and `u`→`U`;
those pairs differ only in size relative to the line, which letterboxing has already discarded, so
softening the representation was pushing on characters that were already nearly coincident.

### What this actually says, which is not "clustering was a bad idea"

Three experiments have now failed, and they were all attacking the same term. Threshold placement,
ramp representation and grouping strategy are all ways of reducing *variance* — of making two
renderings of one character land closer together. Every one of them worked to some degree, and none
of it mattered, because the problem is not variance. It is **separation**: the feature vector places
distinct characters at distance zero, and no amount of variance reduction can recover a distinction
the representation never encoded.

That reframes the ceiling. The pipeline is not a noisy version of something that would work if it
were quieter. It is missing a feature.

And the missing feature is identifiable from the same evidence. Every confusable pair here —
`I`/`l`/`|`/`!`/`1`, `o`/`O`, `c`/`C`, `u`/`U`, `s`/`S` — is a pair whose members have the *same
shape* and differ in **size and position relative to the text line**. An `o` is an `O` that occupies
the x-height band instead of the cap-height band. `AspectPolicy::Letterbox` normalises exactly that
away, deliberately, and it was the right call for scale invariance across resolutions — but it
throws out the only thing separating these pairs.

`subtrackt_glyph::group` already computes line bands. A glyph's height and baseline offset *as
fractions of its line band* are scale-invariant in the way that matters, survive a resolution
change, and separate every pair in the list above. That is the next experiment, it is a #7 question
again, and unlike the last three it attacks the term the measurements actually implicate.

Clustering ships off — `ClusterRules::default` sets a zero radius, so every distinct shape keeps its
own label decision and behaviour is exactly what it was. The machinery stays because it is the
instrument that produced this finding, and because the next experiment changes the feature vector
and will want the sweep re-run against it.

## Measuring against the text line, and the first thing that works

#37, and the first change aimed at **separation** rather than variance. Every confusable pair —
`I`/`l`/`|`/`!`/`1`, `o`/`O`, `c`/`C`, `u`/`U`, `s`/`S` — has the same *shape* and differs in how
tall it stands in its line. `AspectPolicy::Letterbox` normalises exactly that away.

So each glyph gained two figures, both percentages of its line's cap height rather than pixel
counts: its **height**, and its **baseline offset** — how far its bottom sits below the baseline,
signed, so a hyphen floating clear of the baseline does not read as an underscore sitting on it.

### Finding the anchors

Neither anchor is in the image. Both are estimated from the line's own glyphs, and both are **modes
rather than extremes**, because an extreme is decided by one glyph: a single comma drags a minimum
bottom down, a single `É` pushes a maximum top up, and every measurement on the line shifts with it.
The cap line is the highest row *enough* glyphs reach, not the highest row anything reaches.

Two cases report nothing rather than a number:

- **Too few glyphs.** Four is the floor; below that there is no mode, only a guess.
- **No height variety.** A line of one height cannot say which height it is — `NO ONE SAW` and
  `no one saw` present identically. Measuring either would make every glyph on the line read as a
  capital, so the line falls back to shape alone.

An unknown metric contributes *nothing* to a distance rather than a default. A glyph on an
unmeasurable line is compared on shape, which is worse than the full comparison and much better than
being scored against a fabricated height.

### The check that came first

The idea was cheap to falsify and expensive to build, so `xtask separability` ran first, on font
metrics alone. The prediction was put in #37 before the measurement, and it held:

```
o / O   shape  25 ->  39   (heights  76 / 104)   should separate
c / C   shape  19 ->  33   (heights  76 / 104)   should separate
u / U   shape  15 ->  28   (heights  75 / 102)   should separate
I / l   shape   0 ->   0   (heights 100 / 100)   should NOT separate
```

`I` against `l` stays at zero, and that is not a defect to fix. Arial's lowercase ascender and its
cap height differ by under 2%: a capital I and a lowercase l are the same mark at the same height.
Humans read them by context, which is #12's job, not this one's.

One correction to the prediction's arithmetic: `o` measures 76% of cap height, not the 52% #37
guessed — that figure confused x-height over *em* with x-height over *cap height*. The separation
held; the magnitude was wrong.

### What it is worth

`xtask metric-sweep` scores the weight across the same four conditions as #10:

| weight | `o` vs `O` | plain, exact | varied, exact | plain, near miss | varied, near miss |
| :--- | ---: | ---: | ---: | ---: | ---: |
| 0 (shape only) | 0 | 16.9% | 23.9% | 32.3% | 28.5% |
| 98 | 7 | 16.1% | 19.4% | 29.8% | 24.7% |
| **196** | **14** | **16.1%** | **17.0%** | **24.2%** | **22.8%** |
| 293 | 21 | 16.1% | 17.0% | 25.0% | 23.1% |
| 586 | 42 | 16.1% | 18.3% | 25.0% | 26.0% |
| 977 | 70 | 16.9% | 22.6% | 31.5% | 31.1% |

CER, lower is better. The weight is in tenths of a percent of the vector per full cap height, and
the second column is what that makes the gap between an `o` and an `O` — 28 points of cap height —
worth in cells. #45 moved the unit after the fact: these rows were measured as 0, 25, 50, 75, 150
and 250 hundredths of a cell, and at 16×16 the two spellings name the same cell counts exactly.

**196 — fourteen cells between an `o` and an `O` — is best or tied-best in all four**, which is a
clean choice rather than a fitted one, and it is what ships. Fourteen cells is twice the ambiguity
margin, comfortably inside the 51-cell match ceiling.

The gains land where they should. The ceiling case improves by 0.8 points because it was never
failing on this; the realistic cases — varied rendering, mismatched typeface — improve by **5.8 to
8.1 points**. Ambiguous glyphs fall with them, 40 to 24 on the plain fixture.

Two other results worth recording. Zero-distance pairs in the reference set drop from **three to
one**, the survivor being `I`/`l` as predicted. And grey coverage, which #35 measured 7.3 points
worse, now scores *identically* to the binary mask — its failure was collapsing `o` onto `O`, and
that failure is what this fixes. The diagnosis in #35 was right.

Clustering was re-swept against the new vector and still does not help: the best cell is −1.0 and
everything else is worse, so #10's conclusion stands unchanged.

### What is left

The remaining fixture errors are two classes, and neither is a shape problem:

```
! The quick brown foxjumps      want: The quick brown fox jumps
! over the Iazy dog.            want: over the lazy dog.
```

Word spacing (#11) and `l`/`I` (#12). The first is a gap threshold; the second needs context, which
the matcher made available by flagging the glyph as ambiguous rather than answering silently. #12
has since taken that up and cleared most of the second class, though not this line: see
`docs/post-correction.md` for why `Iazy` is the case it deliberately declines.

## Counting holes, and a feature that works but has nothing left to do

#37 left a question standing. If the shape vector cannot separate certain pairs, what *else* could
be measured that it does not encode? Hole count — how many enclosed background regions a glyph
carries — is the obvious candidate. It is topological rather than geometric, so it is exact at any
resolution, orthogonal to the shape vector, and immune to the ±1px edge term the table above
measures as the dominant source of variance. `O` carries one hole, `C` none, `B` two, and no amount
of rasterisation noise changes that.

`xtask separability` gained three sections to check it, on font renders alone, before anything was
built. Two had to pass, and the wrong one failed.

### It is a good feature

| Test | Result | |
| :--- | ---: | :--- |
| Stability across 21–50px × 3 ink thresholds | 99% agreement with the modal count | passes |
| Portability across Times, Verdana, Tahoma, Segoe UI | 1 of 139 characters disagrees | passes |
| **Separation of the pairs the matcher calls ambiguous** | **0 of 21** | **fails** |

The risk that looked fatal beforehand — that a counter closes at the 21px heights the library survey
measured, making the count a confident lie — is essentially absent. One character wavers, `Ô`, in
one rendering out of eighteen. Portability is better still: the only cross-typeface disagreement is
Times' double-storey `g` reporting two counters against Arial's one, which is a real letterform
difference rather than a measurement artefact.

The census validates the instrument itself: `$%&8B` at two holes, `#0469@ADOPQRabdegopq` and the
accented round capitals at one, everything else at none. Background is walked **4-connected** as the
dual of `ccl`'s 8-connected foreground — two strokes touching at a corner are joined by the
foreground pass, and an 8-connected background walk would leak a counter out through that same
corner, under-counting precisely the tight counters the measurement exists to interrogate.

### It separates nothing that is actually confused

Not one of the 21 ambiguous pairs has differing hole counts. Widening the band past the shipped
margin does not rescue it — still zero out to 15 cells, and the pairs that do differ appear only
past 24, where nothing was confused to begin with.

The reason is that the confusion set this idea was aimed at no longer exists. `c`/`e`, `O`/`Q`,
`B`/`R` — the pairs holes would fix — are not ambiguous. `c` sits 33 cells from `C`, nearly five
times the margin. The shape vector separates them comfortably, and #37 cleared the rest.

What is left is two families, and holes are constant within both *by construction*:

- **Vertical bars** — `I`/`l`, `!`/`I`, `!`/`l`, `I`/`i`, `i`/`l`. Zero holes, all of them.
- **Accent direction** — `Ò`/`Ó`, `À`/`Á`, `È`/`É`, `Ó`/`Ô`, `à`/`á`, `ò`/`ó`, `ù`/`ú`. The same
  base letter, therefore the same count necessarily.

Sixteen of the twenty-one are the second kind, and that is the finding worth carrying forward. The
dominant residual confusion is **not** `I`/`l`, which is one pair. It is **diacritic direction** — a
mark occupying the top sixth of the glyph, whose *slope* is the entire distinction. Accented
lowercase is ordinary text in Spanish, French and Italian subtitles, so this is not an edge case.

**Not built.** The harness stays: it is the instrument that produced the finding, and it is what the
next proposal should have to survive.

## Doubling the grid, and two instruments that disagree

`docs/architecture.md` has carried 16×16 versus 32×32 as an open decision from the start, deferred
until #9 landed. It has landed, and the accent-direction finding above is exactly the shape of
problem more cells ought to fix: a fine spatial detail in a small region, sub-cell at 16 and
resolvable at 32.

The answer is **not proven**, and how the measurement says so matters more than the verdict.

### One instrument says yes

`xtask metric-sweep`, each grid scored at its own best weight — #45 is why it had to move at all,
and has since been fixed. Both settings are quoted below in the hundredths-of-a-cell unit that was
current when they were measured:

| condition | 16×16 (w=50) | 32×32 (w=250) | |
| :--- | ---: | ---: | :--- |
| plain, reference typeface exact | 15.9% | 15.9% | tie |
| varied, reference typeface exact | 16.7% | 16.1% | −0.6 |
| plain, reference typeface a near miss | 25.6% | **22.0%** | −3.6 |
| varied, reference typeface a near miss | 24.2% | **19.5%** | −4.7 |

That is the #37 signature exactly: ceiling unchanged, realistic conditions improving by several
points. On this table alone it would ship.

### The other says no

`xtask reference-fit` scores four near-miss typefaces instead of one. Both rows measured on the same
tree, and the 16×16 row reproduces `docs/reference-set.md` exactly:

| reference set | 16×16 | 32×32 | |
| :--- | ---: | ---: | :--- |
| arial (ceiling) | 15.9% | 15.9% | tie |
| verdana | 27.4% | 29.3% | +1.9 worse |
| tahoma | 29.3% | 36.0% | +6.7 worse |
| trebuc | 36.0% | 34.8% | −1.2 better |
| segoeui | 37.8% | **26.2%** | −11.6 better |
| **mean of the four** | **32.6%** | **31.6%** | −1.0 |

Two better, two worse, per-typeface deltas from −11.6 to +6.7, and a one-point mean against that
spread is noise. **Coverage falls on every candidate** — 92.4→87.8, 84.0→77.1, 87.8→84.7,
93.9→87.0 — which is the predicted cost showing up: a finer grid places a mismatched typeface
further away, so fewer glyphs land inside the ceiling at all.

### Why they disagree, which is the useful part

The two harnesses mismatch typefaces in opposite directions. `metric-sweep` renders material in
Verdana and reads it with an Arial reference; `reference-fit` renders material in Arial and reads it
with a Verdana one. Same pair, reversed, opposite answer. **A gain that depends on which side of the
mismatch you are standing on is not a gain.**

The summary statistic agrees with the pessimistic reading. Intra-character p75 against
inter-character p25, as a fraction of the vector, is −9.4% at 16×16 and −10.0% at 32×32: the
separation the shape vector achieves does not improve with resolution, it very slightly worsens.
Zero-distance pairs do fall from three to one, `I`/`|` and `l`/`|` finally separating, but the
accent pairs that dominate the residual confusions stay proportionally as close — `Ò`/`Ó` moves from
2.0% of the vector to 2.1%.

### What it says

Four experiments have now attacked **variance** — threshold placement, ramp representation,
clustering, and now resolution — and none has paid. One attacked **separation**, #37, and it paid
immediately. Granularity looked like a separation lever because the accent pairs are a spatial-detail
problem; measured, it behaves like every other variance lever, because a finer grid records the
letterform's noise as faithfully as its signal.

**Not shipped.** `FEATURE_GRID` stays at 16. What the attempt did buy is that the constant can now
actually be changed — the hardcodes are gone and #45 below is fixed — so the next person to ask is a
one-line experiment away rather than an afternoon of hardcode archaeology.

### The exchange rate that was not a fraction

`metric_weight` was documented in hundredths of a cell: an absolute count, on the one axis
`MatchThresholds` promises in as many words is scale-free. A full cap height was therefore worth 50
cells at every grid size — 19.6% of a 256-bit vector and 4.9% of a 1024-bit one — which is why the
32×32 attempt above had to refit it before the grid could be judged at all, and why anyone flipping
the constant without noticing would have measured the refit's absence and blamed the grid.

It is now tenths of a percent of `FEATURE_BITS` per cap height. 196 is the same 50 cells at 16×16
and 200 at 32×32, and the promise the type makes is now enforced by a test that evaluates the
conversion at several grid sizes rather than only the one the build uses.

**A re-expression, not a retune**, and the sweep says so: every row of `xtask metric-sweep` at 16×16
is identical before and after, setting for setting, cell count for cell count and CER for CER.

| `o` vs `O` | plain, exact | varied, exact | plain, near miss | varied, near miss |
| ---: | ---: | ---: | ---: | ---: |
| 0 cells | 11.6% | 19.8% | 31.7% | 26.5% |
| 7 | 11.0% | 14.4% | 29.9% | 22.9% |
| **14 (shipped)** | **11.0%** | **11.4%** | **22.6%** | **19.8%** |
| 21 | 11.0% | 11.4% | 21.3% | 19.3% |
| 42 | 11.0% | 12.6% | 21.3% | 21.5% |
| 70 | 11.6% | 16.6% | 28.7% | 26.2% |

Indexed by cell count rather than by setting, because the cell count is the thing both spellings
agree on. The figures sit well below the #37 table further up because the tree has moved since —
#12 and #40 both landed.

And what the fix is worth, which is the point of it. `FEATURE_GRID` flipped to 32 on this tree, with
the term priced both ways:

| an `o` against an `O` at 32×32 | plain, exact | varied, exact | plain, near miss | varied, near miss |
| :--- | ---: | ---: | ---: | ---: |
| 14 cells — what the cell count gave | 22.0% | 18.1% | 32.9% | 22.6% |
| **56 cells — what the fraction gives** | **12.2%** | **11.0%** | **20.1%** | **15.9%** |
| | **−9.8** | **−7.1** | **−12.8** | **−6.7** |

Up to 12.8 points, recovered by a constant that follows the grid rather than by someone remembering
to follow it — and #46, measuring the same effect from the other side, put it at 10.9. The absolute
figures differ from the 32×32 column further up because that column predates #12 and #40; both rows
here come from the same tree, which is what makes them a comparison.

One loose end, recorded and not acted on. At **both** grid sizes the sweep row above the shipped one
scores a shade better — 21 cells against 14 at 16×16, 84 against 56 at 32×32 — by 1.3 and 0.5 points
on the near-miss conditions here and 1.2 and 0.5 there, and no worse anywhere. That the argmin sits
at the same *fraction* of the vector at both sizes is the new unit's own argument for itself. It is
also still a retune: the fixture has moved under the number #37 chose, and re-choosing it belongs in
its own issue rather than smuggled into a change whose entire claim is that it changes nothing.

## The accent's own direction, and a mark that has to reach its body first

#48, measured before anything was built, the way #37 and #46 were. #46 went looking for a hole count
and turned up a different fact on the way past: of the 21 pairs the shipped matcher calls ambiguous,
**sixteen are one base letter differing only in which way its accent leans** — `À`/`Á`, `È`/`Ê`,
`è`/`é`, `ò`/`ó`. Five are the vertical bars, which are #12's territory. So `l`/`I` is one pair and
accent direction is sixteen, and accented lowercase is ordinary Spanish, French and Italian text.

The shape vector cannot see the difference for a structural reason. Letterboxing scales the merged
bounding box — base plus mark — to fill the grid, so a mark occupying the top sixth of the glyph
lands in one or two rows of cells and its direction is largely sub-cell, while everything below it
is identical between the pair. But the pipeline *knows* which pixels are the accent: `group`
identifies the component as a mark and attaches it to a body before `feature` merges the boxes and
throws that away. Three candidates, cheapest first, and the prediction for each was
[recorded on the issue][48-prediction] before the bench was run.

[48-prediction]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/48#issuecomment-5377570156

| | Candidate | What it stores |
| :--- | :--- | :--- |
| **A** | placement | The mark's box as four fractions of the body's |
| **B** | shape | The mark's own ink, letterboxed onto the same grid by the same `vectorize` |
| **C** | slope | `Cxy / sqrt(Cxx · Cyy)` over the mark's ink, as a percentage. One signed number |

### The question nobody had asked, which came first

A mark only becomes a mark if `group` attaches it to a body, and `group` works inside one text line.
`line_bands` cuts a line at any row carrying no ink **across its whole width** — and the accent on a
*capital* sits above every letterform the charset can spell. So the row between `À`'s grave and its
`A` is blank for the width of the line, and the accent bands as a line of its own.

Rendering each subject between the tallest available neighbour, at the most generous of the three
ink thresholds — more generous than the runtime's, which is fill-only and excludes the outline:

| Neighbour | Reaches above baseline | Marks that reach their body |
| :--- | ---: | :--- |
| `$`, the tallest ASCII character | 76 px | 44 of 51 |
| `f`, the tallest **letter** | 70 px | 25 of 51 |
| a capital `H`, for scale | 69 px | — |

Arial draws its ascenders to cap height, so in a line of nothing but letters and spaces there is
nothing above the capitals at all, and **every accented capital fails to group**. `$` and the
brackets overshoot on purpose and rescue it; ordinary subtitle text does not contain them. The seven
that fail even in the best case are `"` and `%` and `:`, which `group` documents, and `Î Ï î ï`,
where the mark is wider than the narrow `i` beneath it and the 50% overlap rule rejects it.

This is a segmentation fact rather than a matching one, and it lands *before* anything below: a mark
that does not reach its body is not a mark. The letter under it is matched bare and the accent is
matched as a glyph in its own right, which is a different failure from the one #48 set out to
measure. It is #6's territory rather than #48's, and is tracked as
[#57](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/57) — with the narrow `i`
case, which fails for an unrelated reason, as
[#58](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/58).

### Separation, measured against each character's own noise

Not against a fixed threshold. #14 is the reason: two renderings of one character sit further apart
than two different characters do, so a gap only counts when it is wider than the wobble either
character shows on its own across the survey range. All sixteen pairs, best-case neighbour:

| Candidate | Separates | Typical gap | Typical noise | Ratio |
| :--- | ---: | ---: | ---: | ---: |
| A placement | 11 of 16 | 7–28 | 7–14 | ~1–3× |
| B mark shape | 16 of 16 | 88 cells, 34% of the vector | 56 cells, 21% | **1.6×** |
| C mark slope | 16 of 16 | 64–135 | 6 | **10–20×** |

Both B and C separate everything. The distance between them is the ratio. B clears its own noise
floor by a factor of 1.6, which is the margin #14 says is not enough to rely on; C clears it by ten
to twenty, in one signed byte rather than a second 32-byte vector.

The sign is what does the work, and it does it across three marks rather than two: an acute reads
about −65, a grave about +67, and a circumflex sits at 0 — *between* them, because it is symmetric
about its vertical axis and its cross term cancels. Every mark that leans at all holds its sign in
**100%** of renderings across six sizes and three ink thresholds. Nothing reverses direction between
Arial and Verdana or Arial and Tahoma; the largest move is 10 points on a ±100 scale.

### The predictions, scored

Three of five, with both misses in the informative direction.

- **B separates, mean gap at least 60 cells.** Right: 88, and 16 of 16.
- **A separates fewer than 4 of 16.** Wrong: 11. Placement carries more than expected, because a
  circumflex is a visibly wider box than an acute — it is the acute-against-grave pairs, whose boxes
  really are near-identical, that A misses.
- **C separates on sign, at least 14 of 16.** Right: 16, and the circumflex-sits-between mechanism
  held.
- **B fails stability — same-mark spread over 60 cells, swamping the gap.** Wrong: 56, which stays
  under the 88 gap. B holds, but only just, and "only just" is what decides between B and C.
- **C wins over B.** Right, on the ratio rather than on B failing outright.

### What it says

Carry the mark's slope, not the mark's vector. One signed number, priced against the shape vector
the way #45 established every term must be, separates all sixteen pairs at a signal-to-noise ratio
an order of magnitude better than the obvious candidate — and the obvious candidate costs 32 bytes a
reference entry to do the same job worse.

The measurement is a ceiling, in the same sense `xtask accuracy` is: it is font renders, and the
reference typeface is the material's own. What it establishes is the necessary condition. Whether it
pays is a CER question, and answering it needs a fixture carrying accented text in more than the one
line it has today.

## Building it, and finding the sixteen pairs were never the ones going wrong

The necessary condition held, so the mark's slope was built: one signed byte per reference entry, a
version 3 set, a term in the matcher and the same term in the clustering rules. The fixture gained
three cues carrying both members of `à`/`á`, `è`/`é`, `ò`/`ó` and `ù`/`ú`, and `xtask mark-sweep`
swept the weight across eight conditions — two rendering sizes, two typeface conditions, plain and
varied.

**CER does not move at any setting on any condition.** The best row anywhere is −0.1, which is one
cell. Above 78 permille it gets worse: +0.8 to +2.1, because the term starts rejecting correct
characters outright rather than demoting wrong ones.

| Weight | Acute vs grave, in cells | Ambiguous glyphs | CER |
| ---: | ---: | ---: | ---: |
| 0 | 0 | 350 | 20.3% |
| 20 | 6 | 326 | 20.3% |
| 39 | 11 | 328 | 20.3% |
| 78 | 24 | 325 | 20.3% |
| 156 | 51 | 327 | +0.9 |
| 293 | 98 | 335 | +1.2 |

*(42px, five renderings, reference typeface a near miss — the most realistic of the eight. The other
seven have the same shape.)*

The ambiguity tally does fall, consistently, by 12 to 20% across every condition. That is the term
working exactly as designed: it pushes the wrong-leaning candidate clear of the 3% margin. But it
falls on glyphs that were **already being read correctly**, so nothing downstream changes.

### The census that explains it

CER cannot see this. A wrong accent is one character in a line of thirty, so flipping every one of
them moves the number by less than the gap between two conditions. So `xtask mark-sweep` counts the
accented characters directly — and the reference set's own slopes alongside them, so a null result
cannot be a disarmed term misread as an absent effect.

```
  slopes   68 -67  65 -66  66 -65  66 -66   (the reference set's own, so the term is armed)
  weight    à   á   è   é   ò   ó   ù   ú
   truth    3   3   1   2   1   1   1   1
       0    3   0   1   5   1   1   1   1
     293    3   0   1   5   1   1   1   1
```

Every `á` is being read as `é`. Not one of them is being read as `à`.

**The accents this pipeline gets wrong are wrong in the base letter, not in the direction of the
mark.** `á` for `é`, `è` for `ò` — pairs that carry the *same* accent and differ in the letter
underneath. The slope term cannot separate those by construction, because both members report the
same slope; it is a term that fires only on the axis where nothing was failing. Turn it up to where
it would overrule shape and it destroys correct reads: `á` 3 → 0, `ù`/`ú` 1/1 → 0/0.

### What it says

**Shipped off.** `mark_weight_permille` defaults to 0, next to a comment recording why, the way
`ClusterRules::radius_percent` does. The implementation stays, because it is the instrument that
answered the question and because turning it on is one number if the conditions ever change — #43
is the change that would do it, since a reference set fitted to the title is what removes the
base-letter confusions that dominate here.

The lesson is about the instrument rather than the feature. `xtask separability` measures the
reference set **against itself**: which pairs sit within the ambiguity margin. #48 read that as
"accent direction is the dominant residual confusion", and it is not — it is the dominant residual
*ambiguity*. Those are different claims, and only one of them is about characters coming out wrong.
Sixteen pairs sitting close together in the reference set tells you nothing about whether any glyph
ever landed on the wrong one of them. Nothing before this had made the distinction matter, because
#37's pairs — `o`/`O`, `c`/`C` — were both ambiguous and wrong.

That is the fourth feature measured before building and the second built before being disproved. The
bench is cheap; running the sweep afterwards is what caught this one.

## A mark wider than the letter under it, and the rule that could not say so

#58, found while building #48's bench and fixed with one line and one guard.

`GroupingRules::min_overlap_percent` was documented as *"minimum horizontal overlap with the base,
in percent of the diacritic's width"*. The denominator is the mark, so **a mark wider than the letter
under it can never reach 50%, however perfectly it is centred.** That is the `i`/`I` family, where
the body is a bare stem. Component boxes at 96px in Arial:

| character | mark box | body box | overlap | as % of the mark | grouped |
| :--- | ---: | ---: | ---: | ---: | :--- |
| `î` | w 28 | w 9 | 9 | **32%** | ✗ |
| `Î` | w 29 | w 9 | 9 | **31%** | ✗ |
| `ï` | w 9, w 9 | w 9 | **0** | **0%** | ✗ |
| `ì` | w 17 | w 9 | 9 | 53% | ✓ barely |
| `é` | w 17 | w 45 | 17 | 100% | ✓ |

The question the rule exists to ask is whether a mark sits over *this* letter rather than a
different one, and for a body narrower than the mark that cannot be expressed as a fraction of the
mark. Denominating by **the narrower of the two boxes** asks it properly: `î` becomes 9/9.

### The guard the change needed

Denominating by the narrower has a failure of its own, and the prediction on the issue named it
before the code did: a wide mark *straddling* two narrow letters covers half of each, so it would
now claim whichever one `best_body` reached first — trading a missed attachment for a wrong one,
which is worse.

So the mark's horizontal centre must also fall inside the letter. A mark the typeface placed over a
letter is centred on it; a mark between two is centred between them. It costs nothing measurable —
the census is 46 of 51 with or without it — and it closes the hole.

### What it moves

| | before | after |
| :--- | ---: | ---: |
| Marks reaching their body, best neighbour | 44 of 51 | **46 of 51** |
| Marks reaching their body, letters-only line | 25 of 51 | **26 of 51** |
| Accuracy fixture, CER | 11.1% | 11.1% |

`Î` and `î` group; `Ï` and `ï` still do not, and that was predicted. A diaeresis is two components
straddling the stem — at x84 and x102 over a stem at x93 — so **neither dot overlaps it at all** and
neither centre falls on it. Only their union does, and `best_body` tests each mark independently.
Fixing that means grouping adjacent marks before matching them to bodies, which is a larger change
with a hazard of its own: the dots of `ii` are also two marks at the same height, and merging those
would attach one mark to one stem. #58 stays open for it.

The fixture gained `S'il vous plaît, maître.` so the gain is visible in `xtask accuracy` rather than
only in the census. Both `î` come out right; the line's remaining errors are `l` read as `I`, which
is #12's, and post-correction now fixes one of them.

## What follows

- ~~**#10 needs redesigning, not implementing.**~~ Redesigned as cluster-then-match, implemented,
  and **measured worse at every radius** — see above. The premise was that within-stream variation
  is small enough to group safely; the measurement found the nearest different character is at
  distance *zero*, so no radius exists. Clustering ships off.
- ~~**The next experiment is a line-relative size feature**~~ — **done, and it works**: #37, see
  above. 5.8 to 8.1 points of CER on realistic conditions, and zero-distance pairs down from three
  to one. The first change to aim at separation rather than variance, and the first to pay.
- **What remains is not shape.** Word spacing is #40 — #11 landed the rule before anything could
  score it, and scoring found it finds 21 of 29 spaces — and `l`/`I` is #12. ~~#12 now receives those
  glyphs flagged as ambiguous rather than answered silently.~~ **Built and measured** — see
  `docs/post-correction.md`. Reading `l`/`I` from the characters either side of it takes 3.1 points
  off the CER above and makes no line worse. `jaIapeño` is fixed; `Iazy` is not, because the
  evidence for correcting a word-initial capital is the same evidence that would break `Iowa`.
- ~~**#9 cannot embed its way to a solution.** The fixed set identifies the typeface and seeds
  labels; it will not carry the load alone.~~ **Stronger than that, measured.** A fixed set should
  not be embedded at all: a near-identical typeface costs 11 points of CER, which is a
  visibly-different one's cost to within noise, and nothing the accuracy gate can see detects it.
  The set has to be fitted to the title — #43. See `docs/reference-set.md`.
- **Hole count is a good feature with nothing to do** — measured before building, see above. It is
  stable (99%) and portable (1 character in 139), and it separates *none* of the 21 pairs the
  matcher calls ambiguous, because sixteen of those are accent-direction pairs that carry the same
  count by construction. Not built; the harness stays.
- ~~**Those sixteen pairs have an answer, and it is one byte**~~ — **built, swept, and shipped
  off.** The mark's slope separates all sixteen at ten to twenty times its own noise and takes 12 to
  20% of glyphs out of the ambiguous bucket, and it moves CER on none of eight conditions. The
  accents that come out wrong are wrong in the *base letter* — `á` read as `é` — which carries the
  same mark and is invisible to the term by construction. `mark_weight_permille` defaults to 0 and
  the machinery stays. Worth re-sweeping after #43, which is what removes the base-letter
  confusions.
- **"Ambiguous" and "wrong" are not the same set, and `xtask separability` only sees the first.** It
  measures the reference set against itself. Every conclusion it produces is a claim about which
  pairs sit close together, not about which glyphs land on the wrong one — and #48 is the first time
  those came apart. Any future feature justified by that bench needs a sweep before it is believed.
- **A mark on a capital never reaches its body in ordinary text** — found while setting the
  measurement above up, and it is #57 rather than #48. `line_bands` cuts a line at any blank
  row and the accent on a capital sits above every letter Arial draws, so `À` segments as an `A` and
  a floating grave. 25 of 51 marks group in a letters-only line against 44 of 51 with a `$` on it.
  The accuracy fixture has never caught this because its accented text is all lowercase.
- **A 32×32 grid is not proven** — see above. One instrument makes it 3.6 to 4.7 points better on
  near-miss typefaces, another makes it a one-point wash across four of them with coverage down on
  every one, and the shape vector's own separation statistic slightly worsens. `FEATURE_GRID` stays
  at 16. The attempt exposed #45, which is fixed — the exchange rate between shape and line metrics
  is a fraction of the vector now, so the next grid change no longer un-tunes the matcher on its
  way past.
- ~~**Reducing edge sensitivity in binarization**~~ — **tried and failed**, see above. Two
  approaches measured neutral-to-worse. The lever is the binary mask itself, not the threshold
  placement, which makes it a #7 question about carrying grey coverage into the feature vector.
