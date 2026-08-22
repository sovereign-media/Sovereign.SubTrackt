# Post-correction of ambiguous reads

Answers [#12][issue-12]. It asks for two things — a corrector for the pairs a binarized glyph cannot
separate, and a measurement deciding whether it should be on.

**It works, it is measured, and it ships switched off.** 3.1 points of character error rate on the
ceiling fixture, zero lines made worse — and one generated fixture is not the corpus that should
decide a default which rewrites what a viewer reads.

[issue-12]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/12

## What it does

`subtrackt_text::correct::ContextCorrector`. One rule:

> A character the matcher **could not call**, sitting between two characters it **could**, belongs
> to the same class as those two.

`He11o` reads as a word with two digits wedged inside it, and digits do not appear inside words, so
the two glyphs the matcher had already declined to call become `l`. `2O15` reads as a number with a
letter wedged inside it, and the letter becomes `0`. `jaIapeño` reads as a lowercase word with a
capital in the middle of it, and the capital becomes `l`.

There is no dictionary and no language model. Not because one would not help, but because the rule
above is checkable against the line it fires on, whereas a dictionary is an assertion about a
language that the tool cannot verify and that would guess hardest at exactly the words — names,
places, invented nouns — where a subtitle is least replaceable. [#12] asks for `symspell`; the
dependency rule in `CLAUDE.md` asks that a library dependency justify itself, and a spellchecker
cannot, for a stage whose whole safety argument is that it does not know English.

## What it refuses, and why that is the design

Every guard is structural — a thing the code cannot do, not a threshold that happens to be set
conservatively.

| Refusal | What it prevents |
| :--- | :--- |
| Only a glyph the matcher flagged ambiguous | Overruling a read the matcher was sure of |
| Only inside a confusion set (`0`/`O`/`o`, `1`/`l`/`I`/`\|`) | Rewriting a character into an unrelated one |
| Exactly one character out for one character in | `rn`/`m` and `cl`/`d`, which are insertions in disguise |
| Evidence needed on **both** sides, within the word | `I'm` → `l'm`, `1st` → `lst`, `3D` → `3O` |
| Both sides must agree | `No.1` and `H2O`, where the word says two different things |
| Ambiguous neighbours and placeholders are not evidence | An unread glyph deciding the one beside it |

The last two rows are worth stating plainly. Evidence is only ever taken from characters the matcher
read *clearly*, and corrections only ever land on characters it did not, so the two sets are
disjoint: no substitution can become the evidence for the next one, and there is no order in which
corrections cascade.

### The cost, named

Two-sided evidence means a word-initial ambiguous glyph has nothing to its left and is never
corrected. On the fixture that costs two errors the corrector can see perfectly well:

```
! over the Iazy dog.        want: over the lazy dog.
! FoIIow the yeIIow Iine    want: Follow the yellow line
```

`Iazy` and `Iine` stay wrong. They stay wrong because the evidence for rewriting them is
*identical* to the evidence for rewriting the `I` of `Iowa`, which is in the fixture for exactly
this reason and which comes out intact. A rule that fixed `lazy` would break every proper noun in
the track and leave no trace that it had, which is the failure [#12] is written around.

## Method

```console
$ cargo run -p xtask -- accuracy
```

Generates a PGS fixture and a reference set from one font, extracts it twice — post-correction off,
then on — and scores both against the ground truth the fixture was rendered from. Same font on both
sides, so this is the ceiling case: typeface mismatch is excluded by construction and real material
can only read worse.

Aggregate CER is not enough on its own to decide this, so lines are also scored individually. A
corrector that fixes three characters and invents one has still turned a detectable failure into a
plausible wrong answer once, and an aggregate would hide it.

## Result

| | off | on |
| :--- | ---: | ---: |
| Character error rate | 15.9% | **12.8%** |
| Word error rate | 56.8% | **51.4%** |
| Lines improved | — | 2 |
| **Lines made worse** | — | **0** |

Six substitutions, from 35 glyphs the matcher declined to call. Every one of them:

```
cue 3 line 0 col  9: 'I' -> 'l' in "na<?>l<?>ve,"
cue 3 line 0 col 17: 'I' -> 'l' in "jalapeño"
cue 5 line 0 col  2: 'I' -> 'l' in "Follow"
cue 5 line 0 col  3: 'I' -> 'l' in "Follow"
cue 5 line 0 col 13: 'I' -> 'l' in "yellow"
cue 5 line 0 col 14: 'I' -> 'l' in "yellow"
```

Five are right. The sixth is neither: `naïve` had already been shattered by segmentation into
`na<?>I<?>ve`, and the `I` the corrector rewrote was a fragment of the `ï` rather than a letter at
all. It cost nothing and gained nothing, which is the honest description of a corrector operating
on a line another stage had already broken. Worth remembering when reading the number: **the
context post-correction reads is the output of every stage before it**, so an upstream spacing or
segmentation error arrives here as bad evidence.

That is also why `0123456789 O o I l 1` is untouched despite being full of candidates: the spacing
rule (#40) ran its characters together into `0123456789OoI I 1`, and the merged token gives the
trailing letters digits on one side and nothing on the other. Two-sided evidence declined it. Had the rule been one-sided,
the same upstream bug would have rewritten `O` and `o` into digits and made that line worse.

## Why the default is still off

The measurement is positive and the guards are structural. What is missing is not confidence in the
rule, it is a corpus:

- **One fixture, one font, six substitutions.** That is an existence proof, not a rate. The pairs it
  exercises are the ones [#8] found studios author against, but six events cannot say how often the
  rule fires on real material or what it fires *at*.
- **The ceiling case is the wrong place to see the risk.** Where the matcher is nearly right, the
  corrector's evidence is nearly always sound. Real material moves both.
- **[#15]'s larger corpus is referenced but not checked in.** Until it is, "measured on the test
  corpus" means measured on one generated file.

So `Config::post_correct` stays `false` and `subtrackt extract` defaults to off. Both flags work —
`--post-correct` and `--no-post-correct`, last one on the line wins — so pinning either behaviour
survives the default moving.

**What would flip it:** the same table, produced over real tracks with hand-verified ground truth,
still showing zero lines made worse. Nothing else is needed; the code does not change.

## Auditing a run

A stage allowed to rewrite text has to leave a trace of what it rewrote, and a count is not one.
`3 corrections` cannot be checked by anybody; `'I' -> 'l' in "jalapeño"` can.

```console
$ subtrackt extract synthetic.sup --reference accuracy-fixture.subtref       --on-unmatched placeholder --post-correct --report
reference set: accuracy-fixture (139 glyphs)
6 cues from 6 images (36 packets); glyphs 123 matched / 8 unmatched / 35 ambiguous; cache 100%; corrections 6 (context)
  cue 3 line 0 col 9: 'I' -> 'l' in "na<?>l<?>ve,"
  cue 3 line 0 col 17: 'I' -> 'l' in "jalapeño"
  cue 5 line 0 col 2: 'I' -> 'l' in "Follow"
  ...
```

Every substitution is in `Outcome::corrections` with its cue, line, column, both characters and the
word it landed in, and at `-v` each one is a log line. The summary names the corrector even when it
changed nothing, because "post-correction was off" and "post-correction ran and found nothing" are
different facts about a track and `0` alone would hide the difference.

Two counters are worth watching together: `corrections` can never exceed `ambiguous`, since the
corrector may only touch glyphs the matcher flagged. A ratio anywhere near 1 would mean the refusals
had stopped refusing.

## What follows

- **`rn`/`m` and `cl`/`d` belong to segmentation, not here.** [#12] lists them, but resolving either
  means inventing or destroying a character. They are two components fused or one component split,
  and the place to notice that is where components are grouped.
- **A track's own vocabulary is the next lever, and it needs no dictionary.** A word-initial `Iazy`
  could be corrected on evidence rather than on a guess if `lazy` — or any of the l-words a film
  repeats constantly — occurred elsewhere in the same track in a read that was unambiguous. That is
  evidence from the material, which is the kind this project accepts. It cannot be demonstrated on a
  six-cue fixture, so it waits for [#15]'s corpus alongside the default.
- **Ambiguity is a per-glyph property, not a per-cluster one.** #12 was written expecting #10's
  clustering to make it the latter — one wrong label being wrong for every instance of it, which
  would have made correction both more consequential and easier to audit. Clustering measured worse
  and ships off (`ClusterRules::default()` has a radius of zero), so every distinct shape still
  carries its own decision and the corrector sees each occurrence alone. Cluster-level voting is not
  available and would need #10 revisited first.
- **The confusion table is the blast radius.** `5`/`S`, `8`/`B` and `2`/`Z` are the obvious
  additions and are deliberately absent. Each one wants its own row in the table above before it
  goes in.

[#8]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/8
[#12]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/12
[#15]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/15
