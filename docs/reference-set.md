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

## The same question against a real disc

Everything above is a fixture: rendered by this project, in a font that is *in* the candidate list.
The caveat is stated in #43 and it is the right one — an encouraging signal, not a result. So the
same question was put to a Blu-ray.

**10 Cloverfield Lane (2016)**, 1.8 GB, one PGS track at 1920×800. 822 cues in 70 seconds. Scored
against the English subtitle shipped beside the rip — a different release of the same film, which is
not ground truth and is not claimed as any: it is an independent transcript of the same dialogue,
produced without reference to this pipeline. Enough to rank ten extractions of one track against
each other and to tell 7% from 60%. Not enough to certify an absolute figure.

```console
$ subtrackt extract '10 Cloverfield Lane (2016).mkv' --reference arial.subtref \
      --on-unmatched placeholder --format srt --report -o out.srt
822 cues from 822 images (1644 packets); glyphs 19566 matched / 958 unmatched
  / 3878 ambiguous (95.3% read); fit 11.7; cache 100%
$ cargo run -p xtask -- srt-score out.srt release.eng.srt
```

Ten candidate typefaces, one extraction each:

| reference set | fit | CER, all | CER, upright | CER, italic |
| :--- | ---: | ---: | ---: | ---: |
| **arial** | **11.7** | **8.8%** | **7.1%** | 35.7% |
| tahoma | 18.5 | 17.4% | 15.8% | 42.8% |
| verdana | 20.2 | 16.6% | 15.4% | 34.0% |
| trebuc | 20.6 | 29.7% | 28.2% | 51.1% |
| calibri | 22.6 | 30.9% | 30.2% | 42.7% |
| segoeui | 25.7 | 28.3% | 27.4% | 41.5% |
| arialbd | 28.8 | 36.4% | 35.6% | 48.1% |
| georgia | 31.0 | 43.1% | 41.5% | 67.0% |
| times | 31.5 | 62.3% | 61.8% | 68.8% |
| ariali | 35.2 | 38.6% | 40.5% | **10.8%** |

### The argmin holds, on material this project did not render

Mean match distance picks Arial, and Arial is the right answer by a wide margin: 11.7 against 18.5
for the runner-up, 8.8% CER against 16.6%. That is the only comparison a selector makes, and it is
now made once on a real disc rather than only on a fixture rendered in a candidate's own font.

It is no better among the also-rans than it was on the fixture. Tahoma and Verdana invert. Times
reads at 62% — worse than anything else by twenty points — and mean distance ranks it *ninth of ten*,
under-penalised for exactly the reason §3 gives: a systematic substitution is by construction a
low-distance one.

### A floor, with numbers under it for the first time

#43 asks for a floor: refuse rather than fit to the least-bad candidate. The gap here is wide enough
to place one.

| | fit |
| :--- | ---: |
| The right typeface, this disc | 11.7 |
| The right typeface, the accuracy fixture (exact by construction) | 13.9 |
| The best *wrong* typeface, this disc | 18.5 |

A floor near 15 accepts the right answer on both and refuses all nine wrong ones here. One disc is
one data point and the number should not be fixed from it, but the bracket is real and it is the
first evidence that a floor can exist at all rather than being a hope in an acceptance criterion.

### And a constraint on where fitting happens

The film opens with a 43-cue italic phone message, and the two Arial variants are mirror images of
each other:

| reference set | upright | italic |
| :--- | ---: | ---: |
| arial | **7.1%** | 35.7% |
| ariali | 40.5% | **10.8%** |
| arialbd | 35.6% | 48.1% |

