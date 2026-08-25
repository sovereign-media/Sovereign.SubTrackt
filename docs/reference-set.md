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

A floor near 15 accepts the right answer on both and refuses all nine wrong ones here.

**And that inference was wrong, which is worth leaving in place rather than deleting.** The three
numbers above are real and reproducible; what does not follow from them is that a floor exists. One
disc gives one bracket, and a bracket drawn around a single sample says nothing about where the next
sample lands. Leave-one-out over eight typefaces — the next section but one — puts a *good* read at
16.1 cells and a *bad* one at 14.7, and no floor survives that. The gap here was this disc's luck.

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

## Fitted against deliberately wrong, end to end

The comparison #62 asks for, run through `subtrackt fit` itself rather than by picking the answer by
hand. Same disc, same pipeline, post-correction on; the only variable is which set was used.

```console
$ subtrackt fit '10 Cloverfield Lane (2016).mkv' --references ./sets
  arial-ri                     12.5      96.5%
  arial                        13.6      95.6%
  tahoma                       20.8      93.0%
```

| reference set | read | upright | italic | **all** |
| :--- | ---: | ---: | ---: | ---: |
| **what `fit` chose** (arial + italic) | 96.2% | 5.5% | 4.4% | **5.5%** |
| chosen to be wrong (times) | 73.4% | 61.1% | 68.7% | **61.6%** |

**Eleven-fold.** That gap is the entire argument for fitting, and it is why the tool ships with
nothing embedded: a set that is merely *plausible* lands somewhere in between and says nothing about
which.

Worth noting what the fitter did without being told: the candidate directory held both `arial` and
`arial-ri`, and it preferred the combined set — 12.5 against 13.6 — because reading the italic act
properly is worth a cell of mean distance across the track. The style work of #66 pays here without
anything in `fit` knowing that styles exist.

## Carrying two cuts of a typeface in one set

#66. The disc above reads its upright dialogue at 7.1% and its 43-cue italic act at 35.7%; an
Arial Italic set reads them at 40.5% and 10.8%. A per-title fit picks Arial, reports a good score,
and reads the italic act at 36% — and whole-track distance cannot see the split, because 95% of the
track is upright.

`ReferenceEntry` has carried a `Style` byte since the format was written and nothing had ever
populated it. `gen-reference --italic` does now: one vector per character per cut, in one set, and
the matcher picks whichever is closer on shape alone. No style detection — the letterform decides,
which is what the whole matcher rests on.

The [prediction is on the issue][66-prediction], and it was wrong in the direction that mattered.

[66-prediction]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/66#issuecomment-5379747643

### What it costs, from `xtask set-pairs`

| set | entries | confusable pairs | across styles | same letter, two cuts |
| :--- | ---: | ---: | ---: | ---: |
| arial | 139 | 21 | 0 | 0 |
| arial + italic | 278 | 43 | **0** | 10 |
| arial + italic + bold | 417 | 57 | 2 | 22 |

**Adding italic creates no cross-style confusion at all.** The prediction was that shearing would
push an italic `l` into an upright `/`; it does not, because shearing moves the *whole* italic
alphabet together, so it stays in its own neighbourhood. The 22 pairs italic adds are the italic
set's own copy of the accent-direction problem #48 measured in the upright one — `À`/`Á`, `è`/`é` —
not new kinds of confusion.

The ten same-character pairs are an upright `a` beside an italic `a`. They sit within the margin by
construction and are **not** confusions: since #68 the matcher takes its runner-up from a different
character, so it will not report one as the other's rival. They are counted separately rather than
filtered by distance, because filtering by distance is exactly what would hide them.

### What it buys, on the disc

| set | upright | italic | all |
| :--- | ---: | ---: | ---: |
| arial | 7.1% | 35.7% | 8.8% |
| **arial + italic** | **7.1%** | **4.7%** | **6.9%** |
| arial + italic + bold | 7.3% | 4.7% | 7.2% |

The italic act goes from 35.7% to **4.7%** and the upright column does not move by a decimal. It
also beats the italic-only set's 10.8% on its own cues, because a combined set covers what either
cut alone misses.

**Bold costs and buys nothing here**: 0.2 points on the upright column, ambiguous glyphs up from
3,872 to 4,532, and no gain — the film has no bold. That is one film, so it is an argument for not
adding bold *by default* rather than for never adding it. A track with bold in it would want the
same measurement run again.

### That track turned up, and it is expensive

