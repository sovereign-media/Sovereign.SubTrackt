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

| weight | plain, exact | varied, exact | plain, near miss | varied, near miss |
| :--- | ---: | ---: | ---: | ---: |
| 0 (shape only) | 16.9% | 23.9% | 32.3% | 28.5% |
| 25 | 16.1% | 19.4% | 29.8% | 24.7% |
| **50** | **16.1%** | **17.0%** | **24.2%** | **22.8%** |
| 75 | 16.1% | 17.0% | 25.0% | 23.1% |
| 150 | 16.1% | 18.3% | 25.0% | 26.0% |
| 250 | 16.9% | 22.6% | 31.5% | 31.1% |

CER, lower is better. **Fifty is best or tied-best in all four**, which is a clean choice rather than
a fitted one, and it is what ships. At that weight a full cap-height difference is worth 14 cells —
twice the ambiguity margin, comfortably inside the 51-cell match ceiling.

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

Word spacing (#11) and `l`/`I` (#12). The first is a gap threshold; the second needs context, and
the matcher now flags it as ambiguous rather than answering silently, which is what makes #12
tractable.

## What follows

- ~~**#10 needs redesigning, not implementing.**~~ Redesigned as cluster-then-match, implemented,
  and **measured worse at every radius** — see above. The premise was that within-stream variation
  is small enough to group safely; the measurement found the nearest different character is at
  distance *zero*, so no radius exists. Clustering ships off.
- ~~**The next experiment is a line-relative size feature**~~ — **done, and it works**: #37, see
  above. 5.8 to 8.1 points of CER on realistic conditions, and zero-distance pairs down from three
  to one. The first change to aim at separation rather than variance, and the first to pay.
- **What remains is not shape.** Word spacing is #11 and `l`/`I` is #12, which now receives those
  glyphs flagged as ambiguous rather than answered silently.
- **#9 cannot embed its way to a solution.** The fixed set identifies the typeface and seeds labels;
  it will not carry the load alone.
- ~~**Reducing edge sensitivity in binarization**~~ — **tried and failed**, see above. Two
  approaches measured neutral-to-worse. The lever is the binary mask itself, not the threshold
  placement, which makes it a #7 question about carrying grey coverage into the feature vector.