Each reads its own style in single digits and the other five times worse. Weight costs nearly as
much as slope. That is [#14][issue-14]'s question — whether typographic variants need their own
reference vectors — answered on real material: **they do**, and one upright vector per character
cannot carry a track that changes style mid-film.

It also lands on #43 rather than only on #14. Whole-track mean distance **cannot see this**:
Arial-italic scores 35.2 because 95% of the track is upright, so a per-title fit would pick the
right set, report a good number, and still read the italic act at 36%. Whatever #43 decides about
caching scope — per library, per title — style is a level below the one it is currently considering.

## Can a fitter tell when it has no good answer?

#43 asks for the negative case **first**, and it is right to: a fitter that always names its argmin
is a machine for producing confident wrong answers, which is the failure this project exists not to
have. So before building any of it:

```console
$ cargo run -p xtask -- fit-select --repeat 8 arial.ttf verdana.ttf tahoma.ttf \
      trebuc.ttf segoeui.ttf calibri.ttf georgia.ttf times.ttf
```

Leave-one-out over eight typefaces. Each in turn renders the fixture, every candidate is scored
against it, and the run is reported twice — once with the material's own font in the list, once with
it withheld. Eight fixtures rather than the one this document's first table rests on, and the true
font withheld, which is the second thing #43's acceptance criteria ask for.

Two statistics, because the obvious one has a hole. **Mean match distance** — what `--report` prints
as `fit` — averages over the glyphs that *matched*, so a set recognising a tenth of a track at close
range outscores one recognising all of it at medium range. **Charging the unmatched** fixes that by
charging every unread glyph the match ceiling, which is a lower bound on what it really cost.

### Choosing works, and gets better with more material

| material | best available | argmin picks | it costs |
| :--- | :--- | :--- | ---: |
| arial | arial (11.4%) | arial | +0.0 |
| verdana | verdana (2.0%) | verdana | +0.0 |
| tahoma | verdana (2.3%) | tahoma | +0.3 |
| trebuc | trebuc (2.6%) | trebuc | +0.0 |
| segoeui | segoeui (16.9%) | segoeui | +0.0 |
| calibri | verdana (10.1%) | calibri | +1.4 |
| georgia | georgia (8.2%) | georgia | +0.0 |
| times | times (14.0%) | times | +0.0 |

Costs nothing on six of eight, **0.2 points on average and 1.4 at worst**. At the shorter fixture it
was 0.7 and 3.4, so the selector improves with material — as a noisy mean should.

Note what the middle column already shows: the best available set is not always the material's own
font. Verdana reads Tahoma-rendered material better than Tahoma's own set does. They are siblings by
the same designer, and it matters below.

### The floor cannot be built on either statistic

Not "not yet", and not a matter of picking a better number. Line up what each material's argmin
scored against what it actually read:

| material | argmin, cells | its CER |
| :--- | ---: | ---: |
| calibri | **14.7** | **11.5%** |
| arial | 15.3 | 11.4% |
| trebuc | **16.1** | **2.6%** |
| tahoma | 16.7 | 2.5% |
| verdana | 17.2 | 2.0% |
| segoeui | 18.2 | 16.9% |
| times | 20.3 | 14.0% |
| georgia | 20.4 | 8.2% |

**Calibri is the closest fit by distance and reads four times worse than Trebuchet, which sits 1.4
cells further away.** The column is not merely noisy — over this range it is close to
uninformative. So every floor either ships everything or refuses the cleanest extraction in the set:

| floor | shipped | worst shipped | refused | **best refused** |
| ---: | ---: | ---: | ---: | ---: |
| 60 permille = 15 cells | 1 | 11.5% | 7 | **2.0%** |
| 70 permille = 17 cells | 4 | 11.5% | 4 | **2.0%** |
| 80 permille = 20 cells | 6 | 16.9% | 2 | 8.2% |
| 90 permille = 23 cells | 8 | 16.9% | 0 | — |

The last column is the cost, and it is the wrong way round throughout: the *first* thing every
usable floor throws away is the 2.0% read. Charging the unmatched glyphs shifts every number up by
about two cells and changes nothing about the ordering.

### The margin does not work either

The matcher already has a threshold for "did the winner really win" — `ambiguity_margin`, which
exists because a winner barely beating its runner-up has not won. The same shape, applied to
candidate sets rather than to characters:

| | smallest gap, answer present | largest gap, answer withheld |
| :--- | ---: | ---: |
| mean match distance | 0.9 cells | 4.6 cells |
| charging unmatched | 0.9 cells | 6.4 cells |

They overlap by four to six cells, in the wrong direction. Withhold Verdana and Tahoma stands out by
a wide margin while being the wrong answer; keep Segoe UI and it wins its own material by 1.2 cells.
Sibling typefaces produce decisive-looking wins that mean nothing, and distinctive ones produce
narrow wins that are correct.

### Why more material does not rescue it

The obvious objection is sample size, so the run above is at eight renderings of every cue.
Selection improved. **Separation did not**, and it should not be expected to, because this is not a
sampling problem: §3 of this document already named the mechanism. A systematically wrong reference
set is *by construction* a low-distance one — the matcher chose `I` for `t` precisely because they
were close — so the statistic that would detect a wrong set is suppressed by the very thing that
makes it wrong. Eight times the glyphs measures the same suppressed signal eight times more
precisely.

### What this leaves of #43

The issue splits cleanly, with opposite answers.

- **Choosing among candidates: works.** Argmin by distance costs 0.2 points on average across eight
  fixtures, and picked the winner out of ten on a real Blu-ray.
- **Knowing whether the choice is any good: does not.** Neither statistic, at either fixture length,
  under an absolute floor or a relative margin.

And #43's framing of the negative case needs adjusting before it can be satisfied. It asks that
material rendered in a font *absent from the candidate list* be refused. The Tahoma row says that is
the wrong test: withhold Tahoma and the argmin picks Verdana, which reads that material at 1.7%.
Refusing it would throw away a clean extraction to satisfy a definition. **What a floor has to
detect is a bad read, not an absent typeface** — and what is measured above is that mean distance
cannot detect one.

What has not been tried, and is the cheapest next thing: **agreement between the top candidates'
output**. Two reference sets that produce the same text are unlikely to be wrong in the same way,
whereas a set with no real answer should disagree with its runner-up everywhere. That is a
comparison between two extractions rather than a threshold on one, it costs a second scan of glyphs
already segmented, and nothing measured so far bears on it either way.

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
- **The selector works, on a disc and across eight fixtures.** Mean match distance picks the right
  typeface out of ten candidates on real material, and leave-one-out prices its choice at 0.2 points
  of CER on average, 1.4 at worst. #43's *choose the set* half is answered.
- **The floor that half depends on cannot be built on that statistic.** Measured before anything was
  built, which is the order #43 asks for. Calibri fits closest by distance and reads four times
  worse than a candidate 1.4 cells further away; neither an absolute floor nor a
  winner-versus-runner-up margin separates a good read from a bad one, at either fixture length. The
  gap under the real disc's argmin — 11.7 against 18.5 — turns out to be one disc's luck rather than
  a property of the statistic.
- **And the negative case needs restating.** #43 asks that material whose typeface is absent from
  the candidate list be refused. Withhold Tahoma and the argmin picks Verdana, which reads that
  material at 1.7%: refusing it would throw away a clean extraction. What a floor has to detect is a
  bad *read*, not an absent typeface.
- **Style is a level below typeface.** An italic reference set reads italic cues at 10.8% and
  upright at 40.5%; the upright set does the reverse. Whole-track distance cannot see it, so a fit
  that is right about the typeface can still be wrong about a third of a reel. [#14][issue-14] is
  not a separate mechanism from #43, it is a level of the same one.
- **`reference-fit` is the instrument for all of it.** Any future claim that some set is good enough
  to embed should arrive as a row in this table. `xtask srt-score` is the equivalent for real
  material, where the only available reference is another release.

[`reference::embedded`]: ../crates/subtrackt-glyph/src/reference.rs
