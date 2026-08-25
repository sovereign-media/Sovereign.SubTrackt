# A word list built out of eight films, and what it can honestly say

[#217][issue-217]. [`language-coverage.md`](language-coverage.md) shipped the character layer of
#189's reader: it counts characters the declared orthography cannot spell, needs no dictionary, and
is deliberately a floor. It catches `pà` because Swedish has no grave accent. It cannot catch a
wrong letter that is still in the alphabet, and it cannot catch `den` read as `det` at all.

This is the word layer. It answers the question the character layer cannot — *is this a word* — and
the useful part of the answer turned out not to be that question at all.

**The unattested rate is worthless. What one edit would repair a token is not.**

[issue-217]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/217
[issue-189]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/189
[issue-219]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/219

## The library is the corpus

A word list is a **data** dependency — per language, licensed, and stale in the direction that
produces false positives. #217 priced a character n-gram model as the no-dependency alternative.
Neither was needed: the library carries 5,183 sidecars, and while the overwhelming majority are
English there are **49 French, 17 Dutch, 11 Portuguese, 9 Swedish, 9 Spanish, 8 Norwegian, 8 Danish
and 8 Finnish**. Nothing is downloaded, nothing is licensed, and the corpus is drawn from the same
population as the discs being read.

`scripts/language/lexicon.py build --language swe` collects them. It is thin — 8 sidecars, 52,119
tokens, 7,930 distinct words — and being honest about how thin is the first thing it does.

### It rejected a sidecar before anything else used it

`build` runs a leave-one-out pass and drops any source sharing almost no vocabulary with the rest,
printing the rejection with its reason. That is `scripts/alternatives/select.py`'s rule applied
here: reject a file for being **a different language**, never for scoring badly at anything.

It fired immediately. `Tremors (1990) ... .swe.srt` is **Finnish** — *Huomenta, hra Basset* — and
89.4% of its words are unattested in nine films of Swedish. A word list built with it in would have
called a great deal of real Swedish impossible and a great deal of Finnish fine.

## Every figure has to clear a measured floor

`lexicon.py calibrate` rebuilds the lexicon without each source in turn and scores that source
against the rest. Every token in it is a real word of the language, so **every miss is a false
positive**. Three classes, and they do not have the same floor:

| language | unattested | one letter off | splits |
| :--- | ---: | ---: | ---: |
| English (8 sidecars, sampled) | 12.76% | 3.91% | 1.10% |
| Spanish | 17.89% | 4.16% | 1.16% |
| Swedish | 16.86% | 4.07% | 2.17% |
| Norwegian | 14.90% | 3.99% | 2.64% |

**A one-in-six unattested rate on text that is definitionally correct** is what a word list built
out of eight films costs. Any reader quoting an unattested rate as an error rate would be reporting
its own corpus.

The English lexicon is **sampled to eight sidecars on an even stride** rather than built from all
2,428. A control that differs from the thing it controls for in corpus size measures corpus size.

## What the tracks score

Gone Girl, four language streams of one disc, each against its own floor:

| | unattested | floor | one letter off | floor | splits | floor |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| **English** (control) | 12.90% | 12.76% | 4.76% | 3.91% | 1.12% | 1.10% |
| Spanish | 18.72% | 17.89% | **7.04%** | 4.16% | 1.55% | 1.16% |
| Swedish | 21.63% | 16.86% | **10.64%** | 4.07% | 3.02% | 2.17% |
| Norwegian | 21.88% | 14.90% | **11.61%** | 3.99% | 3.14% | 2.64% |

**The English control clears its floor by 0.14, 0.85 and 0.02 points.** The pipeline reads that
track at 1.4% character error, and the reader correctly reports almost nothing. That is what makes
the other three rows worth reading: Swedish clears its one-letter-off floor by 6.6 points and
Norwegian by 7.6, which is two and a half to three times the floor.

The unattested column, meanwhile, separates almost nothing from anything. It is printed because
hiding it would make the other two look better than they are.

## The classes it found, without being told what to look for

Nothing here knows about `å`. The reader is given a word list and a rule — *what single character
substitution would make this token a word* — and the classes fall out of the data. Only a token with
**exactly one** repair votes: a token six edits from six words says the lexicon is thin, not that a
character is confused.

Swedish:

| read | should be | count | examples |
| :--- | :--- | ---: | :--- |
| `â` | `å` | **184** | `ocksâ` `dâlig` `nân` `râkar` |
| `à` | `å` | 31 | `Röstbrevlàdan` `tvà` |
| `í` | `i` | 6 | `mÍna` `PrecÍs` `SannÍngen` |

Norwegian finds the same two at 72 and 13; English finds `í`→`i` at 29 and nothing else above ten.
This is [#189][issue-189]'s `å` defect and its `í` defect, rediscovered from the other end by an
instrument that was never told about either.

Spanish adds one nobody had named:

| read | should be | count | examples |
| :--- | :--- | ---: | :--- |
| `í` | `i` | 72 | `CÍnco` `gracÍa` `MurÍó` |
| `ì` | `i` | 18 | `SÌempre` `pÌenSo` `eSpacÌo` |
| **`î`** | **`t`** | **15** | `eXÎraño` `esÎo` `esÎos` `Îeoría` |

A lowercase `t` read as a **capital circumflex I**. It is not a missing character — both are in the
set — and it is not in the same family as the `í`/`Í` height confusion. It had no name before this.

### The split class is where the language fights back

The same machinery finds a lost word space: a token that comes apart into two attested words.

| | Swedish | Norwegian | English |
| :--- | :--- | :--- | :--- |
| `j` | **80** — `när\|jag` `tänker\|jag` `gillar\|jag` | **61** — `tenker\|jeg` `at\|jeg` | 12 |
| `c` | 87 — `ni\|ck` | 95 — `ni\|ck` | — |
| `e` | 54 — `present\|en` `dess\|ert` | 91 — `skriv\|er` `york\|er` | 8 |

**The `j` row is exact.** 80 and 61 are the same instances a hand-written regex counted for
[#219][issue-219] — `jag` and `jeg` fused to the word before them — found here with no knowledge of
either word.

**The `c` and `e` rows are false positives**, and they are the limit #217 stated in advance. `ni|ck`
is the name Nick, and `ni` and `ck` are both attested. `present|en` and `skriv|er` are Swedish and
Norwegian inflection and compounding, where the parts are words and the whole is a word. No rule
without a grammar can tell those from a lost space, so the split class is quotable **per letter**
and not as a total.

## What this does not claim

- **It is still a floor**, and a lower one than it looks. A real word read as a different real word
  is invisible: `den` for `det` passes every test here.
- **It does not score the language, it scores the corpus.** Every figure is relative to eight films.
  A thicker lexicon would lower every floor and every measurement together, and the two would not
  move by the same amount.
- **The split class does not generalise.** It is stated per letter for Swedish, Norwegian, Spanish
  and English above, and it would be worth much less on German or Finnish, where compounding is
  more productive still. #217 predicted exactly this.
- **The n-gram alternative was not built and its prediction is unscored.** It was the fallback for a
  language the library could not supply a corpus for, and the library supplied one.
- **One disc, four tracks.**

## Where the predictions landed

#217 made three. One was not tested and **the other two were both wrong**, which is worth recording
because the instrument was built on them.

1. *"The Spanish word layer finds at least 3× more wrong tokens than the character layer finds
   impossible characters."* **Refuted.** The character layer flags 317 (145 foreign letters plus 172
   accented capitals mid-word); the word layer's one-letter-off class clears its floor by 2.9 points
   of 13,629 tokens, about 390. Roughly 1.2×, not 3×. The two layers overlap far more than the
   prediction assumed — most of what the word layer finds in Spanish *is* the `í` family the
   character layer already had.
2. *"On Swedish and Norwegian the largest single class is the glued word."* **Refuted.** In Swedish
   the largest true class is `â`→`å` at 184 against the `j` split's 80, and in Norwegian it is 72
   against 61. The missing character beats the lost space on both tracks.
3. *"The n-gram alternative recovers less than half of what a word list finds."* **Unscored.**

## Reproducing

```console
$ scripts/language/lexicon.py build --language swe --out swe.lex.json
$ scripts/language/lexicon.py calibrate --lexicon swe.lex.json
$ scripts/language/census.py swe.srt --language swe --lexicon swe.lex.json
$ scripts/language/lexicon.py selftest        # pins the two edit rules
```

`build` walks the library and takes about a minute; `calibrate` and the census are seconds. The
English control is `--language eng --limit 8`.
