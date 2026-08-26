# What a disc's subtitle palette actually holds

[#234][issue-234]. `binarize.rs` has opened with the same sentence since it was written:

> The mask itself and the threshold policy are implemented; classifying palette indices into fill,
> outline and anti-aliased edge is #5.

#5 is closed. That sentence is still true, and `Threshold::default` still describes its own values
as *"fill only, at half alpha and half luma. A starting point, not a measured answer."* Every glyph
this pipeline has ever read was cut at `min_luma: 128` with the outline discarded, and **nothing in
this repository could print what was being cut.**

Two measurements had wanted it before this existed. `docs/glyph-stability.md` refused
palette-adaptive thresholding on a claim about palettes, made without counting one. And [#235]'s
grey coverage collapses on VOBSUB with a four-entry palette as its leading explanation and nothing
able to check.

`xtask palette` counts them. Every share below is of **drawn foreground ink**, weighted by pixels,
because a palette declares up to 256 entries and a subtitle draws what it draws: reporting the
declaration would describe the format rather than the disc.

## The two codecs are not the same kind of thing

| track | codec | entries drawn | fill | outline | edge | widest empty luma band |
| :--- | :--- | ---: | ---: | ---: | ---: | :--- |
| 10 Cloverfield Lane | PGS | 122 | 43.9% | 51.5% | 4.6% | 33..87 |
| Gone Girl | PGS | 122 | 43.7% | 51.4% | 4.9% | 33..87 |
| King Kong | PGS | 124 | 44.4% | 54.5% | 1.1% | 33..87 |
| Airplane! | PGS | **372** | 43.9% | 51.1% | 5.0% | 33..87 |
| A Fish Called Wanda | PGS | 128 | 46.8% | 42.7% | 10.5% | 228..232 |
| The Prestige | PGS | 64 | 52.9% | 36.8% | 10.2% | 205..212 |
| Once Upon a Time in Mexico | PGS | 64 | 48.7% | 33.4% | 17.9% | 205..212 |
| The Karate Kid | VOBSUB | **3** | 49.2% | 50.8% | **0.0%** | 16..147 |
| Training Day | VOBSUB | **3** | 55.1% | 44.9% | **0.0%** | 16..147 |

`fill`, `outline` and `edge` are the three names `binarize.rs` uses, applied at the shipped
threshold: `edge` is partially transparent, `fill` is opaque at or above luma 128, `outline` is
opaque below it.

**A VOBSUB track draws three colours and no partial transparency at all.** Karate Kid: luma 16, 147
and 191. Training Day: 16, 147 and 222. Not "about four" — exactly three, with the entire ink of a
feature film in them.

**A PGS track draws a hundred or more, and authors its anti-aliasing in the *luma* channel at full
opacity.** Cloverfield's largest entries are luma 192 and luma 16 at alpha 255, taking 71% of the
ink between them, and the remaining hundred-odd entries are a ramp between them — also at alpha
255. Only 1% to 18% of PGS ink is partially transparent.

That is the finding the two open questions needed, and it points them in opposite directions.

## It falsifies the sentence that closed palette-adaptive thresholding

`docs/glyph-stability.md`, refusing an adaptive split:

> The reason is visible once measured: subtitle palettes put fill near luma **235** and outline near
> **16**, so a fixed 128 is *already* comfortably in the gap.

Measured, both halves are wrong for PGS:

- **Fill is at luma 192**, not 235, on four of the seven PGS tracks. Wanda and the two 64-entry
  discs put it higher, so there is no single figure to name.
- **128 is not in the gap.** The widest empty band in opaque ink is **33..87** on four discs, and
  the shipped cut sits 41 points above its top — inside the drawn ramp rather than in the space
  below it. On Wanda, Prestige and Mexico the widest band is 4 to 7 points wide, which is to say
  those discs draw a continuous ramp and there is no gap anywhere.

The *conclusion* stands and only its reasoning was wrong: adaptive splitting measured 3.9% worse on
distinct shapes and is still refused on that. But "the threshold is already in the empty space" was
never true of this material, and a future proposal must not lean on it.

**Where the sentence is exactly right is VOBSUB.** 16..147 is a 131-point chasm with nothing in it,
and 128 falls inside — comfortably, and only there.

## What it says to #235

Grey coverage reads opacity times brightness. On a VOBSUB track there are **three** levels and no
partially transparent ink whatsoever, so there is no anti-aliasing ramp to read: the feature has
nothing to gain and can only soften three hard values into something less separable. That is the
hypothesis `glyph-stability.md` records for grey coverage costing 768 cues on The Karate Kid and 167
on Training Day, and this is the measurement that supports it.

## What it says to the two-level mask, which was #234's own proposal

#234 proposed carrying the outline as a second level in the feature vector, on the argument that an
outline is *authored* rather than sampled — a scaled copy of the letterform, stable under the
±1px weight variation `glyph-stability.md` calls the dominant term.

**The survey refuses it for PGS and leaves it open for VOBSUB**, and prediction 1 is how:

> Every bench disc draws a distinguishable outline entry, and the count of drawn entries is small
> and stable within a title.

*Right for VOBSUB and wrong for PGS.* A VOBSUB outline is one entry — luma 16, half the ink, nothing
else near it. A PGS "outline" as the shipped threshold defines it is 51% of the ink spread over
**70 to 243 entries**, which is not an outline at all: it is the dark half of a continuous ramp. A
two-level mask needs a level to point at, and on the codec that is 83% of this library there is not
one.

So the proposal survives only where the codec draws hard edges, which is the codec whose glyphs are
smallest and whose fit is worst. That is a narrower and more interesting question than the one #234
asked, and it is not answered here.

## Reproducing

```console
$ cargo run --release -p xtask -- palette bench-cache/cloverfield.sup
$ cargo run --release -p xtask -- palette "...Karate Kid...mkv" --stream 0 --cues 400
```

`--cues` bounds the pass, which matters only for reading a track out of a container: cues are spread
through a film, so a prefix touches a fraction of a multi-gigabyte rip. A palette does not change
halfway through a title — the two VOBSUB discs report the same three entries over 400 images as over
their first ten.

Entries are keyed on their **value**, not their index. PGS updates a palette incrementally and
VOBSUB carries one out of band, so one index can mean two colours within a track and two indices can
mean one colour. What the survey is asking about is the colours.

[#235]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/235
[issue-234]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/234
