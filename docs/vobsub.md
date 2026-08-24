# What VOBSUB actually reads at

Answers [#140]. Until this, **every accuracy figure this project had published was PGS**: the bench
built in [#133] was seven PGS tracks, `docs/library-accuracy.md`'s corpus is PGS, and the ceiling
fixture is PGS by construction. The VOBSUB decoder shipped in [#3], `subtrackt list` offers those
tracks, `extract` reads them and produces text — and nothing anywhere said whether that text was
right.

Fourteen VOBSUB titles were measured. The short answer is that **VOBSUB reads about as well as PGS
when the reference set fits, and the accuracy gate catches the cases where it does not** — which is
a less alarming answer than the issue expected, and it took two false starts to get to.

## What was measured

| title | read | fit | CER | WER | sidecar |
| :--- | ---: | ---: | ---: | ---: | :--- |
| The Karate Kid | 99.8% | 16.1 | **2.5%** | 9.8% | plain |
| Training Day | 99.7% | 15.1 | **3.1%** | 11.7% | SDH, same disc |
| The Mission | 99.6% | 11.5 | 8.8% | 20.5% | plain |
| Collateral | 99.8% | 16.0 | 15.4% | 24.7% | plain |
| Set It Off | 99.8% | 12.0 | 43.0% | 57.8% | plain, **track is SDH** |
| Sleepy Hollow | 99.8% | 15.8 | 45.9% | 58.7% | mismatched SDH convention |
| Natural Born Killers | 99.4% | 11.8 | 62.2% | 83.2% | plain, **track is SDH** |
| The Patriot | 79.5% | 31.8 | — | — | *refused by the gate* |
| The American Astronaut | 56.4% | 42.1 | — | — | *refused by the gate* |
| Diamond Men | 63.4% | 38.6 | — | — | *refused by the gate* |
| The Omen | 25.6% | 35.8 | — | — | *refused by the gate* |

Three more carried no untagged or `eng` VOBSUB track at all and were dropped.

## The gate works

The tracks split cleanly in two, and **fit is what separates them**:

- Seven tracks read 99.4–99.8% of their glyphs at a fit of 11.5–16.1.
- Four read 25.6–79.5% at a fit of 31.8–42.1, which is `docs/reference-set.md`'s *the typeface does
  not fit* band, and the 0.90 floor refuses every one.

So the floor cut from PGS coverage figures transfers. Nothing in this sample slipped through it, and
nothing was refused that should have passed.

## Coverage does not grade a read, and neither does fit

Among the seven that pass, CER runs from 2.5% to 62.2% at indistinguishable coverage. That looks
like the confident-wrong-answer failure this project exists to avoid — and it is not. **Every badly
scoring track in that group is a sidecar mismatch, not a bad read.**

- **Natural Born Killers** and **Set It Off** are SDH tracks — 304 and 362 speaker labels — and
  neither disc has an SDH sidecar. Every label is a whole line of character error against a
  reference that does not carry it.
- **Sleepy Hollow** is SDH in a different spelling: its sound cues are `(GHOSTLY NEIGHING)` in
  capitals, and the release writes `(ominous music playing)` in lower case. Same information,
  no characters in common.

`CLAUDE.md` already carries this rule, from The Prestige: *a scored track needs a sidecar of matching
convention, not just any sidecar.* This puts a number on it.

## What choosing the wrong sidecar costs

Scoring one extraction against every English sidecar its own folder holds:

| title | best | worst | spread |
| :--- | ---: | ---: | ---: |
| The Karate Kid | 2.5% | 66.4% | 63.9 |
| Training Day | 3.1% | 80.7% | 77.6 |
| Natural Born Killers | 62.2% | 89.5% | 27.3 |
| Collateral | 15.4% | 27.4% | 12.0 |
| The Omen | 77.0% | 88.0% | 11.0 |

Training Day is the sharpest of them, and it is not the SDH-versus-plain axis. All four of its
sidecars are from one disc; the two cut from the Blu-ray score **3.1% and 5.0%**, and the two cut
from the WEB release score **79.7% and 80.7%**. Same film, same convention, different master.

**A CER quoted without naming its sidecar means nothing.** That is worth more than any single figure
in the table above.

## What is actually wrong with the reads

The one substantive error mode, from the per-character census on Natural Born Killers:

| substitution | count |
| :--- | ---: |
| `l` → `I` | **510** |
| `i` → `I` | 182 |
| everything else | ≤ 65 each |

`You'll be late` reads `You'II be Iate`. [#109] added the ink-width term precisely to separate `l`
from `I` — measured on a real Blu-ray at two glyphs in 867 — and on this material it is not holding.
The mechanism is open: VOBSUB is four-colour and anti-aliased differently, so the ink a glyph
presents is not the ink the term was tuned against. That is its own measurement and its own issue.

## What the issue expected and did not find

**These are not DVD-era renderings.** [#140] reasoned that "DVD-era subtitles are a different era, a
different authoring pipeline and a lower resolution", and that Arial might therefore be the wrong
reference. In this library they are not: every sampled VOBSUB track reports a 1920-wide plane, and
its glyphs are the same size as PGS glyphs —

| | p10 | median | p90 | max |
| :--- | ---: | ---: | ---: | ---: |
| The Mission, VOBSUB | 32 | **34** | 45 | 57 |
| 10 Cloverfield Lane, PGS | 31 | **33** | 43 | 54 |

These are Blu-ray remuxes that happen to carry VOBSUB, not DVD rips. Whatever is true of genuine
720×480 material is still unmeasured, because this library does not appear to contain any.

**The high distinct-shape ratio is real but does not cost what was assumed.** [#140] measured VOBSUB
at a median 15.4% distinct shapes per glyph against PGS's 0.73%, and expected the session cache to
collapse. It does not: cache hit rates here are 96–100%.

## Two tracks join the bench

`karate-kid` and `training-day`, both `scored`. They are the two with a matching-convention sidecar
and a read good enough to have a signal — 2.5% and 3.1% — and between them they cover plain and SDH.

They cannot use the dump cache: a `.sup` holds PGS and nothing else, so `score` reads them from their
containers. That takes a pass from about five seconds to **fifty-four**, which is still cheap enough
to run before *and* after a change, and `run.py dump` now says so rather than retrying three times
and reporting a failure.

## What is still not known

- **Genuine DVD-resolution VOBSUB.** Not in this library, so not measured. Everything above is about
  HD-authored material.
- **Whether the `l` → `I` rate is a threshold or a mechanism.** 510 substitutions on one track is
  large enough to be worth its own sweep.
- **Whether the four refused tracks are a typeface problem or a decode problem.** Fit above 30 says
  the reference does not match; it does not say whether the ink is wrong or the letterforms are.

[#3]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/3
[#109]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/109
[#133]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/133
[#140]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/140
