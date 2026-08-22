# The reference side, and the box nobody was looking at

[#99][issue-99]. `docs/error-census.md` had just found that **48.8% of a real Blu-ray's errors were
the full stop**, matching nothing at all — 60 cells from the nearest entry in a reference set built
from the material's own typeface, against a 51-cell ceiling. #99 was the issue that predicted it
before the census existed, and this is what came of running its bench.

**The disc reads at 2.8% character error, down from 5.5%. Coverage is 99.5%, up from 96.2%. The
ceiling fixture is at 6.4%, down from 9.8%.** The change is two entries per character instead of
one, from a single rasterisation at a single threshold, differing only in **which box the
normalisation letterboxes**.

[issue-99]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/99
[issue-98]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/98
[issue-45]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/45

## What #99 thought the mismatch was

Two things, both real and both named in the issue:

- **Size.** `font::RENDER_PX = 96.0`, against the 21–50px glyph heights the library survey found.
- **Edge treatment.** The reference rasterises a *plain* glyph and thresholds at half coverage. Real
  material is anti-aliased fill inside a 1px dark outline, and the binarizer
  [keeps fill only](../crates/subtrackt-glyph/src/binarize.rs) — so the material glyph's edge sits
  where the ramp between fill and *outline* crosses, not where the ramp between glyph and
  *background* does.

The fixture generator imitates the outline deliberately, citing #14's 30-cell finding.
`gen-reference` did not. So `xtask accuracy` — the ceiling case, the measurement the whole project
treats as its upper bound — compared an outlined material glyph against a plain reference glyph and
priced the difference at zero.

Both were worth measuring. Neither is the answer.

## The third one, which is the answer

Building the bench turned up something the issue had not named. The runtime letterboxes a
**connected component's** bounding box — the ink that survived thresholding, because that is what
`ccl::label` hands over. `font::vector_for` letterboxed **fontdue's** box, which includes every
pixel with any coverage at all, down to 1.

Those differ by roughly a pixel. What makes that matter is that it is a pixel, not a fraction:

| glyph at 96px | box | one-pixel inset is |
| :--- | ---: | ---: |
| `M` | 68px tall | 1.5% of it |
| `.` | 13px tall | **15% of it** |

And letterboxing is the operation that turns a row of pixels into a whole grid cell. On an `M` the
two boxes normalise to the *same* 256-bit vector. On a full stop they land 28 cells apart on Arial —
and against real material, 60.

That is #99's third prediction — "small glyphs are worst… a period is a few pixels square and
letterboxes to a solid block, while a 96px period letterboxes to a disc" — with a mechanism the
prediction did not have. It is not the disc-versus-plain rendering. It is a fixed margin whose cost
scales as 1/size.

## The bench, and the controls that identify it

Two instruments, because the first one got the answer wrong.

**`xtask reference-render`** measures the gap in cells: reference vectors against material
renderings — anti-aliased fill inside a 1px outline, through `Binarizer`, exactly as the runtime
would — across 22–49px, judged against each character's own noise the way #48 was judged.

Its verdict, on Arial, over 973 character-and-size samples:

| rendering | gap p50 | unread | wrong | of which case |
| :--- | ---: | ---: | ---: | ---: |
| 96px, ink 128, raster box (shipped) | 13 | 19 | 85 | 32 |
| crop to ink only | 10 | 13 | 71 | 19 |
| material ink only | 13 | 20 | 91 | 36 |
| both, still 96px | 10 | 10 | 66 | 11 |
| material 21+29+38+50px | 8 | 4 | 54 | 13 |

It ranked a four-size material set best on both of its columns. **The first thing that set did end
to end was read every `t` as an `I`** — 1,231 times on the disc, taking CER from 5.5% to 9.8%.

Cells are not characters. `docs/glyph-stability.md` records the same lesson about `xtask
separability` and #48; this is the second instance, and it is why the second instrument exists.

**`xtask render-sweep`** generates a real `.subtref` per candidate and runs the ceiling fixture
through it. Against a real disc, driven by `xtask gen-reference --renderings`:

| reference set | bytes | coverage | CER |
| :--- | ---: | ---: | ---: |
| 96px / ink 128 / raster box — before #99 | 12,534 | 96.2% | **5.5%** |
| 96px / ink 128 / ink box, *alone* | 12,534 | 99.5% | 9.6% |
| **both boxes** | **21,444** | **99.5%** | **2.8%** |
| + a second raster box at ink 140 | 17,709 | 96.2% | 5.5% |
| + a second raster box at 48px | 24,684 | 96.2% | 5.5% |
| + a second raster box at 90px | 23,694 | 96.2% | 5.5% |
| + an ink box at 21px, material ink | 24,639 | 99.5% | 2.8% |
| + an ink box at 50px, material ink | 36,204 | 99.5% | 2.8% |
| + an ink box at 96px, material ink | 23,334 | 99.5% | 2.8% |
| three boxes | 32,964 | 99.5% | 2.9% |

The three control rows are the point. **A second entry that keeps the same box changes nothing at
all** — not the coverage, not the error rate, not by a single character, at any size or threshold.
**A second entry that changes the box halves the error rate** — at any size, at any threshold. The
size does not matter. The ink threshold does not matter. The box is the whole effect.

A third box, inset further, gains nothing.

## Why *both* rather than the right one

The ink box is the one the runtime uses, so the obvious fix is to switch to it. That row is in the
table: **9.6%, worse than doing nothing.** Coverage goes to 99.5% and the full stop is fixed, and
then `t` reads as `I` 1,231 times.

The reason is visible in the grid. A `t`'s stem at 96px is about 1.4 cells wide, so whether it
occupies one cell or two is decided by where the box edge falls relative to the cell grid — a phase,
not a width. The raster box and the ink box put it in different phases:

```
  raster box        ink box           material at 33px
  .......##.......  .......#........  .......#........
  .....######.....  .....######.....  .....#####......
  .......##.......  .......#........  .......#........
```

Neither phase is *correct*. The material's own phase depends on the glyph's size in the stream, and
a feature film's glyphs are not all one size. So both are carried, which is carrying the axis
instead of guessing a point on it — and the axis is exactly the ±1px edge shift
`docs/glyph-stability.md` already priced at **30 cells, as much as character identity itself**. That
finding has stood since #14 as a hazard every experiment since has tried to absorb; this is the first
change to treat it as a dimension of the reference set instead.

The extra entries are not a flat doubling. Where the two boxes normalise to the same vector the
duplicate is dropped, which on Arial is most of the capitals: 476 entries against 278 for a set with
an italic cut, 21,444 bytes against 12,534. Extraction time is unchanged at 14.8 seconds — the
session cache still answers 100% of glyphs, so the scan count is per *cluster*, not per glyph.

## What it did

On the disc, from `xtask srt-score`'s census:

| release → read | before | after |
| :--- | ---: | ---: |
| `.` → unread | 660 | **6** |
| `l` → `I` | 330 | 330 |
| `y` → unread | 45 | 45 |
| everything else | 318 | 306 |
| **total errors** | **1,353** | **687** |

Unread glyphs, from `xtask unread`: **775 → 105**, and the 5×5 component that was 87% of them is
down to 6. What remains is 41 components at 42×44 and 27 at 31×42 — full-size glyphs that are a
different problem.

On the ceiling fixture: 9.8% → **6.4%** CER, 15 unread → 2. That is #99's fourth prediction, which
asked only that the number move below 9.8%.

## Predictions, scored

- **1. The reference-to-material distance is ≥20 cells at 21px and never below ~10.** *Narrowly
  wrong on both bounds, in the same direction.* 19 at 21px, falling to 9 at 50px. Directionally
  right, and the median understates it — p95 is 46 at 21px.
- **2. Dominated by the edge treatment, not by size.** *Wrong, and both halves were wrong.* Neither
  matters. Moving the threshold from 128 to 160 at a fixed box costs nothing; moving the size from
  96px to 21px at a fixed box costs nothing. What the prediction called "edge treatment" turned out
  to be a proxy for the box, because a material rendering is cropped to its ink and a plain one was
  not.
- **3. Small glyphs are worst; the full stop is why `.` matches nothing.** *Right, and it was the
  single most valuable prediction in the issue.* It is 48.8% of the disc's errors and 87% of its
  unread glyphs, and the mechanism is a fixed inset against a shrinking box.
- **4. Closing it moves the ceiling fixture below 9.8%.** *Right.* 6.4%.

## The re-fit #45 demands

Anything that changes what a reference vector *is* silently re-prices every threshold fitted against
the old one. #45 is the cautionary case — an un-refitted metric weight cost up to 12.8 points with
nothing erroring and no counter moving. So `xtask metric-sweep` was re-run on all four conditions:

| condition | best weight | CER there |
| :--- | ---: | ---: |
| plain, reference typeface exact | 98–977 (flat) | 6.4% |
| varied, reference typeface exact | **196** | 7.8% |
| plain, reference typeface a near miss | **196** | 21.6% |
| varied, reference typeface a near miss | **196** | 15.9% |

`metric_weight_permille` stays at 196. It is still the argmin on every condition that has one, which
is the answer this check was run to get rather than one it was assumed to give.

## What is now the largest error class

`l` read as `I` — 330 errors, **48% of what remains**, exactly where the full stop used to sit. It
is untouched by any of this, and it is the pair #10 measured at distance *zero*. Post-correction's
context arm already fires on it: all 363 corrections on this track are `I` → `l` and nothing else,
and 330 of the pair still come out wrong afterwards.

## Reproducing

```console
$ cargo run -p xtask -- reference-render C:/Windows/Fonts/arial.ttf
$ cargo run -p xtask -- reference-render C:/Windows/Fonts/arial.ttf --show tIl.
$ cargo run -p xtask -- render-sweep C:/Windows/Fonts/arial.ttf
$ cargo run -p xtask -- gen-reference C:/Windows/Fonts/arial.ttf out.subtref \
      --italic C:/Windows/Fonts/ariali.ttf --renderings 96:128:raster
$ cargo run -p xtask -- metric-sweep C:/Windows/Fonts/arial.ttf
```

`--renderings px:ink:crop,...` is an xtask flag and deliberately not on the shipped CLI. It exists so
the sweep above can generate sets the tool would never write — including the pre-#99 one, spelled
`96:128:raster`. A user has no such question; a user wants the set the measurement chose.
