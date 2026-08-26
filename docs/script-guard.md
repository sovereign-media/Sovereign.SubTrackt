# A wrong script and a wrong typeface are the same event

[#218][issue-218]. The Russian stream of Gone Girl was refused before this, and the message is the
whole reason the issue existed:

```
error: track rejected by the threshold gate: 50852 of 61249 glyphs read (83.0%), floor is 90.0%
```

Seven points. What the guard held on was that 17% of Cyrillic happens to fall outside a distance
threshold — not on any knowledge that no glyph in the track is a Latin letter.

This measured whether any statistic over the read could do better, found that **none can, for a
reason**, and shipped a guard that consults the container instead.

```
error: the track declares rus, which is written in Cyrillic, and the reference set arial-ri holds
no Cyrillic character at all: it would read as whatever the set does hold rather than fail
```

18 ms rather than 1.6 s, because it refuses before a packet is decoded.

[issue-218]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/218
[issue-189]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/189
[issue-180]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/180
[issue-63]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/63
[issue-101]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/101

## The lead, and why it died

#218 opened with a real observation. `fit` — the mean match distance `--report` prints — was 27.8 on
the Russian track and 11.8 to 13.5 on the Latin tracks of the same disc. Better than two to one, on
every run, and nothing had ever looked at it.

So it was swept properly. Every one of Gone Girl's 17 usable language streams, extracted against
`arial-ri.subtref` with `--on-unmatched placeholder`. Same disc, same authoring, same typeface, same
resolution: **the language is the only variable.**

| | | fit | glyphs read |
| :--- | :--- | ---: | ---: |
| `eng` | English (untagged, stream 0) | 11.8 | 99.9% |
| `dan` | Danish | 12.0 | 98.2% |
| `est` | Estonian | 12.0 | 100.0% |
| `nor` | Norwegian | 12.4 | 98.8% |
| `may` | Malay | 12.5 | 100.0% |
| `ind` | Indonesian | 12.7 | 100.0% |
| `swe` | Swedish | 12.8 | 99.9% |
| `fin` | Finnish | 13.1 | 100.0% |
| `spa` | Spanish | 13.5 | 99.2% |
| `lit` | Lithuanian | 13.9 | 99.4% |
| `lav` | Latvian | **14.3** | 99.4% |
| | | | |
| `ukr` | Ukrainian | 26.5 | 83.5% |
| `kor` | Korean | 27.8 | 45.6% |
| `rus` | Russian | 27.8 | 83.0% |
| **`vie`** | **Vietnamese** | **31.1** | **81.8%** |
| `chi` | Chinese | 36.0 | 33.5% |
| `tha` | Thai | 37.3 | 44.9% |

The gap is 12.2 cells against a within-Latin spread of 2.5. Prediction 1 said fit would separate
the non-Latin tracks from the Latin ones with no overlap, and on this table it does.

**Except that Vietnamese is Latin.** It sits at 31.1, above every Cyrillic track, because
[`language-coverage.md`](language-coverage.md) found the set holds 26 of the 134 characters
Vietnamese requires. So fit is not measuring script. It is measuring whether the set can spell the
track, which is a different thing that happens to correlate.

## The control that ended it

If fit measures spellability rather than script, then a Latin track read with the wrong *typeface*
should score like a non-Latin track read with the right one. The same English stream, read against
six reference sets generated from typefaces it was not authored in:

| | fit | glyphs read | what comes out |
| :--- | ---: | ---: | :--- |
| Arial (its own) | 11.8 | 99.9% | `"What are you thinking?"` |
| Verdana | **23.1** | 97.0% | `"Wha! are you !hinking?"` |
| Consolas | 25.6 | 89.5% | |
| Comic Sans | 29.8 | 67.9% | |
| Georgia | 34.5 | 77.9% | `''�b�l �re you lb!nk!n9?''` |
| Courier New | 34.8 | 35.1% | |
| Times New Roman | **34.9** | 76.9% | |

**The wrong-typeface band is 23.1 to 34.9. The non-Latin band is 26.5 to 37.3. The second sits
inside the first.**

There is no floor that refuses Russian and admits a Verdana-read English track, and no floor that
refuses a Georgia-read English track and admits Ukrainian. Prediction 2 said this and it holds
conclusively.

The mechanism is one sentence, and it generalises past this proposal:

> **A wrong script and a wrong typeface are the same event to everything downstream — the set
> cannot spell this track — and nothing computed from the read can name which.**

This is the seventh statistic to fail here, and [`fit-confidence.md`](fit-confidence.md) has the
other six. Four broke because a systematically wrong set is by construction a low-distance one. The
fifth broke on the channel: font identity does not survive being rendered at subtitle size and
binarized. The sixth, [#101][issue-101], read the output text under a character-bigram prior and
overlapped at every line. This one breaks on a third thing — the *question* is underdetermined by
the evidence, whatever statistic is computed from it.

### The census does not rescue it either

[`language-coverage.md`](language-coverage.md)'s reader counts characters the declared orthography
cannot spell, and it does separate Russian at 63.78% from the Latin tracks at 0.11% to 1.43%. It is
useless as a guard for the same reason:

| | placeholders | letters the language does not have |
| :--- | ---: | ---: |
| English, Arial | 0.09% | 0.13% |
| **English, Verdana** | **2.56%** | **0.08%** |
| Russian, Arial | 14.81% | 63.78% |

The Verdana read is garbage — `Wha! are you !hinking?` — and the census calls it cleaner than
Swedish, because every character it emits is a legal English letter. An orthographic test cannot see
a confusion that stays inside the alphabet. It measures legality, never correctness, exactly as its
own documentation says.

## What shipped

`Script::of_language` and `Script::of_char` in `subtrackt-core`, `ReferenceSet::spells` in
`subtrackt-glyph`, and one check in the pipeline before the decoder is built. Three things must be
true before it objects, and each is a fact rather than a judgement:

1. the container **declared** a language;
2. that language has a **known** script;
3. the set holds **not one** character of it.

Any of the three failing is a pass. The asymmetry is the design: a wrong refusal costs a caller an
expensive fallback on a track that would have read, and a missed refusal costs what the pipeline
already did — hand the threshold gate a track it will probably catch.

- An **untagged** stream passes. 658 files in the library carry one, and [#180][issue-180] found
  why: the muxer leaves the default untagged, so the untagged stream is usually the English one.
  A guard that refused these would refuse the whole bench.
- An **unrecognised** tag passes, so a new tag is never refused before anyone has looked at it.
- **Serbian and Azerbaijani are absent from the table entirely.** Both are written in either script
  and the surveyed discs carry the Latin cut, so a row saying "Cyrillic" would refuse a readable
  track. A test pins the absence, because otherwise it looks like an omission.
- **One character is enough.** A set with a single Cyrillic letter passes a Russian track. How well
  it would read is the question `fit-confidence.md` has seven failed measurements of, and this
  refuses to ask it.

`--ignore-declared-script` turns it off, for a mistagged stream or a set deliberately fitted to
something the tag does not describe.

## What it refuses, measured

Prediction 3 said it would refuse all the non-Latin Gone Girl tracks and none of the nine bench
tracks. Both halves hold.

| Gone Girl | |
| :--- | :--- |
| stream 0, untagged | read |
| `nor` `spa` `swe` `vie` | read |
| `chi` `kor` `rus` `tha` `ukr` | **refused before decoding** |

The standing bench is unchanged — nine tracks, all read, every figure identical, because a guard
that only refuses cannot move a number on a track it does not refuse.

A second disc and a second codec, The Karate Kid's VOBSUB streams, which is the case the bench does
not cover:

| | |
| :--- | :--- |
| `dut` `fre` `ger` | read |
| `ara` `bul` `gre` `hin` | **refused before decoding** |

**Vietnamese is the honest limit.** It is Latin, so this guard passes it, and the threshold gate
refuses it at 81.8% against a 90% floor. Two gates, two different facts, and widening this one to
cover the second would turn it into the fraction it exists to replace.

## The same evidence at the resolution of one character

[#230][issue-230]. The guard above refuses a whole track when the set holds not one character of the
declared script. It has nothing to say about a track it passes — and #189's reader had already
counted what those tracks produce:

> `102 — 0.13% letters the language does not have`

That line is printed for the English stream of Gone Girl by `xtask language-coverage`, and nothing
consumed it. Over the nine bench extractions, placeholders excluded, the matcher answers with a
character English cannot spell **630 times**: `Í` 331, `ì` 186, `Ì` 39, `Î` 21, and a scattering of
`í`, `è` and `é`. Almost all of it is one letter wearing an accent it cannot have.

So the gate is the same fact at a finer resolution. The container named a language, the language has
a documented orthography, and this letter is not in it — **evidence from outside the read, refusing
on a fact**, which is the standard this document sets and the reason none of the seven statistics in
`docs/fit-confidence.md` could meet it.

### It is a mask on the scan, not a strike on the answer

The difference is the whole design. A masked entry is never scored, so the winner is whatever the
remaining set returns and the runner-up is a real second choice. Striking the character out
afterwards would leave the glyph unread, which is a *worse* answer than the one below it.

The measurement says the same thing. Stripping every impossible accent from the bench output by
hand — the ceiling a strike could reach — is worth **83 cues better and 12 worse**, and the 12 are
genuine `é` in names on two discs. The mask beats that ceiling because it can hand the glyph to a
different base letter, and it keeps the `é` because French and Spanish rows carry it and English's
row is only consulted for an English track.

### Every uncertainty resolves to a pass

The rule this document already states, applied unchanged:

- **No language tag, or a tag the table does not carry** — no mask. All 51 rows are ISO 639-2/B,
  which is what `scripts/language/survey.py` found over 1,316 files; a two-letter tag is a pass.
- **Letters only.** An orthography's claim on a letter is solid — English does not write `Í`, in any
  typeface, on any disc. Its claim on punctuation is not, which is exactly why `TYPOGRAPHY` exists:
  a curly quote fails every language on the discs that draw one, and a musical note is drawn on an
  SDH disc in every language there is.
- **A character of another script is not this gate's business.** The guard above answers that,
  before the read. Two gates that answered the same question in two places would eventually
  disagree about one track.
- **A mask that would refuse every entry is not applied at all.** A matcher that can answer with no
  character leaves a whole track unread, which is a state no orthography implies.
- **It is counted and printed.** `language refuses 169/478 entries` on the report line, because a
  gate that fires silently is a gate nobody can audit — which is what `glyphs_without_metrics` cost
  this project for as long as it existed and was never printed.

### What it is worth

Nine bench extractions, told they are English. **239 cues better and 0 worse**, and every scored
track improves:

| track | CER | better | worse |
| :--- | :--- | ---: | ---: |
| A Fish Called Wanda | 1.3% -> **1.0%** | 71 | 0 |
| Gone Girl | 1.3% -> **1.2%** | 60 | 0 |
| Training Day | 1.9% -> **1.8%** | 37 | 0 |
| King Kong | 6.0% -> **5.9%** | 28 | 0 |
| The Karate Kid | 1.7% -> **1.6%** | 28 | 0 |
| 10 Cloverfield Lane | 0.3% | 15 | 0 |

That clears the criterion [#185][issue-185] used to flip post-correction's default -- a table over
real material with nothing made worse -- so this ships **on**.

**The first pass did not clear it, and the twelve failures are the finding.** Before the loanword
column existed the same measurement read 239 better and **12 worse**, and every one of the twelve
was a real word:

```
Leave your résumé with my secretary.        ->  Leave your rdsumd with my secretary.
Fancy my porridge à la walnuts?             ->  Fancy my porridge ? la walnuts?
- How's your Español? - Más o menos.        ->  - How's your Espahol? - M?s o menos.
You calling me a cheater, ése?              ->  You calling me a cheater, bse?
```

English's row lists **no letters at all**, which is orthographically correct and empirically wrong:
English subtitles carry French and Spanish loanwords, and a gate reading `letters` refuses them. The
two questions are different — *what does this language need* is the census's, *what may this track
draw* is the gate's — and conflating them is what produced the twelve. `Language::loanwords` is the
second question, consulted by `can_spell` and by nothing else, so every figure
`language-coverage.md` has published is untouched.

It costs nothing because it is **lowercase only**. `Í` is 331 of the 630 impossible characters the
bench produces and no English word wants a capital I-acute; admitting the capitals would hand back
over half the gain to buy a word nobody writes.

### And it does nothing where nothing is declared

The same nine tracks, at the shipped default with no `--language`, are **byte-identical** to the
run before this existed — 0 better, 0 worse, on every one. A `.sup` carries no tag, so the mask is
never built.

That is the honest shape of the default: #180 found **21 of 50** titles declaring a language on the
track this pipeline chooses. The 239 cues are what the gate is worth *when it can fire*, and on this
bench it can only fire because the caller says so.

### A `.sup` has no container, so the caller may say

`--language <TAG>` overrides whatever the container declares, and both gates read the same resolved
answer. This document's "Reproducing" section said the guard *needs* the container; that is now a
default rather than a requirement. The override is also the answer for a container that declares the
wrong tag, which #180 found 16 titles of.

[issue-230]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/230
[issue-185]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/185

## What this does not claim

- **It is not a detector.** It compares two declarations and never looks at a bitmap. A stream
  tagged `eng` that is really Greek passes, and always will.
- **It does not make the threshold gate redundant**, and Vietnamese is the proof: the two catch
  different things and neither is a superset.
- **The wrong-typeface control uses deliberately distant typefaces** — Courier, Comic Sans, Georgia.
  [`reference-set.md`](reference-set.md) and [#63][issue-63] establish that the hard cases are the
  near neighbours, and none of those is in the table above. That makes the overlap finding
  *stronger*, not weaker: these are the easy cases and they already overlap.
- **One disc for the sweep, two for the guard.** The right disc for the question — 24 languages, one
  authoring — and still one disc.

## Reproducing

```console
$ cargo run --release -p xtask -- dump-sup "...Gone Girl...mkv" rus.sup --stream 11
$ subtrackt extract rus.sup --reference arial-ri.subtref --on-unmatched placeholder --report
$ cargo run --release -p xtask -- gen-reference C:/Windows/Fonts/verdana.ttf verdana.subtref
$ subtrackt extract eng.sup --reference verdana.subtref --on-unmatched placeholder --report
$ subtrackt extract "...Gone Girl...mkv" --stream 11 --reference arial-ri.subtref   # refused
```

The guard takes the container's word by default, and a `.sup` holds no language tag —
`subtrackt-demux` says so rather than guessing one from the filename. Since #230 a caller may say
instead:

```console
$ subtrackt extract wanda.sup --reference arial-ri.subtref --restrict-to-language --language eng
```
