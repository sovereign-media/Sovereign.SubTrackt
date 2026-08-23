# Post-correction of ambiguous reads

Answers [#12][issue-12]. It asks for two things — a corrector for the pairs a binarized glyph cannot
separate, and a measurement deciding whether it should be on.

**It works, it is measured, and it ships switched off.** 1.9 points of character error rate on the
ceiling fixture, zero lines made worse — and one generated fixture is not the corpus that should
decide a default which rewrites what a viewer reads.

The number moves when the fixture does, and it has four times: #48 added three cues of accented
text, #58 a line carrying `î`, #57 a line of accented capitals, and #60 a cue supplying case-folded
evidence. What has not changed across any of them is that no line was made worse.

**Read the whole of this document as history from #110 onward.** Giving the matcher an ink aspect
ratio took the substitutions this stage makes on a real Blu-ray from 363 to **3**, and on the
ceiling fixture to **none at all** — the corrector no longer moves either instrument, because the
glyphs it existed to rescue are now decided by shape. That is the right direction and it is also a
demotion: the stage is a backstop for what shape genuinely cannot decide, not a lever on the error
rate. The default is unaffected — it was never argued from the size of the gain. "Those 363 are now
3" below has the detail, and [`error-census.md`](error-census.md) has the change that caused it.

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

`Iazy` and `Iine` stay wrong under the context rule alone. They stay wrong because the evidence for
rewriting them is *identical* to the evidence for rewriting the `I` of `Iowa`, which is in the
fixture for exactly this reason and which comes out intact. A rule that fixed `lazy` from context
would break every proper noun in the track and leave no trace that it had.

**A second arm resolves that, and #60 is where it came from.** The two cases are identical *to the
context rule*, and they stop being identical the moment a different kind of evidence is admitted —
see below.

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
| Character error rate | 9.8% | **7.9%** |
| Word error rate | 35.3% | **30.9%** |
| Lines improved | — | 3 |
| **Lines made worse** | — | **0** |

Six substitutions, from 72 glyphs the matcher declined to call. Every one of them:

```
cue 3 line 0 col 15: 'I' -> 'l' in "jalapeño"
cue 5 line 0 col  2: 'I' -> 'l' in "Follow"
cue 5 line 0 col  3: 'I' -> 'l' in "Follow"
cue 5 line 0 col 13: 'I' -> 'l' in "yellow"
cue 5 line 0 col 14: 'I' -> 'l' in "yellow"
cue 6 line 1 col 11: 'I' -> 'l' in "plaît,"
```

**All six are right**, which they were not until recently. A seventh used to sit at the head of that
list: `naïve` arrived from segmentation as `na<?>I<?>ve`, and the `I` the corrector rewrote was a
fragment of the shattered `ï` rather than a letter at all. It cost nothing and gained nothing.

#58 fixed the segmentation — a diaeresis straddles its stem, so neither dot overlapped it and only
their union did — and the spurious substitution went with it. That is the lesson kept rather than
the substitution: **the context post-correction reads is the output of every stage before it**, so
an upstream spacing or segmentation error arrives here as bad evidence, and fixing the upstream
stage is what removes it.

That is also why `0123456789 O o I l 1` is untouched despite being full of candidates: the spacing
rule (#40) ran its characters together into `0123456789OoI I 1`, and the merged token gives the
trailing letters digits on one side and nothing on the other. Two-sided evidence declined it. Had the rule been one-sided,
the same upstream bug would have rewritten `O` and `o` into digits and made that line worse.

## The track's own vocabulary, which is not a dictionary

#60. The context arm needs evidence on *both* sides of a glyph, so it can never fire at a word edge
— and `Iazy` and `Iowa` present it with exactly the same evidence. A different kind of evidence
separates them: **a word the same track already read clearly**.

The finding that shapes it is one line of `glyph.rs`. `is_unambiguous` is
`runner_up_distance - distance >= margin`, and after #37 exactly one zero-distance pair survives in
the reference set: `I`/`l`. So *every* `l` and `I` in a track is flagged ambiguous, always, and no
word containing one is ever read clearly. A same-spelling lookup would return nothing for the one
case it exists for.

**Case folding rescues it.** `l` and `I` are indistinguishable, but `L` and `i` are ordinary
distinct shapes read clearly, and dialogue supplies both forms of a word constantly:

| ambiguous | candidate | clear evidence a track holds |
| :--- | :--- | :--- |
| `Iook` | `look` | `Look` |
| `Iazy` | `lazy` | `Lazy` |
| `It` | it stands | `it` — so the reading itself wins |
| `Iowa` | — | nothing folds onto `iowa`; **refused** |

The `Iowa` refusal rests on the **absence** of evidence rather than on a threshold. Correction needs
positive evidence; silence means leave it alone.

### What it is allowed to do

Every guard the context arm keeps, kept:

| Refusal | What it prevents |
| :--- | :--- |
| Only a glyph the matcher flagged ambiguous | Overruling a read the matcher was sure of |
| Only within a confusion set, one character out for one in | Insertions in disguise |
| Only where the context arm **declined** | The measured behaviour stays a strict subset |
| A token is evidence only if *every* character in it was read clearly | The evidence set and the correction set stay disjoint |
| Exactly one candidate supported | Two is contradictory evidence, and both zero and two are refusals |

### Result

| | CER | WER | lines better | lines worse |
| :--- | ---: | ---: | ---: | ---: |
| off | 9.8% | 35.3% | — | — |
| context | 7.9% | 30.9% | 3 | **0** |
| context + vocabulary | **7.6%** | **29.4%** | 4 | **0** |

On the fixture, one extra substitution: `Iazy` becomes `lazy`, on evidence from the clear `Lazy` the
cue set now carries. Thirteen distinct clear tokens were learned from nine cues.

**And on a real Blu-ray** — 10 Cloverfield Lane, 818 scored cues, the same track everything else in
this repository is measured against:

| | |
| :--- | ---: |
| Substitutions by context | 363 |
| Substitutions by vocabulary | **9** |
| Cues improved | 9 |
| **Cues made worse** | **0** |
| CER | 5.5% → 5.5% |

Nine characters out of 24,522. Every one of them right, every one logged with the word that decided
it — `'I' -> 'l' in "lucky" (vocabulary: "lucky" x1)` — and the aggregate does not move, because
nine characters cannot move it.

**Those 363 are now 3.** Every one of them was `I` → `l`, and #110 gave the matcher a way to tell
the two apart before the corrector ever sees them: an ink aspect ratio on the reference entry, which
Arial draws 7% wider for `I` and the 16-cell grid rounds away. The stage itself is unchanged, and
still off by default; what changed is that almost nothing now reaches it. That is the right
direction — a correction is evidence about a glyph the matcher could not call, and the fewer of
those there are the better — and it is also the answer to what this stage is *for*: not carrying the
pipeline, but catching what shape genuinely cannot decide.

### The sweep, and what it settled

Both thresholds were guesses. On the disc:

| setting | substitutions | better | worse |
| :--- | ---: | ---: | ---: |
| `min_occurrences` 1 | 7 | 7 | 0 |
| `min_occurrences` 2 | 3 | 3 | 0 |
| `min_occurrences` 3 | 3 | 3 | 0 |
| `min_len` 3 | 7 | 7 | 0 |
| **prefix matching** | **9** | **9** | 0 |

`min_occurrences` stays at **1**: raising it costs four correct substitutions and prevents none,
because nothing was made worse at any setting. The guard it offers is against a failure that has not
been observed. `min_len` changes nothing on that track — every token the arm fired on is four
characters or more — and stays at 2 because a one-character token folds onto far too much.

**Prefix matching is on**, and this is the row that decided it. A track says `Looking` more often
than `look`, so exact matching leaves evidence on the table. The obvious reach is a stemmer, and it
is the wrong reach twice over: a lemmatizer carries a lexicon, which is the dictionary objection
wearing a hat, and a stemmer over-collapses by design — `universe` and `university` share a stem.
Prefix matching against the track's own clear tokens gets most of it with no dependency and no
knowledge of any language. It over-matches, and over-matching is harmless here: the substitution
decides one character within a confusion set, not which word the line holds.

### The row is now worth nothing, and that is the finding

#127 found that the CLI had been overriding this default: `--vocab-prefix` was declared as a bare
`bool`, so a plain `bool`'s zero value reached `VocabularyRules` on every run that did not pass the
flag. Every extraction through the binary had prefix matching **off** from #78 to #128, against the
`true` the table above chose.

Re-running the three discs both ways to price what that cost found the answer is **nothing**:

| | upright | italic | all | corrections | output |
| :--- | ---: | ---: | ---: | ---: | :--- |
| 10 Cloverfield Lane | 0.5% | 2.0% | **0.6%** | 3, all context | byte-identical |
| Gone Girl | 2.0% | — | **2.0%** | — | byte-identical |
| A Fish Called Wanda | 3.8% | 13.4% | **4.2%** | — | byte-identical |

Not "within noise" — the two SRTs compare equal on all three titles, and the vocabulary arm makes
zero substitutions on any of them at either setting.

**#110 is why.** The sweep above ran when this stage made 363 corrections on Cloverfield; the ink
aspect ratio took that to 3, and all three are the context arm. The vocabulary arm exists to reach
word-*edge* ambiguity that context cannot, and there is no longer any word-edge ambiguity on these
discs for prefix matching to make a difference to. Nine against seven was a real measurement of a
population that has since been removed.

So the fix is correct and its blast radius is zero, which are two separate facts and both worth
recording. The setting should follow the measurement whatever the measurement is worth — a default
the CLI silently inverts is a bug even in the year it changes nothing, because the thing that made
it harmless is a *different* change that could be revisited. What this does retire is the idea that
the vocabulary arm is a lever: on current material it is a backstop that never fires.

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

`Config::track_vocabulary` stays `false` for the same reasons and one of its own: nine substitutions
on one film is an existence proof, and the two new failure modes it introduces — a proper noun that
case-folds onto a common word, and a single clear occurrence that was itself a misread — are both
unobserved rather than ruled out.

So `Config::post_correct` stays `false` and `subtrackt extract` defaults to off. Both flags work —
`--post-correct` and `--no-post-correct`, last one on the line wins — so pinning either behaviour
survives the default moving.

**What would flip it:** the same table, produced over real tracks with hand-verified ground truth,
still showing zero lines made worse. Nothing else is needed; the code does not change.

## The first two of those three, on a real disc

The first two bullets above are now answered. The third is not, and it is the one holding the
default.

**10 Cloverfield Lane (2016)**, one PGS track, 822 cues, an Arial reference set, scored by
`xtask srt-score` against the English subtitle shipped beside the rip:

| | off | on |
| :--- | ---: | ---: |
| Character error rate | 8.8% | **7.4%** |
| Character error rate, upright cues only | 7.1% | **5.5%** |
| Cues improved | — | 263 |
| **Cues made worse** | — | **0** |

362 corrections across the track. **Not one cue got worse**, over 818 scored cues of material this
project did not render — two orders of magnitude more events than the fixture's six, and the risk
case the second bullet asks for, since a real disc is where the corrector's evidence is least sound.

### Why this still does not flip the default

The comparison subtitle is **not hand-verified ground truth**. It is another release's transcript of
the same dialogue, and release subtitles are frequently themselves read off the same bitmaps by some
other tool. So a systematic error the corrector introduced could in principle be matched by the same
systematic error in the comparison, and score as agreement.

That hole is narrower than it sounds — the corrector only ever rewrites within `0`/`O`/`o` and
`1`/`l`/`I`/`|`, so agreeing wrongly across 263 cues would need the comparison to have made the same
confusions in the same places — but narrower is not closed, and it is not what the criterion asks
for. What is still missing is one track whose ground truth a person checked.

It is also one film, in English, in one typeface. `Config::post_correct` stays `false`.

## Auditing a run

A stage allowed to rewrite text has to leave a trace of what it rewrote, and a count is not one.
`3 corrections` cannot be checked by anybody; `'I' -> 'l' in "jalapeño"` can.

```console
$ subtrackt extract synthetic.sup --reference accuracy-fixture.subtref       --on-unmatched placeholder --post-correct --report
reference set: accuracy-fixture (139 glyphs)
10 cues from 10 images (60 packets); glyphs 248 matched / 15 unmatched / 72 ambiguous (94.3% read); fit 13.0; cache 100%; corrections 6 (context)
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
- ~~**A track's own vocabulary is the next lever**~~ — **built and measured**, see below. It fires,
  it never damaged a cue, and on a feature film it corrected nine characters out of twenty-four
  thousand. A real result and a small one.
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
