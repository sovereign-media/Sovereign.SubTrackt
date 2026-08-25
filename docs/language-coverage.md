# Two thirds of the library is in a language the set cannot spell

[#189][issue-189]. Every accuracy figure this project has published is English: the bench is nine
English tracks, `arial-ri.subtref` is generated from a charset chosen for English, and
[`library-accuracy.md`](library-accuracy.md) measures fifty titles that are all English. That is not
because the library is English.

**It is 50 language tags over 1,316 files, and Spanish is on 533 of them.**

Three instruments landed here, and between them they answer #189's §2 and §3 and the character layer
of its §1:

| | |
| :--- | :--- |
| `scripts/language/survey.py` | what languages the library actually carries |
| `xtask language-coverage` | what each orthography needs, whether the set has it, and what the matcher does with the ones it lacks |
| `scripts/language/census.py` | what an extraction contains that its declared language cannot spell |

The last is the one worth reading first, because it is the only measurement in this repository that
scores a track **nobody transcribed**. It needs no sidecar and no alignment. It cannot say which
word was on the screen; it can say that `à` is not a Swedish letter, and that is a fact rather than
a confidence score — the same argument the whole project rests on.

[issue-189]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/189
[issue-180]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/180
[issue-100]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/100
[issue-109]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/109
[issue-10]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/10

## What is on the shelf

`scripts/language/survey.py` reads the stream table of every file in the inventory — container
headers only, so the whole library is a few minutes and no bitmap is decoded.

| | |
| :--- | ---: |
| Files in the inventory | 1,328 |
| **Files read** | **1,316** |
| Unreadable | 12 |
| Distinct language tags | **50** |
| (file, language) pairs | 5,089 |

The top of the table is the point:

| tag | | files | streams |
| :--- | :--- | ---: | ---: |
| `--` | *untagged* | 658 | 939 |
| `eng` | English | 620 | 663 |
| `spa` | **Spanish** | **533** | 672 |
| `fre` | **French** | **477** | 577 |
| `ger` | German | 248 | 302 |
| `por` | Portuguese | 180 | 228 |
| `dut` | Dutch | 175 | 192 |
| `swe` | Swedish | 175 | 183 |
| `nor` | Norwegian | 155 | 163 |
| `ita` | Italian | 154 | 188 |

The untagged row is not a mystery language. [#180][issue-180] predicted that the muxer leaves the
default stream untagged, and stream 0 of Gone Girl is English and carries no tag. The survey counts
it as its own row rather than guessing, because counting untagged as English would be inventing data
to avoid an unknown — but the pairs below are quoted over the **tagged** ones for that reason.

Splitting the 4,429 tagged pairs by what the shipped reference set can spell:

| | pairs | |
| :--- | ---: | ---: |
| Latin script, every required character in the set | 1,379 | 31.1% |
| Latin script, **at least one required character missing** | 2,331 | **52.6%** |
| Not Latin script at all | 719 | 16.2% |

**Two thirds of the tagged tracks in the library are in a language this set cannot spell**, and half
of those are Latin-script languages that are one accent short rather than a different writing system.

## What each orthography needs

`xtask language-coverage` holds a table of what every one of those 50 tags requires beyond ASCII,
from first principles: not what a disc happens to draw, but what the standard orthography cannot be
written without. `å` in Swedish. `ñ` in Spanish. `¿` in Spanish, which is a *letter*'s worth of
obligation — Spanish opens a question with it and there is no variant that does not.

`charset()` in [`font.rs`](../crates/subtrackt-glyph/src/font.rs) is ASCII printable, plus a
hand-listed 45 Latin-1 letters, plus the eighth note. 140 characters. Against the table:

| tag | required | in set | absent | | tag | required | in set | absent |
| :--- | ---: | ---: | ---: | :--- | :--- | ---: | ---: | ---: |
| `eng` English | 0 | 0 | **0** | | `pol` Polish | 18 | 2 | 16 |
| `ger` German | 7 | 7 | **0** | | `hun` Hungarian | 18 | 14 | 4 |
| `ita` Italian | 14 | 14 | **0** | | `ice` Icelandic | 20 | 12 | 8 |
| `dut` Dutch | 10 | 10 | **0** | | `hrv` Croatian | 10 | 0 | 10 |
| `fin` Finnish | 4 | 4 | **0** | | `slv` Slovenian | 6 | 0 | 6 |
| `ind` Indonesian | 0 | 0 | **0** | | `est` Estonian | 12 | 6 | 6 |
| `spa` Spanish | 16 | 14 | 2 | | `lit` Lithuanian | 18 | 0 | 18 |
| `fre` French | 34 | 26 | 8 | | `lav` Latvian | 22 | 0 | 22 |
| `por` Portuguese | 24 | 20 | 4 | | `slo` Slovak | 34 | 14 | 20 |
| `swe` Swedish | 8 | 6 | 2 | | `tur` Turkish | 12 | 6 | 6 |
| `nor` Norwegian | 8 | 2 | 6 | | `rum` Romanian | 10 | 4 | 6 |
| `dan` Danish | 8 | 2 | 6 | | `vie` **Vietnamese** | **134** | 26 | **108** |
| `cze` Czech | 30 | 10 | 20 | | `cat` Catalan | 21 | 20 | 1 |

Nine Latin-script tags of 34 are covered in full, and one of them is Indonesian, which is ASCII
throughout — the only non-English language in the library the set already spells by accident rather
than by design. Croatian, Serbian, Slovenian, Lithuanian and Latvian have **not one** of their
required characters. Vietnamese needs 134 and has 26.

The non-Latin tags read 0 required, and that is not a claim that Russian needs nothing. It is that
this table is about characters the set could plausibly gain; a Cyrillic track needs a different
deliverable, and #189 §4 is right that it is an honest **rejection** rather than a read.

### Where a row made a call

The table prints its own judgement calls under itself, and the rule behind them is asymmetric on
purpose. A row that lists one character too many understates a gap by one. A row that lists one too
few makes the census below call a real word impossible, and a false entry in the only column an
instrument prints is the failure `scripts/bench/roster.json` records at length about sidecars.
**Swedish `é` was left out on the first pass, and the Norwegian census flagged `én` — a real
word — nine times.** It is in now.

## What the set does with a character it does not have

This is #189 §3, and it is the half that matters, because the two answers are not equally bad.

A character the set **rejects** is a fact. It arrives as an unmatched glyph, it is counted, and
`--on-unmatched` can act on it — the placeholder policy prints it, the threshold gate can refuse the
track. A character the set **rehomes** is invented data: `J` is a perfectly ordinary character to
find in a subtitle, so no coverage figure, no census and no gate can distinguish it from a `J` that
was really there. Nothing downstream can catch the second kind.

`xtask language-coverage` builds two reference sets from the same faces through the same
normalisation — the real one over `charset()`, and a probe over the characters `charset()` omits —
and scans every probe entry against the real set's matcher at the shipped thresholds. A probe
character cannot match itself, so whatever comes back is what the pipeline would emit in its place.

Over Arial regular and italic, ceiling 51 cells:

> **213 absent characters: 150 rehome silently, 63 come back unread.**

Sorted by how *confident* the rehoming is, because a short distance is one the matcher is most sure
of, and sureness is exactly what makes it unfindable:

| | | needed by | Regular | Italic |
| :--- | :--- | :--- | :--- | :--- |
| `ĺ` | U+013A | `slo` | → `Í` 2 | → `Í` 1 |
| `Ả` | U+1EA2 | `vie` | → `À` 3 | → `À` 2 |
| `Ã` | U+00C3 | `por` `vie` | → `Ä` 7 | → `Ä` 4 |
| `İ` | U+0130 | `tur` `aze` | → `Í` 6 | → `Ì` 4 |
| `Ő` | U+0150 | `hun` | → `Ò` 6 | → `Ô` 4 |
| `ă` | U+0103 | `rum` `vie` | → `ä` 7 | → `ä` 6 |
| `Å` | U+00C5 | `swe` `nor` `dan` | → `Ä` 9 | → `Ä` 8 |
| `å` | U+00E5 | `swe` `nor` `dan` | → `â` 18 | → `à` 14 |
| `ø` | U+00F8 | `nor` `dan` | → `o` 45 | unread 64 |
| `¿` | U+00BF | `spa` | unread 55 | unread 53 |
| `æ` | U+00E6 | `fre` `nor` `dan` `ice` | unread 64 | unread 61 |
| `«` | U+00AB | `fre` | unread 82 | unread 74 |
| `’` | U+2019 | *typography* | → `'` 29 | → `'` 48 |

Four things in that table are worth naming.

**`ĺ` at two cells is [#10][issue-10] generalised.** `l` and `I` were measured at distance *zero*,
and [#109][issue-109] separated them by ink width. Put an acute over each and the pipeline has no
term left: `ĺ` and `Í` differ by a fraction of a stroke, and Slovak's `ĺ` lands on Spanish's `Í`
more confidently than any two distinct characters in the set land on each other.

**Not every rehoming is damage.** `’` → `'` is the answer a caller wants, and a curly quote is not
in any orthography's requirements anyway. The distinction the column cannot draw is between a
transliteration and a lie, and it is drawn by hand here rather than by the tool.

**`å` splits by face**, → `â` upright and → `à` italic, which is the prediction the disc confirmed
below and is why the instrument reports per style rather than per character.

**`¿` is rejected here and is not rejected on the disc.** That is the next section.

## What it looks like on a disc

Gone Girl is already on the bench, and `subtrackt list` shows 24 subtitle streams beside the English
one the bench has always read. Same disc, same authoring, same typeface, same resolution: **the
language is the only variable.** Four streams extracted against `arial-ri.subtref`, with English on
the same disc as the control.

| | fit | glyphs read | placeholders | letters the language does not have |
| :--- | ---: | ---: | ---: | ---: |
| English (stream 0) | 11.8 | 99.9% | 68 — 0.09% | 102 — 0.13% |
| Swedish (13) | 12.8 | **99.9%** | 65 — 0.09% | **1,061 — 1.42%** |
| Norwegian (10) | 12.4 | 98.8% | 763 — 1.03% | 1,059 — 1.43% |
| Spanish (12) | 13.5 | 99.2% | 531 — 0.71% | 145 — 0.19% (+172 inner capitals) |
| Russian (11) | **27.8** | **83.0%** | 10,397 — 14.81% | **44,763 — 63.78%** |

**The Swedish track reads 99.9% of its glyphs, exactly as the English one does, and carries ten
times the impossible characters.** That single row is #189's thesis: the matched fraction is not a
correctness signal. Both tracks pass every check this project has.

### `å` is worse than the issue thought

`scripts/language/census.py` on the Swedish track:

```
    â  U+00E2     851   Kan du hälla upp en bourbon ât mig?
    à  U+00E0     171   Närjag tänker pà min fru -
    Í  U+00CD      28   MÍssourÍ?
```

#189 counted the 171 `à` and stopped there. The probe predicted the upright face would land on `â`
instead, and the disc has **851 of them** — so `å` is wrong 1,022 times, not 171, and 83% of the
damage was in a character nobody had looked for. Norwegian is the same shape: 738 `â` and 185 `à`.

None of it is flagged. The Swedish track's 65 placeholders are *fewer* than English's 68 on the same
disc, and the reason is that the wrong answer is what made the count look healthy.

### `í` read as a capital, caught without a dictionary

The Spanish track's `Í`/`Ì`/`Î` are the confusion #189 §3 found, and they are not a missing-character
problem: every glyph involved is in the set. `í` is the only vowel whose accent *replaces* a mark
rather than adding one, which leaves height as the only separator from `Í` — the neighbourhood
[#109][issue-109] mapped for `l` and `I`.

The census catches it with a rule that needs no word list: **an accented capital inside an otherwise
lowercase word is impossible in every one of these orthographies.**

```
    Í  U+00CD     172   presencÍa CÍnco pronuncÍa
    Ì  U+00CC      78   SÌempre pÌenSo en Su cabeZa.
    Î  U+00CE      59   una peroraÎa
```

Restricted to *accented* capitals deliberately: a bare capital mid-word is ordinary in a name —
McDonald, DVDs — and only the accented ones are the defect.

### `¿`, and why the probe is not the disc

The Spanish track has **396 lines ending in `?` and zero `¿` characters anywhere.** Most of the
missing openers are among its 531 placeholders. Thirty are not: they are the letter `J`, in a
position no Spanish word puts one.

```
<i>"JEn qué estás pensando?"</i>
<i>"JCómo te sientes?"</i>
<i>"JQué nos hemos hecho el uno al otro?"</i>
```

The probe says `¿` is rejected — 55 cells against a 51-cell ceiling, in both faces. The disc says
it is rejected *most* of the time. **A clean 96px outline and a decoded 30px bitmap are not the same
glyph**, and the difference is worth four cells here, which is the whole margin.

So the probe names which characters are at risk and does not predict the rate, and it errs in both
directions: `ø` rehomes to `o` on the outline and placeholders on the disc, `¿` does the reverse.
Neither instrument replaces the other, and the disc stays the only one that can accept a change.

*(#189 reported 63 `J`s and 389 `?` lines against this measurement's 30 and 396. Same disc, same
policy, a later build; the shape of the finding is unchanged and the counts are what this run
produced.)*

### Cyrillic is rejected by luck of margin

The Russian track fails the default gate, and the message is the whole argument:

```
error: track rejected by the threshold gate: 50852 of 61249 glyphs read (83.0%), floor is 90.0%
```

Seven points. What the guard holds on is that 17% of Cyrillic happens to fall outside a distance
threshold, not on any knowledge that no glyph in the track is a Latin letter. The census puts a
number on the other 83%: **63.78% of the characters emitted are letters Russian does not contain**,
against 14.81% honest placeholders. `ceoeü`, `I?peôcma���lo`.

There is one signal already in the output that does separate it. **Fit is 27.8 against 11.8 to 13.5
for every Latin track on the same disc** — better than two to one, and computed today, on every
run. That is a lead for #189 §4 rather than a guard: nothing has swept it, and one disc is one disc.

### A non-English track is a better test of an English defect

Word gaps are lost before a tall narrow letter with a high-starting ascender, which is a defect in
no language in particular:

| | glued | examples |
| :--- | ---: | :--- |
| Swedish, `jag` | 80 | `Närjag`, `Serjag`, `Görjag`, `Alltjag` |
| Norwegian, `jeg` | 62 | `Nàrjeg`, `Ogjeg`, `Trorjeg`, `Kjennerjeg` |
| English, `you`, same disc | 6 | `ifyou`, `hearyou`, `knowyou`, `protectyou` |

Six instances is noise the bench cannot rank a change by. 142 across two Scandinavian tracks is a
signal, and it is the same bug — `j` in one language, `y` in the other. The English-only bench is
not merely blind to non-English defects; it is a **weak instrument for the defects it does have**.

## What this does not claim

- **The census bounds error from below and does not measure it.** A real word read as a different
  real word is invisible to it — `den` for `det`. So is a wrong letter that is still in the
  alphabet: a Swedish `a` read as an `o` is an ordinary Swedish character in an ordinary Swedish
  position. Every figure above is a floor. Quoting one as an error rate would be the invented-data
  failure `CLAUDE.md` forbids.
- **It is weakest exactly where a sidecar is strongest.** A sidecar can tell `den` from `det` and
  cannot be had for these tracks at all. That is the argument for having both.
- **Confidence varies by language.** The verdicts here are Spanish, Swedish, Norwegian and Russian,
  where a character outside the orthography is unambiguous. The character layer generalises to any
  alphabetic language in the table; the *word* layer #189 §1 asks for does not, and would be worth
  less on a language with productive compounding.
- **The orthography table is written by hand and is the weakest link.** It is 50 rows of judgement.
  Every close call is printed under the table, and one of them was already wrong.
- **One disc.** Every per-track figure here is Gone Girl. It is the right disc for the question —
  24 languages, one authoring — and it is one disc.

## What is left

- **A word-level reader** — #189 §1's full instrument, which needs word lists this repository does
  not have and a decision about which languages it claims.
- **A script guard that is not a fraction** — #189 §4. The container declares `rus`, the reference
  set knows its own charset, and nothing compares them. `fit` at 27.8 versus 11.8 is where to start.
- **Growing the charset** — the table above says what to grow it to, and the rehoming column says
  which characters are urgent. It is not free: `å` against `à` is a ring against a stroke over the
  same body at 256 cells, the neighbourhood [#100][issue-100] already found
  hard, and adding a character that lands inside the margin of one already there trades a silent
  wrong answer for a different silent wrong answer.
- **The lost word gap**, which is the one defect here that is not about language at all.

## Reproducing

```console
$ scripts/language/survey.py --out library-languages.json
$ cargo run --release -p xtask -- language-coverage C:/Windows/Fonts/arial.ttf \
      --italic C:/Windows/Fonts/ariali.ttf
$ cargo run --release -p xtask -- dump-sup "...Gone Girl...mkv" swe.sup --stream 13
$ subtrackt extract swe.sup --reference arial-ri.subtref --on-unmatched placeholder -o swe.srt
$ scripts/language/census.py swe.srt --language swe
```

The survey is a few minutes over the whole library. `language-coverage` is a second and needs
nothing but a font. Each stream dump is about a minute; each extraction is one second.