[#209][issue-209] widened the competitor corpus to 24 titles and found one: **Excision (2012) is set
in Arial Bold throughout.** Read with the regular-plus-italic set this section recommends, it scores
**24.8% CER with 2,666 unreadable glyphs** — more than every other title in that corpus combined, and
the worst result this project has published. `fit` given a directory containing the bold cut ranks
`arialbd` at a mean distance of **11.5** against **30.5** for the regular cut, with 99.6% of glyphs
read against 90.1%, and the title reads **11.4% with 52** unreadable glyphs.

So the caveat above was the right shape and the wrong size. Bold is not a rounding error a title
either has or does not have in passing; it is a cut a whole film can be authored in, and a set
without it fails that film outright rather than degrading. In the competitor comparison it is the
entire difference between winning the accuracy column and losing it.

It does not change what ships *by default* — 0.2 points and 660 extra ambiguous glyphs is still the
wrong trade for the 23 titles that have no bold. It changes what a **candidate directory** should
contain, which is a different question: `fit` costs 2.2 CPU-seconds to scan 128 candidates instead of
6 and 2 MB of peak RSS, so there is no reason for a cut to be missing from the pool it chooses from.

[issue-209]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/209

### What ships

`gen-reference --italic <font>` and `--bold <font>`. **Generate regular plus italic for a single
set; put every cut you have into the directory `fit` chooses from.** A single set carries the trade
above; a candidate pool carries none of it, because only the winner is used. Nothing changes in the matcher, the format or the fitting — the
combined set is an ordinary `.subtref` and `subtrackt fit` scores it like any other.

Cap height is measured per cut rather than once, because an italic and an upright of one typeface do
not share one exactly, and scaling one against the other's would make every line metric slightly
wrong in a way nothing would report.

## Choosing from a real font directory, which is where the statistic matters

Everything above uses eight typefaces chosen to span the design space. #62 has to decide where a
fitter's candidates come from, and enumerating installed fonts is the only option that works with no
setup. This machine has **128** of them.

```console
$ cargo run -p xtask -- fit-select --pool C:/Windows/Fonts --repeat 4 arial.ttf verdana.ttf ...
```

Ten materials, 128 candidates each, scored by what following the ranking costs in CER against the
best candidate available:

| statistic | the argmin alone | best of top 3 |
| :--- | ---: | ---: |
| mean match distance | **15.3 mean, 74.8 worst** | 0.2, 1.8 |
| charging unmatched | **0.7 mean, 2.4 worst** | 0.2, 1.8 |

**Mean match distance collapses at this scale, and the extraction report prints it as `fit`.**
`SegoeIcons` — a symbol face with almost no Latin coverage — wins Georgia-rendered material and
reads it at **79.8%**, and wins Comic Sans material and reads it at 79.2%. It wins because a mean
taken over the glyphs that *matched* rewards a set that recognises a tenth of the track at close
range over one that recognises all of it at medium range. Eight hand-picked text faces never
contained such a candidate; a font directory does.

Charging every unmatched glyph the match ceiling removes it: rank 1 on eight of ten materials, and
0.7 points of CER on average. The two residual cases are not failures of consequence — on Calibri
material `SansSerifCollection` reads 2.4 points better than Calibri's own set, and on Segoe UI
material `ebrima` reads 2.2 points better than the argmin's pick. In both the argmin chose the
material's own font and something else happened to read slightly better.

### What it costs

| | |
| :--- | ---: |
| Generating 128 reference sets | 2.7s, 21ms each |
| Scanning one material against all 128 | 1.0s |

The sets do not depend on the material, so a fitter generates them once and pays only the scan per
title. And the scan here is a full re-extraction of the fixture per candidate, which is the
harness's convenience: a fitter would decode once and rescan the glyphs it already has.

### What this settles for #62

- **The candidate list can be an installed font directory.** 128 candidates cost seconds and the
  ranking survives them.
- **The selection statistic is not the one the tool reports.** `fit` in `--report` is mean match
  distance, and it is the wrong number for this job by 74 points of CER in the worst case here.
- **A top-3 shortlist is a cheap safety net rather than a necessity.** Under the charged statistic
  the argmin is already within 2.4 points; the shortlist gains 0.5 of that. It is still worth
  showing, because #63 says nothing checks the answer and a user looking at three scores can see how
  close the race was.

## Asking whether anything corroborates the winner

The idea left standing when the floor and the margin both failed: stop asking how good the winner's
score is, and ask whether a second reference set agrees with it. Two sets producing the same text
are, the argument goes, unlikely to be wrong in the same way. A comparison between two extractions
rather than a threshold on one, and the same run produces both.

The [prediction is on the issue][43-prediction], and it was not optimistic. If the winner reads at
2% and the runner-up at 15%, they disagree on about 13% — so disagreement largely measures the
*runner-up's* error, and high agreement means the two typefaces resemble each other rather than that
either is right.

[43-prediction]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/43#issuecomment-5377936463

### At track level it is worse than uninformative

| material | winner | runner-up | agreement | winner CER |
| :--- | :--- | :--- | ---: | ---: |
| tahoma | tahoma | verdana | 99.6% | **2.5%** |
| verdana | verdana | tahoma | 96.9% | **2.0%** |
| calibri | calibri | segoeui | **93.0%** | 11.5% |
| segoeui | segoeui | calibri | **92.9%** | 16.9% |
| trebuc | trebuc | tahoma | **79.2%** | **2.6%** |
| georgia | georgia | times | 78.2% | 8.2% |
| arial | arial | calibri | 74.9% | 11.4% |
| times | times | georgia | 72.9% | 14.0% |

Trebuchet reads at 2.6% and agrees 79.2%. Calibri reads at 11.5% and agrees 93.0%. The reads under
5% agree at least 79.2%; the reads over 5% agree at most 93.0% — **overlapping by 13.8 points**, in
the wrong direction. Verdana and Tahoma agree at 99.6% because they are siblings by the same
designer, which is a fact about the type foundry.

### Per character it has signal, and the signal points the wrong way where it matters

Line-level agreement is the wrong granularity: three reference sets are almost never byte-identical
over a whole line, so it flags **701 of 704** lines and measures line length. Aligning the texts and
asking per character is closer to what a fitter would really have — one segmentation, several sets,
N answers per *glyph*, aligned by construction.

| material | flagged | wrong given flagged | wrong given agreed | lift |
| :--- | ---: | ---: | ---: | ---: |
| **arial** | 32% | **3%** | **15.7%** | **0×** |
| verdana | 22% | 4% | 1.6% | 2× |
| tahoma | 13% | 9% | 1.6% | 6× |
| trebuc | 29% | 6% | 1.5% | 4× |
| segoeui | 25% | 40% | 8.6% | 5× |
| calibri | 17% | 42% | 5.3% | 8× |
| georgia | 46% | 10% | 4.2% | 2× |
| times | 45% | 18% | 8.9% | 2× |

Overall it flags 29% of characters; the flagged are wrong 15% of the time and the unflagged 5.7%.
A 2.6× lift, and a character three sets agree on is still wrong once in eighteen.

**And on Arial it inverts.** Characters the committee agreed on are wrong *five times more often*
than the ones it flagged. Arial is the typeface #8 fitted the library to, so that is the row that
matters most.

### Why, in one line of diagnostic

What the committee agreed on and got wrong anyway:

```
    arial        'I' x108  '<?>' x86  'i' x2
    segoeui      '<?>' x112  '`' x4  ' ' x3
    times        '<?>' x85  ':' x2  'û' x1
```

108 characters where all three sets said `I` and the truth was `l`. That is #12's pair, and *every*
sans-serif resolves it the same way because the letterforms are identical — the correct reference
set makes the error too. The `<?>` runs are the unmatched punctuation every set fails on alike.

So the failure is structural, and it is the same shape as the one §3 named for mean distance:

> a systematically wrong set is *by construction* a low-distance one

becomes

> a systematically shared confusion is *by construction* an agreed one.

Both times, the thing that makes the answer wrong is the thing that makes the evidence look right.
Corroboration between candidates cannot see an error every candidate makes, and those are exactly
the errors that survive to be the residual.

### What is left to try

Nothing in the direction of scoring the winner. Three statistics have now failed — mean distance,
the winner-versus-runner-up margin, and inter-candidate agreement — and the third failed for a
reason that would apply to any fourth built out of the candidates themselves.

What that leaves is evidence from **outside** the candidate set: the material's own repetition, a
language model, or a human. The first is the only one that fits this project's constraints, and it
is not obviously enough. #43 should not ship a floor until something does.

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

- **Arial is Monotype's.** `subtrackt gen-reference` reads a font the user already has and writes
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
- **Corroboration between candidates fails too, and for a reason that generalises.** Agreement
  between the top two overlaps by 13.8 points; per character it lifts 2.6x overall and *inverts* on
  Arial, where the committee agrees on `I` for `l` 108 times because every sans-serif makes that
  error. A shared confusion is an agreed one by construction, so no statistic built out of the
  candidates themselves can see the errors that survive. Evidence has to come from outside the
  candidate set.
- **Style is a level below typeface.** An italic reference set reads italic cues at 10.8% and
  upright at 40.5%; the upright set does the reverse. Whole-track distance cannot see it, so a fit
  that is right about the typeface can still be wrong about a third of a reel. [#14][issue-14] is
  not a separate mechanism from #43, it is a level of the same one.
- **`reference-fit` is the instrument for all of it.** Any future claim that some set is good enough
  to embed should arrive as a row in this table. `xtask srt-score` is the equivalent for real
  material, where the only available reference is another release.

[`reference::embedded`]: ../crates/subtrackt-glyph/src/reference.rs
