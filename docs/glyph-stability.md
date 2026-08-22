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

## What follows

- **#10 needs redesigning, not implementing.** It is written as per-glyph matching against a fixed
  set. It should become: cluster a stream's shapes, then match centroids.
- **#9 cannot embed its way to a solution.** The fixed set identifies the typeface and seeds labels;
  it will not carry the load alone.
- ~~**Reducing edge sensitivity in binarization**~~ — **tried and failed**, see above. Two
  approaches measured neutral-to-worse. The lever is the binary mask itself, not the threshold
  placement, which makes it a #7 question about carrying grey coverage into the feature vector.
