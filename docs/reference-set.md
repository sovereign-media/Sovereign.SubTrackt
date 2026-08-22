# Which reference set should ship

Answers the last open question in [#9][issue-9]: embed a set, and if so, whose typeface — given that
fitting *identified* Arial and identifying is not permission to derive.

**Nothing should be embedded, and the licensing question turns out to be moot.** Not "not yet". The
open substitute that #9 nominates reads Arial-authored material **69% worse** than an Arial set
does, and neither number the accuracy gate can see is able to notice.

[issue-8]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/8
[issue-9]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/9
[issue-13]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/13
[issue-14]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/14

## The measurement that was missing

`xtask accuracy` builds its fixture and its reference set from the *same* font. That is the ceiling
case and it says nothing about an embedded set, because an embedded set is by definition built from
a typeface that is not the one the disc was authored in. So:

```console
$ cargo run -p xtask -- reference-fit C:/Windows/Fonts/arial.ttf \
      LiberationSans-Regular.ttf verdana.ttf tahoma.ttf trebuc.ttf segoeui.ttf
```

One fixture rendered in Arial — the typeface [#8][issue-8] fitted to the library — read by reference
sets built from six fonts. Arial reads itself first, as the ceiling; every other row is what an
embedded set would actually deliver.

## Result

| reference set | CER | WER | coverage | mean distance | vs ceiling |
| :--- | ---: | ---: | ---: | ---: | ---: |
| **arial** (ceiling) | 15.9% | 56.8% | 93.9% | 13.0 | — |
| LiberationSans-Regular | 26.8% | 75.7% | 89.3% | 14.8 | **+11.0** |
| verdana | 27.4% | 78.4% | 92.4% | 21.7 | +11.6 |
| tahoma | 29.3% | 78.4% | 84.0% | 20.9 | +13.4 |
| trebuc | 36.0% | 83.8% | 87.8% | 21.6 | +20.1 |
| segoeui | 37.8% | 83.8% | 93.9% | 22.7 | +22.0 |

Distance ceiling is 51 cells, 20% of the 256-bit vector.

Three things fall out, and each is worse news than the one before it.

### 1. The metric-compatible substitute does not substitute

Liberation Sans is as close to Arial as an openly-licensed font gets: metrically compatible, drawn
to match. As a reference set it lands at **26.8% against Verdana's 27.4%** — a typeface nobody would
mistake for Arial. Being visually close bought **0.6 points**.

This is [#14][issue-14] arriving one level up. Two renderings of the same character are further apart
than two different characters, so a near-clone's `t` is not reliably nearer Arial's `t` than Arial's
`I` is:

```
! over Ihe I?zy dog?          want: over the lazy dog.
! - IsiI4orI?                 want: - Is it 1 or l?
! Io Iow? in 2045?            want: to Iowa in 2015.
```

`t` → `I` in every position. `1` → `4` in every position. `a` unmatched everywhere. These are not
noise; they are *systematic substitutions*, applied confidently and consistently, which is the exact
failure mode this project was built to avoid and the reason it chose a glyph matcher over an OCR
engine in the first place.

### 2. Coverage does not predict correctness

Segoe UI matches **93.9%** of glyphs — the same fraction as Arial — and produces **2.4× the error**.
Tahoma has the worst coverage in the table at 84.0% and reads better than Trebuchet and Segoe UI,
which both cover more.

Every accuracy number this project produced before `xtask accuracy` existed was a coverage number.
This is the clearest demonstration so far that they were measuring the wrong quantity, and it is a
direct problem for [#13][issue-13], whose `Threshold { min_ratio }` gate reads exactly this figure.

### 3. Mean match distance is better, and still not enough

Mean distance does separate the obviously-wrong typefaces — Arial at 13.0, the wrong faces bunched
at 20.9–22.7. That is a real signal and it is now in the extraction report.

But **Liberation Sans sits at 14.8**, nearer Arial's 13.0 than anything else in the table, while
reading as badly as Verdana. A systematic substitution is *by construction* a low-distance one: the
matcher chose `I` for `t` precisely because they were close. The signal that would detect a wrong
reference set is suppressed by the very thing that makes the set wrong.

## What this means for embedding

The argument against embedding is not licensing, and it is not "we could not find a good enough
font". It is that a fixed set of any typeface converts a **detectable** failure into an
**undetectable** one:

| shipped | what a user gets on a disc the set does not match |
| :--- | :--- |
| Empty set (today) | Every glyph unmatched, track refused, caller falls back to burn-in |
| Any embedded set | ~73% correct text, ~27% confidently wrong, and no counter that says so |

The second row is Tesseract's failure mode. §4 of #1 rejected general OCR to avoid exactly it, and
`Confidence` counts glyphs rather than estimating probabilities so that the failure stays a fact. An
embedded set spends that property to make the tool feel like it works.

And the +11 points is not a worst case. #8 fitted the library to "Arial **or very close**"; this
table prices "or very close" at eleven points. Real discs are the Liberation row, not the Arial row,
whatever font gets embedded.

## The licensing position, for the record

Since nothing is being embedded, nothing needs licensing — but the reasoning should not have to be
reconstructed if that changes:

- **Arial is Monotype's.** `xtask gen-reference` reads a font the developer already has and writes
  256-bit normalised bitmaps. Whether that output is a derivative work of the font program is
  genuinely unsettled — typeface *designs* are not copyrightable in the United States while font
  *programs* are, and a downsampled bitmap is neither cleanly. It was never worth resolving, because
  the measurement above says the artefact is not worth shipping.
- **Liberation Sans is OFL 1.1** and could have been embedded freely. It was measured for exactly
  that reason. Version 2.1.5, SHA-256 `76d04c18ea243f426b7de1f3ad208e927008f961dc5945e5aad352d0dfde8ee8`,
  is what the table above used.
- **No font file is redistributed by this repository**, and the checked-in fixture is our own
  rasterisation of our own text. That position is unchanged and is recorded in
  `crates/subtrackt/tests/fixtures/MANIFEST.md`.

## Consequences, stated plainly

**The tool does nothing out of the box.** `subtrackt extract movie.sup` matches zero glyphs and the
default gate refuses the track. `--version` now says so — `reference set: empty, 0 glyphs` — so a
user hitting it can find out why from the tool rather than from the source. That is honest, and it
is not a product.

What would make it one is not a better font. It is **per-title reference data**: the material's own
typeface, fitted once per disc or per library, rather than one set fitted to everything. #8 already
showed a dominant glyph family runs through the library and that fitting beats guessing; this table
shows the fitting has to happen closer to the material than a shipped binary can get. That is a new
issue, and it is the one that unblocks the product.

## What follows

- **Do not embed.** [`reference::embedded`] stays empty, now against a measurement rather than
  against a pending one. The `#9` checklist item "embed a set once it is worth embedding" is
  answered: it is not, and the reason is not going to change by picking a different font.
- **Coverage is not a correctness gate.** [#13][issue-13] has to account for this; mean match
  distance is a better signal and is now reported, but the Liberation row shows it is not sufficient
  either.
- **Per-title fitting is the unlock.** Everything above is an argument for deriving the reference
  set from the material rather than shipping one.
- **`reference-fit` is the instrument for all of it.** Any future claim that some set is good enough
  to embed should arrive as a row in this table.

[`reference::embedded`]: ../crates/subtrackt-glyph/src/reference.rs
