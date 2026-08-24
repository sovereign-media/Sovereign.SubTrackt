# Post-correction of ambiguous reads

Answers [#12][issue-12]. It asks for two things — a corrector for the pairs a binarized glyph cannot
separate, and a measurement deciding whether it should be on.

**It works, it is measured, and it ships switched off.** 1.9 points of character error rate on the
ceiling fixture, zero lines made worse — and one generated fixture is not the corpus that should
decide a default which rewrites what a viewer reads.

The number moves when the fixture does, and it has four times: #48 added three cues of accented
text, #58 a line carrying `î`, #57 a line of accented capitals, and #60 a cue supplying case-folded
evidence. What has not changed across any of them is that no line was made worse.

**"No line was made worse" expired once, and is true again.** The claim below was measured before
#110 changed which glyphs reach this stage at all. When the bench of #133 re-ran it, the context arm
was turning a correct `All-State` into `AII-State` on 10 Cloverfield Lane: `look` stopped its
outward scan only at whitespace, so it stepped over the hyphen exactly as it steps over an ambiguous
glyph, saw `A` on one side and reached `State`'s `S` on the other, and agreed. **#139** separated the
two: an ambiguous neighbour is *unknown* and is stepped over; a confidently-read hyphen is a *word
boundary* and stops the scan. A word carries one case, and `All-State` is two words.

Re-measured on all seven tracks after that fix — **8 cues better, 0 worse**, and the only movement
is A Fish Called Wanda at 4.2% to 4.1%:

| track | CER | better | worse |
| :--- | :--- | ---: | ---: |
| 10 Cloverfield Lane | 0.6% → 0.6% | 0 | **0** |
| Gone Girl | 1.9% → 1.9% | 0 | **0** |
| A Fish Called Wanda | 4.2% → **4.1%** | 8 | **0** |
| King Kong | 21.5% → 21.5% | 3 | **0** |
| Airplane! | 41.8% → 41.8% | 0 | **0** |

The lesson is not that the rule was wrong. It is that **a result measured against one stage survives
only as long as that stage does**, and nothing re-ran this one for the two issues between #110 and
#133 that changed what reaches it.

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

## The one-character word

#171 asked which lever was left for `l`/`I`, the largest confusion family this project has, and
supposed it was the ambiguity flag: *the corrector can only reach glyphs the matcher was unsure
about, and the worst failures are the ones it was sure about*. **That premise is wrong, and finding
out is most of what this section is.**

### What the errors actually are

On A Fish Called Wanda the extraction holds **343 lone lowercase `l`s and three lone `I`s**, against
353 in the sidecar. On Gone Girl it is 126 against 564. The arithmetic closes on both: essentially
every standalone `I` on the disc is accounted for, and the ones that are wrong are wrong at exactly
one position. **Four fifths of the family is the English pronoun `I`.**

That position is unreachable by either existing arm, and not because of a flag:

- **the context arm** needs a character on each side, and a one-character word has neither;
- **the vocabulary arm** needs the track to have read the same token *clearly* somewhere. After #37
  exactly one zero-distance pair survives in the reference set, so every `l` and `I` in a track is
  ambiguous by construction and **no clear one-character token ever folds onto `i` or `l`**. The
  only candidate left with support is the digit. Running the arm at `--vocab-min-len 1` measures
  that directly: 515 corrections on Gone Girl, **every one of them a correct pronoun rewritten to
  `1`** — `When l think of my wife` became `When 1 think of my wife`. It is exactly the failure the
  `min_len: 2` default was guessing at, and it is now observed rather than guessed.

That experiment also settles #171's premise, from the other side. The vocabulary arm only ever fires
on glyphs the matcher flagged, and it fired on 515 of them — so **these glyphs are already
ambiguous**. The corrector can reach them. What it lacks is evidence, not permission, and widening
the trigger past the flag would buy nothing while giving up the stage's outer guard.

Nor is there evidence in the ink. [`glyph-hit-list.md`](glyph-hit-list.md) measured the pair's width
on all three Arial discs and Wanda draws them identically at p25, p50 and p75 alike. Height offers
nothing either, and that is a property of the typeface rather than of the disc: Arial's ascender and
its cap height are the same to within a unit, so a lone `l` and a capital `I` are **both 42 px tall**
at 1080p on 10 Cloverfield Lane. The aspect ratio the matcher already uses is `5/42` against `6/42`
there, and `5/42` against `5/42` on Wanda.

### What is left is a language, and it was measured rather than asserted

The remaining lever is the assertion that a lone `l` is not a word. This document rules out a
dictionary, on the grounds that it is unverifiable and guesses hardest at names — so a rule resting
on one has to answer that, and the answer is that this one was *measured*:

**Across 77 English release subtitles from 47 titles, a lone lowercase `l` occurs 641 times.** Every
one of them is itself a misread `I`, in transcripts this project did not produce:

```
The Blair Witch Project   287   "-Okay, l got you."
Fantastic Mr. Fox         218   "l told you"
U.S. Marshals              69   "As long as l live, if I never see Gerard"
```

The third line carries both forms in one sentence, which is what a misread looks like and what a
word does not. The apostrophe cases say the same thing: 271 occurrences of `l'` followed by letters,
and the suffix is `m`, `ve`, `ll` or `d` every single time.

So `--lone-words` promotes a one-character word of `l` or `|` to `I`. Three refusals, each against
something observed rather than imagined:

| Refusal | What it prevents |
| :--- | :--- |
| Only `l` and `\|`, never `1` | A lone digit is a legitimate token — `Chapter 1` |
| A lone twin from the same set anywhere on the line | A shattered word, and a line *about* the letters |
| An apostrophe carries at most two letters | `l'` before a longer word is a French elision |

The middle one earned its place twice. `Well,` arrives from segmentation as `We l l ,` on
10 Cloverfield Lane and each half looks exactly like a pronoun — the same upstream-evidence problem
`naïve` and `0123456789 O o I l 1` record above. And the accuracy fixture carries `- Is it 1 or l?`,
a line *about* the characters, whose lone `l` is correct and is the only correct one this project
has ever observed. An earlier draft rewrote it. What refuses both is the same thing: another member
of the confusion set standing alone on the line.

### Result

On the bench, against `--post-correct` rather than against nothing, so the arm is priced on its own:

| track | cues improved | **cues worse** | CER |
| :--- | ---: | ---: | ---: |
| 10 Cloverfield Lane | 3 | **0** | 0.4% → 0.4% |
| Gone Girl | 104 | **0** | 1.5% → **1.3%** |
| A Fish Called Wanda | 269 | **0** | 1.7% → **1.1%** |
| King Kong | 19 | **0** | 21.3% → 21.2% |
| The Karate Kid (VOBSUB) | 0 | **0** | 1.8% → 1.8% |
| Training Day (VOBSUB) | 0 | **0** | 2.2% → 2.2% |
| **total** | **395** | **0** | |

And nothing at all on the ceiling fixture: 0.0% CER before and after, zero substitutions, zero lines
moved. The guard is doing its job on the one line that could have gone wrong.

**The two VOBSUB tracks gain nothing, and that is the result rather than a gap.** They read the
pronoun correctly and fail in the opposite direction — `Iike`, `Iet`, `Is` for `like`, `let`, `is` —
which is word-*initial* `l` read as `I` inside a longer word. That is what the context arm already
reaches: switching the whole stage on is worth **228 and 301 cues** on those two, against 11 across
every PGS track on the bench. The two codecs fail at opposite ends of the same word, and each has an
arm that reaches one of them.

### The stage, priced separately

The table above starts from `--post-correct`, so here is what that costs against the shipped default
of nothing:

| track | cues improved | cues worse | CER |
| :--- | ---: | ---: | ---: |
| A Fish Called Wanda | 8 | 0 | 1.7% → 1.7% |
| King Kong | 3 | 0 | 21.3% → 21.3% |
| The Karate Kid | 228 | 0 | 2.3% → **1.8%** |
| Training Day | 301 | **1** | 3.1% → **2.2%** |

**One cue worse, and it is instructive.** On Training Day, an all-caps SDH line reads
`[<?>UsIc PLAYs]` for `[MUSIC PLAYS]`: the matcher had already lost `S`→`s` and `C`→`c` to scale
invariance, so the context arm saw lowercase on both sides of a correct `I` and rewrote it to `l`.
The rule fired correctly on evidence that was wrong before it arrived — the same lesson `naïve`
taught, on a disc where §"`C`/`c` is the cost of scale invariance" in
[`library-accuracy.md`](library-accuracy.md) is doing the damage.

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

`Config::lone_words` stays `false` for a reason none of the others has. **It knows a language**, and
every other rule in this pipeline is checkable against the material it fires on. 395 cues improved
and none made worse is a strong measurement and it is still a measurement of English discs: a track
in a language where a lone `l` *is* a word, or where `l'` elision is ordinary, would be damaged by
it and nothing in the tool can tell which language a bitmap is in. That is the argument for keeping
it behind a flag, not for leaving it unbuilt — the errors it fixes are four fifths of the largest
confusion family this project has measured.

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
- **The word-initial position, on VOBSUB.** `Iike` and `Iet` are what the other codec produces, and
  the context arm reaches them only because a longer word has a second side. What it cannot reach
  is `lt` and `lf` for `It` and `If` — two characters, one of them ambiguous, no evidence either
  way. The lone-word arm stops at one character deliberately; two is a different measurement.

[#8]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/8
[#12]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/12
[#15]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/15
