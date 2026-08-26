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

**The slant row has since moved.** #122 sampled a leaning glyph along its own line's slant instead
of along the pixel grid, which takes that axis from **47 cells to 26** — below the median distance
to an entirely different character. What the residual 26 is made of, and why it is letterform rather
than a badly estimated angle, is in [`italic-slant.md`](italic-slant.md). The rest of this table is
as measured.

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

> **[#234] counted the palettes, and that last paragraph is wrong about this material.** Fill is at
> luma **192** on four of seven PGS tracks rather than 235, and 128 is **not** in the gap: the
> widest empty band in opaque ink is 33..87, forty-one points below the shipped cut, which sits
> inside the drawn ramp. On three other PGS discs the widest band is four to seven points wide —
> there is no gap at all. Where the sentence is exactly right is VOBSUB, which draws **three**
> colours with a 131-point chasm at 16..147.
>
> The refusal stands on its own number — 3.9% more distinct shapes — and only its explanation was
> invented. `docs/palette.md` has the survey, and a future proposal must not lean on the sentence
> above.

[#234]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/234

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

### Re-measured after #37, on the material it never saw

[#235][issue-235]. Two things had changed since the paragraph above was written. The cost it names
is a **case collapse**, and #37's line-metric term — the thing that decides case — landed
afterwards. And `scripts/bench/run.py` did not exist, so the representation was refused without
ever being put to a disc.

`xtask grey-bench` is that third instrument: it runs the pipeline both ways, each against a
reference set generated through its own normalisation, and scores both against a release. Passing
one set twice is an error rather than a control — a set built under one transform and a disc read
under the other are compared under different geometries and every distance is meaningless, which is
the trap the original measurement records having to avoid.

**The case collapse is gone.** `xtask accuracy` on the ceiling fixture:

| Representation | CER | WER |
| :--- | ---: | ---: |
| Binary mask | **0.0%** | **0.0%** |
| Grey coverage | **0.0%** | **0.0%** |

Where it read 16.9% and 24.2%. `The quiCk brown foxjumps` against `The qUiCk brOwn fOxjUmps` is not
what happens any more, because the term that separates `o` from `O` is no longer the shape vector.
The reason this feature was refused has expired.

**And it is worse than ever on a disc.**

| track | binary | grey | better | worse |
| :--- | ---: | ---: | ---: | ---: |
| The Karate Kid (VOBSUB) | 1.7% | **4.6%** | 62 | **768** |
| Training Day (VOBSUB) | 1.9% | 2.3% | 46 | 167 |
| Gone Girl | 1.3% | 1.5% | 4 | 63 |
| 10 Cloverfield Lane | 0.3% | 0.5% | 0 | 20 |
| A Fish Called Wanda | 1.3% | 1.3% | 19 | 8 |

**768 cues worse on one disc**, and **both** of the worst two are VOBSUB — the codec this was
predicted to help, because grey coverage's measured benefit is a third off sensitivity to rendering
size and the two VOBSUB tracks fit worst on the bench. Training Day's unread count more than
doubles, 103 to 247, which says the softened vectors are falling outside the distance threshold
rather than merely landing on the wrong entry.

The leading explanation is a palette rather than a ramp. Grey coverage reads opacity times
brightness, and a VOBSUB subpicture is authored with **four** palette entries: the smooth
anti-aliasing gradient the feature exists to read is quantised to a level or two before the
binarizer ever sees it. Softening a shape by a coarse step adds no information and blurs the small
differences that separate one character from its neighbour, which is the inter-character column
below moving in the direction it always moved.

That explanation is a hypothesis, and the instrument that would settle it does not exist:
**nothing in this repository can print what a disc's palette holds.** That is [#234][issue-234], and
this is the second finding to want it.

The harness numbers reproduce exactly, which is what makes the two runs comparable at all:

| Distance | Binary | Grey | |
| :--- | ---: | ---: | :--- |
| Intra-character p75 (regular upright) | 51 | 52 | worse by 1 |
| Inter-character p25 (nearest other) | 27 | 24 | **worse by 3** |
| Margin | −24 | −28 | worse by 4 |

Identical to the table above, measured years of issues apart.

**Still not shipped, and now for a reason that will not expire.** The first refusal rested on a
fixture behaviour that a later feature removed; this one rests on 768 cues of real material and on
the axis that has never moved — characters get closer together, and no term added since #37 changes
that.

[issue-234]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/234
[issue-235]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/235

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

## A band of nothing but accents is not a text line

#57, and the last of the three grouping faults #48's bench turned up.

`line_bands` cuts a line at any row carrying no ink **across its whole width**, which is the right
rule for a line break and the wrong one for an accent. The accent on a *capital* sits above every
letterform this charset can spell, so in a line of nothing but letters the row beneath it is blank
and it bands on its own. `À` then segmented as a bare `A` plus a floating grave — a correctly
spelled wrong word with no counter saying so.

### The rule

The issue offered a blank-row tolerance or attaching across bands. Both reason about the *gap*, and
the gap is not what tells the two cases apart. What does:

> **A band holding nothing but mark-height components is not a line.**

A text line always carries at least one full-height letterform; a row of accents never does. Being a
test on the band's *contents* rather than its distance is what makes it safe — it cannot merge two
lines of text however tightly they are set.

The gap survives as a *second* condition, for one case the contents test cannot see: a cue whose
opening line is genuinely all marks, an ellipsis or a row of dashes, is mark-only too. Line leading
is wider than the space above an accent, so `max_gap_percent` holds them apart. Both thresholds are
the ones grouping already uses, applied at band scale.

### What it moves

| | before | after |
| :--- | ---: | ---: |
| Marks reaching their body, letters-only line | 26 of 51 | **46 of 51** |
| Marks reaching their body, best neighbour | 46 of 51 | 46 of 51 |
| Accuracy fixture with an accented capital, CER | **13.1%** | **11.0%** |

The letters-only row now equals the best-case row exactly, which is the point: the `$` neighbour was
only ever a way of filling the blank row by hand, and nothing needs filling any more.

The CER figures are the same fixture measured both ways, with the banding change as the only
variable. The fixture gained `ÉTAIT-CE ÀPRE? ÎLE.` to carry the case — #48 kept accented capitals
out precisely because the gap would have been scored as a matching error, and that reason has gone.

### What still did not read, and why the first explanation was wrong

`ÎLE` came out `ILE` at the fixture's 42px, and this document first blamed #58's narrow-stem case
failing at a smaller size. **That was wrong**, and extending the census to the sizes real material
ships at is what showed it: `Î` reaches its body at every size from 21px to 50px. The glyph is
assembled correctly — `subtrackt glyphs` shows it as one group, `w12 h38`, circumflex included.

Ranking the reference set by shape distance against that exact glyph settles it:

| distance | character | reference height |
| ---: | :--- | ---: |
| **0** | `Î` | 126% |
| 6 | `I`, `i`, `l`, `\|` | 100% |

The shape says `Î` exactly. What flipped it to `I` was the *line-metric* term, and that is #75 —
a separate defect this merge made reachable. See below.

`Ï` and `ï` remain open in #58 for the diaeresis straddle.

Line metrics move for accented capitals, and correctly: once the accent is part of the glyph box an
`À` measures taller than an `A`, which is what `LineMetrics` documents ("more than 100 for a round
capital's overshoot **or an accented one**"). Nothing regressed — the fixture without the new line
reads 11.1% before and after.

## A diaeresis straddles its stem, so only its two dots together find it

The second half of #58, and the last of the four grouping faults #48's bench turned up.

A diaeresis is two components either side of the letter they belong to — dots at x84 and x102 over a
stem at x93 in Arial at 96px. **Neither dot overlaps the stem and neither centre falls on it.** Only
their union does, and marks are matched to bodies one at a time, so `Ï` and `ï` never grouped.

### The discriminator is not a distance

The issue named the hazard: **the dots of `ii` are also two marks at the same height**, and merging
those would attach one mark to one stem. A gap threshold would have to separate a diaeresis (dots
~9px apart) from two `i` dots (~18px), which is two-to-one and font-dependent — the same kind of
coincidence `ì`'s 53% turned out to be.

There is a better signal available for free. **Only marks that failed to find a body on their own
are retried in pairs.** Each `i` dot sits directly over its own stem and matches it individually, so
neither is ever an orphan and the pair never reaches the retry however the threshold is set. Both
dots of a diaeresis fail individually, and that failure is the signal.

The horizontal gap survives as a secondary guard, together with a test that the two marks share a
top — an apostrophe beside a hyphen is two bodiless marks side by side, and merging them would make
one glyph matching nothing, which is worse than the two it replaced.

### What it moves

| | before | after |
| :--- | ---: | ---: |
| Marks reaching their body, every size 21–50px | 46 of 51 | **48 of 51** |
| Accuracy fixture, CER | 10.6% | **9.6%** |
| Post-correction, on | 8.5% | **7.4%** |
| `naïve` | `na<?>I<?>ve` | **`naïve`** |

And post-correction drops from seven substitutions to six, because the seventh was rewriting a
*fragment* of the shattered `ï` rather than a letter. All six that remain are genuine `l`/`I` fixes.
See `docs/post-correction.md`, where that was recorded as the honest description of a corrector
working on a line another stage had already broken — the upstream fix is what removed it.

### What did not happen

The prediction said a double quote would become one glyph, since `"` is two marks side by side with
no body under them. **It does not**, and that is a consequence of the safety rule rather than an
oversight: a rejoined pair is only kept if it *finds a body*, and `"` has none. So it falls through
to `cluster_orphans` unchanged and still reads as two single quotes, which `group`'s module doc
still records as a known limitation.

Merging bodiless marks is what the apostrophe-and-hyphen case argues against, so the conservative
rule is the right one — but the prediction was wrong about it and the limitation stands.

## Only unmarked glyphs may say where the cap line is

#75, found within minutes of #57 landing and caused by it becoming reachable.

`metrics::anchors` chose the cap line as *the highest row that enough glyphs reach*, with
`min_cap_support_percent` at 15 as the floor. On a seventeen-glyph line that floor is two. The three
accented capitals of `ÉTAIT-CE ÀPRE? ÎLE.` sit together eight pixels above the eleven plain ones,
clear a floor of two between them, and `.min()` takes their row.

Cap height then came out 38 instead of 30, and every glyph on the line was measured against a unit
27% too large:

| | measured | its reference | penalty |
| :--- | ---: | ---: | ---: |
| `Î` against `Î` | 100% | 126% | ~13 cells |
| `Î` against `I` | 100% | 100% | 0 |

So `Î` — the character #57 had just fixed the segmentation of — matched `I`, on a shape distance of
6 against its own exact match at 0.

### The fix is not a mode

The obvious repair is to make the cap line a mode, the way the baseline is, and the comment above
that code even claims the same argument applies. **It does not.** On ordinary mixed-case text the
x-height glyphs outnumber the capitals, so the most popular row is the x-height and every capital
would measure 136%. The existing tests caught that immediately.

The cap line is genuinely the *highest well-supported* row. What was wrong is not the selection but
the electorate: **a glyph carrying a diacritic has its box top at the mark**, which sits above the
cap line by construction, so it cannot be allowed to vote on where the cap line is. Only unmarked
glyphs do now. A line with nothing but marked glyphs falls back to all of them, which is worse than
the usual estimate and better than refusing a line that may still be readable.

Before #57 the question could not arise: an accent over a capital banded separately, so the glyph
box stopped at cap height and never rose above it.

### What it moves

| | before | after |
| :--- | ---: | ---: |
| Accuracy fixture, CER | 11.0% | **10.6%** |
| Post-correction, on | 8.9% | **8.5%** |
| `ÎLE` | `ILE` | **`ÎLE`** |

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

## Vectorizing the body alone, and finding the merged box was load-bearing

[#100](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/100), benched before anything
was built — the convention #37, #46 and #48 established, and the one that caught #48 being wrong
about its own target. It caught this one too.

The diagnosis above is that `feature::vectorize` runs on the **merged** box, base plus mark, so an
accented letter is letterboxed into the bottom five-sixths of the grid while its base fills all of
it. `á` is a third shape, different from `a` and from `e` alike, and which it lands nearer is close
to arbitrary. #100 proposed handing `vectorize` the **body's** box instead, with the mark travelling
separately as `MarkSlope` — which it already does.

**The diagnosis is right and the fix is not available.** `xtask body-box`, on Arial:

| | shape, merged | body only | body + metrics | to its rival |
| :--- | ---: | ---: | ---: | ---: |
| `á` against `a` | 110 | **0** | 14 | 53 (to `e`) |
| `è` against `e` | 112 | **0** | 14 | 37 (to `o`) |
| `ò` against `o` | 99 | **0** | 14 | 37 (to `e`) |
| `ù` against `u` | 66 | **0** | 14 | 55 (to `o`) |

#100's first prediction was that the body-only vector would "sit within noise of `a` and clear `e`
by at least 20 cells". It does better than that: it sits at **distance zero** from its base and
clears its rival by 37 to 55.

Which is the problem. `á` no longer differs from `a` in the shape vector *at all*.

### What that costs, counted

The change moves 51 of 139 characters. Pairs inside the ambiguity margin, under the full shipped
distance rather than shape alone, because that is what the matcher computes:

| representation | pairs inside the margin | of which at distance zero |
| :--- | ---: | ---: |
| merged, shape alone | 41 | 3 |
| **merged + metrics (shipped)** | **24** | **1** |
| body only, shape alone | 153 | 151 |
| **body only + metrics** | **77** | **60** |

Three times the ambiguity and sixty times the ties. Verdana, where `l` and `I` are drawn
differently and the shipped set has *no* pairs at distance zero at all, moves 8 to 70 and 0 to 31 —
so this is a property of the representation and not of one typeface. And #37's line-metric term rescues more of it
than expected — an accented letter is taller than its base, so `á` against `a` recovers to 14 cells,
clear of the 7-cell margin. What it does not rescue is where the *heights* also coincide:

```
  I / i  0     i / l  0     l / ì  0
  I / l  0     i / ì  0     l / í  0
  I / ì  0     i / í  0     l / î  0
  I / í  0     i / î  0     l / ï  0
```

Every dotted letter's body is a bare stem, and a bare stem is `l`, `I` and `|` — the pair #10
measured at distance **zero** and the class `docs/error-census.md` measured as **24.4% of a real
disc's remaining errors**, the largest one left. The proposal trades an accent confusion for a
worse instance of the confusion the project already has most of.

Since this was written, #110 has separated `l` from `I` — by an **ink aspect ratio** carried beside
the metrics, because Arial draws `I` 7% wider and the 16-cell grid rounds that away. It does not
rescue the proposal above. The pair it separates differ in width; the pairs this table lists —
`i`/`l`, `l`/`ì`, `i`/`í` — are *the same bare stem*, identical in width as well as in height, and
no ratio distinguishes a shape from itself. The body-only vector still trades one confusion for
sixty ties.

### Why the mark term cannot put it back

The obvious answer is to turn on `mark_weight_permille`. It cannot help, and the reason is
structural rather than a matter of weight:

```
  pair            slope   shape distance   slope difference
  a against a      none                0      no comparison
  à against a        68                0      no comparison
  á against a       -67                0      no comparison
  â against a         1                0      no comparison
  ä against a         0                0      no comparison
```

`MarkSlope::difference` returns `None` when either side is unknown, and an unmarked letter is
unknown by definition. That refusal is right — `docs/glyph-stability.md` records why an unmeasurable
mark must contribute nothing rather than a fabricated zero — and it means the term contributes
**nothing at all** to the one pair a body-only vector creates. At any weight.

The second row of that table is the other half: a circumflex reports slope 1 and a diaeresis
reports 0. #48 built the slope to separate *grave from acute*, and it does that at ten to twenty
times its noise. Separating *marked from unmarked*, and separating *one mark from another*, are two
different questions, and it answers neither.

### What would be needed, and what it costs

A mark **identity** feature, which is #48's candidate B — the mark's own feature vector. #48
measured it: it separates all sixteen grave/acute pairs, and it clears its own rendering noise by a
factor of **1.6**, against the slope's ten to twenty. It also costs 32 bytes per reference entry and
a format version. #48 rejected it on the ratio; nothing here changes that number.

### Predictions, scored

- **1. Body-only `á` sits within noise of `a` and clears `e` by at least 20 cells.** *Right, and
  further than predicted.* Zero and 53.
- **2. All eight census characters read correctly, where four do today.** *Not reached.* The census
  is a decoded-material measurement and the necessary condition failed on font renders first, which
  is what the bench is for.
- **3. CER moves by less than 1 point, and that is not a failure.** *Not reached, and the premise
  moved:* `docs/error-census.md` found **no accented confusion at all** on the real disc, because
  the material is English. The payoff is bounded by material this project has not measured; the cost
  is certain.
- **4. The mark-slope term stays at 0 and stays worth re-sweeping.** *Right about the weight, wrong
  about the reason.* It is not that the term separates an axis that does not matter yet — it is that
  the term cannot express presence, so it could not carry this change even if it were free.

### What it says

**Closed, not built.** The merged box is the mechanism, exactly as #48 diagnosed, and it is also the
only place the mark's identity is recorded. Taking the mark out of the vector without putting it
somewhere else does not make `á` into `a` plus a mark; it makes `á` **into `a`**, and `ì` into `l`.

The lesson is one the project keeps relearning in new forms: a representation that collapses two
things is a fault only if something else can tell them apart. The letterbox merge looked like the
fault. It was the record.

## A feature that separates every pair it was asked about, and decides nothing

[#232][issue-232] went looking for why `Five` reads as `FÍve`, expecting the merged base-plus-mark
box to be telling #37's term that an `i` is a capital. The diagnosis it asked for first found
something else, and the something else looked like the best axis this project has measured.

`xtask glyph-geometry` has always printed a row nothing consumed: **ink pixels divided by height**,
which is a glyph's *mean* width where [`InkAspect`] is its *extent*. For a solid bar the two are one
number. They come apart wherever a glyph has a hole in it, and the largest confusion family here is
made of glyphs with holes.

Normalised against the glyph's own height — [`InkAspect`]'s own measured choice, re-measured here
rather than inherited — the best single threshold, over glyphs labelled by what the release says
they were:

| pair | 10 Cloverfield Lane | Gone Girl | A Fish Called Wanda |
| :--- | ---: | ---: | ---: |
| `i` / `I` | **0.0%** | **0.9%** | **0.0%** |
| `i` / `l` | **0.0%** | **1.6%** | **0.0%** |
| `l` / `I` | **0.0%** | **2.2%** | 14.7% |

Against what the pipeline has: `InkAspect` reads the same `i`/`I` population at **17.8%** on Gone
Girl, and `docs/glyph-hit-list.md` measured `l`/`I` at 17.0% there and 15.2% on Wanda.

Two things about that table are worth keeping whatever happens to the feature.

**On Wanda the standard deviation is zero on both classes.** That disc is the one
`docs/glyph-hit-list.md` describes as having no axis at all — *"the disc draws `l` and `I` at five
pixels at the 25th, 50th and 75th percentile alike"*, and *"no threshold does better than ignoring
the measurement"*. On mean width its `i` reads 10.20 and its `l` reads 11.90, sd 0.00 on each. The
information was in the ink the whole time; extent is what could not see it.

**And it is fusion-resistant, which nothing designed it to be.** Gone Girl's `l` population runs out
to 27 pixels of ink width with a standard deviation of **3.92**, because a fifth of the class is
fused to a neighbour. A fusion adds box and adds proportionally little ink, so the same glyphs have
a standard deviation of **0.70** here. That is the exact contamination `glyph-hit-list.md` names as
what defeats the threshold on that disc rather than the axis.

### Built, swept, and it moves nothing

`StemWidth` was added to the core, to `Weights`, to the matcher and the clusterer together, and to
the reference format as version 5 with a back-compatibility test. `xtask stem-sweep` ran the whole
pipeline at nine settings against four discs, the shape #110 established for a matcher weight.

The column to read is the confusion the term separates. On The Karate Kid, where `l → I` is the
disc's largest error:

| weight | CER | `l → I` | unread | cues worse |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 2.2% | **638** | 67 | 0 |
| 40 | 2.2% | **638** | 67 | 0 |
| 90 | 2.2% | **638** | 77 | 11 |
| 190 | 2.3% | **638** | 100 | 42 |

**638 at every setting.** Not reduced, not worsened — untouched, while the cost climbs. The other
three discs say the same thing: Gone Girl is 1.4% from 0 to 190 and 1.7% with 66 cues worse at 300;
Wanda moves 1.8% to 1.7% and puts 2 cues in the worse column from weight 60 up; Cloverfield is flat
until 300, where it loses 10 cues. Priced across the whole bench at 190, Training Day goes from 1.9%
to **4.6%** and its read fraction from 99.7% to 97.8% — the term rejecting glyphs it should have
been deciding.

### Why, as far as the measurement can say

The reference side and the disc side do not agree about the value for one character, and the
disagreement is the size of the signal. `xtask glyph-geometry --font` on Cloverfield: the disc draws
`I` at **14.29%** of cap height and Arial's own outlines predict **13.11%**, a gap of 1.18 points —
against an `l`-to-`I` separation of 2.4 points on the disc that needs it most. Half the signal is
spent before the comparison starts.

That is #113's class of defect exactly — *a term measured one way on the disc and another in the
set* — and it is why the calibration row now exists in `glyph-geometry` beside the cap-relative one:
the matcher compares a glyph to an entry, and a reader compares characters to each other, and those
are not the same question.

### What this says, and it is not that the axis is bad

Two rules meet here and the second is new.

`docs/glyph-stability.md` has recorded since #48 that **"ambiguous" and "wrong" are not the same
set** — a statistic over the reference set cannot say which glyphs land on the wrong entry. This is
the converse and it costs more to learn: **"separable" and "decisive" are not the same set either.**
A labelled distribution with zero variance and disjoint classes is a fact about what the disc drew.
Turning it into a match needs a reference side that agrees, and nothing about the separation implies
one exists.

So the axis is live and the term is not. What it wants is a reference value taken from *this disc*
rather than from an outline — which is [#233][issue-233], and this is the sharpest argument for it
the project has: a feature that is perfectly separable on the material and unusable against a
rendered set is precisely the case an adapted set exists to serve.

**Nothing ships but the instrument.** `glyph-geometry` keeps two new rows — the own-height axis on
the disc side, and the same quantity on both sides in the calibration table — so the next attempt
starts from a measurement rather than from this document.

[issue-232]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/232
[issue-233]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/233

## Templates cut from the disc's own ink, and why the idealisation was load-bearing

[#233][issue-233], and it is the strongest-looking proposal this document has ever refused. Its
argument is the conclusion of the section above: the dominant variance term is one pixel of weight
on a stroke, no representation can argue that away, and **a template cut from this disc does not
have that term at all** because it was cut at this disc's weight. That is Tesseract's adaptive
classifier, a structure with decades behind it.

It is also not #10. #10 grouped a stream's *unlabelled* shapes by proximity and died on `l`, `I` and
`|` at distance zero, so no radius exists. A template promoted from a first-pass read carries a
label and has no radius — it is compared to an observation exactly the way a rendered entry is,
which is the distinction `xtask shape-votes` drew for the same reason.

`xtask adaptive` surveys a track, promotes every shape the first pass read confidently, and reads
the track again against the rendered set plus what it learned. The seeding rule is structural rather
than a threshold: **a shape may become a template only if the character it was read as belongs to no
confusion set.** Promote Wanda's `l` and the second pass gets more confident about the same 428
errors while every statistic improves, which is the failure `glyph-hit-list.md` describes and the
one this had to be built not to commit.

### The first attempt failed for a reason that was mine, and it is worth naming

Learned entries reported `LineMetrics::UNKNOWN` and `InkAspect::UNKNOWN`, on the argument that a
shape's occurrences differ in both and picking one would be arbitrary. `Weights::distance` omits a
term when **either** side lacks it — so those entries paid no metric penalty and no width penalty at
all, competing on bare Hamming distance against rendered entries that paid both. 1,205 such entries
against 478 rendered ones took The Karate Kid from 1.7% to **5.6%**, with 919 cues worse.

Carrying the first occurrence's measurements fixes that, and the fix is what makes the real result
visible.

### No floor works, and that is the finding

`--min-occurrences` is the one knob: how often a shape must recur before it is trusted as a template.

| track | floor | learned | CER | better | worse |
| :--- | ---: | ---: | ---: | ---: | ---: |
| A Fish Called Wanda | 3 | 69 | 1.3% → 1.7% | **0** | 70 |
| A Fish Called Wanda | 10 | 37 chars | 1.3% → 1.4% | 7 | 24 |
| A Fish Called Wanda | 100 | 18 | 1.3% → **1.3%** | **0** | **0** |
| A Fish Called Wanda | 500 | 6 | 1.3% → **1.3%** | **0** | **0** |
| Gone Girl | 3 | 80 | 1.3% → 1.5% | **0** | 68 |
| The Karate Kid | 3 | 1,205 | 1.7% → 5.6% | 20 | **919** |

Read the two ends. **At a high floor the templates change nothing at all** — byte-identical output,
because the eighteen most-drawn shapes on the disc are shapes the rendered set already reads
correctly. **At a low floor they are strictly harmful** — 70 cues worse and *zero* better on Wanda,
68 and zero on Gone Girl.

There is no setting between them where it gains. That is the shape #225 established for a refusal
here: sweep the constant, or show that no setting of it works.

### Why, and it is #10's finding arriving somewhere new

A learned template is a **real** shape. The rendered entry it competes with is an **idealised** one.
Real shapes of different characters sit closer together than idealised shapes of different
characters do — which is exactly what #10 measured when it found the nearest different character at
distance zero, and exactly what the inter-character column in this document has always said.

So a template cut from the disc attracts its own character's other renderings, and it attracts
*other characters'* renderings at least as strongly. The unread count is the tell: with fair
measurements it does not move at all — 79 before and 79 after on Wanda, 68 and 68 on Gone Girl — so
the templates rescue nothing the set could not already read, and every cue they change is one they
took from a rendered entry that had it right.

**The idealisation is load-bearing.** A rendered set is not an approximation of the disc that a
better sample would improve on; it is a set of *separated* points, and separation is the property
that survives one pixel of weight while proximity to any particular rendering does not. #14's
finding that a glyph's vector moves further between two renderings of one character than between two
different characters says the same thing, and it says it about exactly the material a template is cut
from.

### What would have to be different

The measurement kills this design and does not kill the direction. What a template could still be
right for is a character the rendered set cannot spell **at all** — a typeface whose `a` is a shape
no candidate holds — where there is no rendered entry to steal from and the comparison is against
nothing rather than against something better. That is a narrower claim than #233 made and it needs a
track the set genuinely fails on, which the bench does not have: every entry on it fits an Arial the
project renders.

`xtask adaptive` stays, because the next attempt should start from this table rather than from this
paragraph.

[issue-233]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/233

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
- **The merged base-plus-mark box cannot be unmerged** — #100, benched and closed above. Handing
  `vectorize` the body's box alone puts `á` at distance *zero* from `a` and `ì` at zero from `l`,
  and `MarkSlope` cannot put back what it removes because an unmarked letter reports `NONE` and
  contributes nothing to any distance. Reopening it needs a mark *identity* feature, which is #48's
  candidate B at 32 bytes an entry and a 1.6x noise margin.
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
