# Five other tools read the same bytes

`README.md` argues that a general OCR engine's failure mode is a confident wrong answer, and that
this is why the project exists. It is the load-bearing claim of the whole design, and until this
document it was **the only claim in the README with no measurement behind it**. Tesseract appeared in
four places in the tree and was a rhetorical foil in every one: never installed, never run, never
scored.

There was a real measurement once, in [sovereign#328][issue-328] (2026-07-27), the analysis that
motivated building this library. It timed ffmpeg's `ocr` filter and Tesseract 5.3.4 against **Big
Hero 6 (2014)** and produced hard cost numbers. It **never measured character or word error**: its
entire accuracy case was five sample lines. So there was a gap on both sides — the prior analysis had
timings and no accuracy, this project had accuracy and no competitor. This closes both, on the same
media, through one instrument.

[issue-328]: https://github.com/sovereign-media/sovereign/issues/328
[issue-131]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/131

## What this can and cannot claim

**Permitted:** *on this track, against this transcript, A is X points ahead of B.*
**Forbidden:** *B's error rate is X%.*

Both sentences belong here, and the reason is that four of the five corpus items are scored against a
release sidecar rather than against ground truth. `disc.rs` is explicit that a release subtitle is
"enough to rank two extractions of one track against each other… not enough to certify an absolute
figure."

The escape is narrow but real, and it is worth writing down precisely:

- Both engines read the **same track**, scored against the **same transcript**, so sidecar divergence
  charges both.
- **The bias has a known sign.** Edit distance is not linear: where the sidecar differs from the
  disc, an engine that *also* misreads that span is charged once rather than twice. The paired gap is
  therefore **biased toward zero** — it understates whoever is ahead. That is conservative for the
  winner, which is the direction you want when the winner is the home team.
- The **fixture** anchors everything else with true ground truth over 500 cues, and it is the only
  absolute CER in this document.

One more caveat, which turns out to matter more than any of the above: **the instrument's own
uncertainty is larger than most of the gaps it measures.** Scoring one engine against every English
sidecar in a title's own folder moves the number by up to 14 points on this corpus. Any inter-engine
gap smaller than that title's spread is reported as inconclusive, and the spread is printed beside
the gap rather than left for a reader to go looking for.

## Method

Every engine reads the same flat `.sup`. `xtask dump-sup` is byte-exact — `docs/error-census.md`
records that extracting the rip and extracting the dump produce byte-identical subtitles — so no
demux difference can contaminate an accuracy or a timing figure. Nothing here is given an `.mkv`.

Each measurement is one container: `--network none`, fixed `--cpus` and `--memory`, corpus mounted
read-only, outputs to a tmpfs and copied out only after `time -v` has exited. Strictly serial, three
repeats round-robin by repeat so drift hits every engine equally. `scripts/alternatives/` holds the
harness; `bench.py` has the four reasons it is a sibling of `scripts/accuracy/sweep.py` rather than a
flag on it, the first of which is that `sweep.py` defaults to four workers and a stopwatch inside a
parallel script is how a benchmark starts lying quietly.

Scoring runs on the host, outside every timed region, through the same `xtask srt-score` that
produced every accuracy figure this project has published. **One instrument for every row** is the
whole method: a competitor number produced by a second code path would have to be independently
trusted, and nothing would be able to say whether a gap was the engines or the scorers.

### The sidecar is chosen once, and frozen

`scripts/accuracy/analyse.py` picks the best-agreeing sidecar per title. That rule is right there and
would be poison here: applied per engine it lets each engine be scored against whichever transcript
flatters it most, which is the most effective way to produce a wrong ranking that looks careful.

So the sidecar was chosen **once**, by one engine (`subtrackt-arial`), before any other engine ran,
and then frozen into `corpus.json` with its SHA-256. What the selector saw is recorded beside the
choice, and the spread between candidates is the instrument's own uncertainty on that title.

### Output normalisation, in the harness and never in the scorer

Competitor SRTs carry BOMs, CRLF line endings and — potentially — `{\an8}` ASS overrides. Widening
`srt-score`'s tag stripping to `{…}` **would silently move every published figure in this project**,
because release sidecars contain `{\an8}` too. So normalisation happens in `analyse.py`, identically
for every engine including subtrackt: strip BOM, normalise CRLF, remove ASS overrides, drop cues
empty after stripping, renumber. Both files are kept — `out.raw.srt` as the engine wrote it and
`out.srt` after — and the count of characters removed is reported per engine so a reader can see the
normalisation was not load-bearing.

Punctuation folding — curly quotes to straight, en and em dashes to hyphen, on both sides — is a
**sensitivity row and not the headline**, because it helps whichever engine emits the typographic
form.

### Italic does not reach CER, and a reader will assume it does

Verified in the code rather than assumed: `disc.rs:88` parses `italic` off the release cue and stores
`strip_tags(&joined)` as the text; `strip_tags` removes everything between `<` and `>` on **both**
sides; and the upright/italic split selects on the **release's** flag, not the extraction's. So an
engine emitting no `<i>` is **not penalised on CER**, and the split still works. Tag agreement is
reported separately, as a capability rather than as accuracy.

## The corpus

| item | cues | truth | why it is here |
| :--- | ---: | :--- | :--- |
| `fixture` | 500 | **true**, generated | The only certifiable absolute CER. `xtask make-fixture --repeat 50`, scored against the `synthetic.srt` [#131][issue-131] added so one instrument scores every row. |
| `clover` | 822 | pinned sidecar | 10 Cloverfield Lane (2016). The tuned baseline; a marked italic act at 6% of characters. |
| `wanda` | 1,396 | pinned sidecar | A Fish Called Wanda (1988). SDH, scored against the SDH sidecar — 1,396 cues on both sides exactly. |
| `gonegirl` | 2,442 | pinned sidecar | Gone Girl (2014). The long end of the library; 18% of lines lean and the release marks none of them. |
| `bighero6` | 1,745 | **none — smoke** | sovereign#328's title, carried for cost. See below. |
| `deathrace2` | — | none | One track of the 37 #328 projected at 8.4 hours. Measured once and multiplied. |

Big Hero 6's 1,745 bitmaps reproduce #328's count exactly, which is the cross-check that the two
analyses are looking at the same track.

### Big Hero 6 cannot be scored, and that is a finding rather than a gap

[#131][issue-131] expected this title to be scoreable, noting only that it "carries one [sidecar], so
its row has no sidecar spread and must say so". It is worse than no spread: **the one sidecar does not
match the track.** The disc's track is SDH — it opens `(WHISTLING)`, `(LOUD THUDDING)`, `MAN: Get up!`
— and the only English sidecar in the folder is plain dialogue carrying none of it, 1,324 cues against
the track's 1,745. The selector read 22.8% against it, which measures the mismatch and not any engine.

This is the roster's own rule, applied to a new entry: *a scored track needs a sidecar of matching
convention, not just any sidecar*, and *check a new entry's sidecar against its extraction's shape
before trusting the number*. It is the third time this project has been caught by it — The Prestige,
then Wanda and Airplane! in #175, now this — and the first time the check was run before publishing
rather than after. Big Hero 6 is a **smoke** entry: it claims no accuracy, and it carries the cost
comparison that is the reason it was wanted.

## The instrument

Before a single competitor number is quoted, the harness has to reproduce what this project already
publishes. It does — and finding out cost two published figures, because they had gone stale.

| check | result |
| :--- | :--- |
| **Reproduce Cloverfield** — same set, flags and sidecar, through the container | **0.4% / 0.8%** over **818** paired cues at **99.9%** coverage and **98.8%** italic agreement. The *shape* matches the published figure exactly; the *rate* does not — see below |
| **Reproduce the fixture ceiling** — `xtask accuracy` vs the CLI scored against the new `synthetic.srt` | Both **0.0%** over the 328-character single pass. The two paths agree, which is what this check exists to test |
| **Identity** — score a file against itself | 0.00% CER/WER, 0 unpaired |
| **Empty** — empty SRT against a sidecar | 100% CER, no crash |
| **Determinism** — SHA-256 of every output across three repeats | **Byte-identical for all nine engines on all six items.** No engine needed a median |
| **Alignment agreement** — `(scale, offset_ms)` per engine per title | **Identical `(1.0, 0)` for every engine on every title.** No engine gained a pairing advantage; no `srt-score` flag was needed |
| **Nothing downloaded** — `--network none` on all 192 units | All 192 completed. Nothing was fetched at run time |
| **Failures** | **0 of 192.** Every engine completed every item |
| **RSS cross-check** — `time -v` vs cgroup `memory.peak` | Within 4% for subtrackt; **33-40% apart for the multi-process tools**, exactly as §(j) feared. The cgroup figure is quoted for those and the disagreement printed |
| **Cold vs warm** on Cloverfield, ten each | 0.650 s vs 0.638 s — **1.8%**, inside the 2% gate |
| **Timing floor** — `/usr/bin/time -v /bin/true` x 50 | **0.00 s**: below `time -v`'s centisecond resolution. Nothing under 0.03 s is quoted as a number |

### Two published figures had drifted, and this is how that was found

The harness reproduces Cloverfield's **shape** to the cue — 818 paired, 99.9% coverage, 98.8% italic
agreement, all three matching `README.md` exactly — while reading **0.4%** where the README said
0.6%. That is not the harness disagreeing with the pipeline. The **released v0.0.3-alpha binary reads
0.4% too**, so the drift predates the release: the figure was written in #128 and the pipeline
improved underneath it without anyone re-reading the sentence.

The ceiling had drifted further. `README.md` said **1.2%**; `xtask accuracy` reads **0.0%** today.

Both are corrected in this change, and the lesson is worth more than the corrections: **a figure with
no instrument that re-derives it will go stale silently.** `scripts/bench/` exists precisely so the
bench numbers cannot do this, and the two figures that drifted were the two with no such harness.

## Result

### Table 1 — the contenders

| tool | version | runtime needed | model / database | install tree |
| :--- | :--- | :--- | ---: | ---: |
| **subtrackt** | 0.0.3-alpha (`4635cc1`) | none, static musl | `arial-ri.subtref`, **22.4 kB** | **2.07 MB** |
| seconv, Tesseract | 5.0.0 (tag v5.1.0) | none, self-contained | apt `eng` 14.0 MB / `tessdata_best` 14.7 MB | 90.7 MB |
| seconv, nOCR | same | same | `Latin.nocr` **476 kB**, fetched separately | 90.7 MB |
| seconv, binary compare | same | same | `Latin.db` **480 kB**, fetched separately | 90.7 MB |
| pgsrip | 0.1.12 | Python 3.12 + OpenCV | `tessdata_best` 14.7 MB | **317.5 MB** |
| PgsToSrt | 1.4.8 | .NET 8 runtime | `tessdata_best` 14.7 MB | 21.3 MB + **66.8 MB** runtime |

Tesseract 5.3.4 and leptonica 1.82.0, from Noble's archive — the same builds sovereign#328 measured,
which is what makes the cost cross-comparison like-for-like rather than approximate.

**Two of these do not run out of the box, and both facts belong in this table rather than in a
footnote.** `PgsToSrt` defaults to `--tesseractversion 4` and loads `libtesseract.so.4`, which no
supported Ubuntu has shipped for years: run as documented on 24.04 it produces a
`DllNotFoundException` and no subtitles. `pgsrip` depends on `opencv-python` rather than the headless
build, so importing it fails with `libGL.so.1: cannot open shared object file` until two X11
libraries are installed in a container that has no display. Neither is a criticism of the OCR; both
are what a pipeline actually has to absorb.

### Table 2 — accuracy on the fixture, the only true ground truth

500 cues, generated, scored against `synthetic.srt`. **This is the only absolute CER in this
document.**

| engine | cue CER | track CER | WER | of its error, from an accented character |
| :--- | ---: | ---: | ---: | ---: |
| **subtrackt, fitted** | **2.4%** | **2.3%** | 11.5% | **7.9%** |
| subtrackt, Liberation | 13.6% | 13.2% | 46.2% | — |
| seconv, Tesseract (apt) | 9.4% | 9.1% | 30.4% | **70.7%** |
| seconv, Tesseract (best) | 10.3% | 10.0% | 32.2% | 63.6% |
| PgsToSrt | 10.5% | 10.2% | 33.1% | 60.1% |
| pgsrip | 10.9% | 10.6% | 34.3% | 58.9% |
| seconv, nOCR | **100.0%** | 97.0% | 100.0% | — |
| seconv, binary compare | **100.0%** | 97.0% | 100.0% | — |

**Read the last column before the first.** The fixture is deliberately accent-dense — it was built
for #48's diacritic work, and four of its ten cues are French, Spanish or Italian — and every
Tesseract arm above was given an **English** model. Between 59% and 71% of their error is one class:
`à→a`, `á→a`, `ï→i`, `è→e`, `ù→u`, the diacritic stripped. Ours is 7.9%.

That is a configuration *we* chose for the competition, so it was re-run rather than argued about:

| `seconv-tesseract` on the fixture | cue CER | track CER |
| :--- | ---: | ---: |
| `--ocr-language:eng` (the headline arm) | 9.4% | 9.1% |
| **`--ocr-language:eng+fra+spa+ita`, `tessdata_best`** | **7.4%** | **7.2%** |
| subtrackt, fitted | 2.4% | **2.3%** |

Giving Tesseract the languages the fixture is actually written in is worth **1.9 points** and does
not close the gap: 7.2% against 2.3%, still more than 3x. Removing a class from a denominator would
have predicted about 2.7% and would have been wrong, which is why the arm was run instead of
estimated — with four models loaded the engine recovers accents and loses ground elsewhere, and the
net is under two points.

So the fixture row survives the fairest configuration available to it. It is still the row where the
corpus most suits us, and the disc rows below are where real material lives.

### Table 3 — accuracy on the discs, and why it settles nothing

Track-level CER, median of three repeats, against one pinned sidecar per title.

| engine | clover | wanda | gonegirl |
| :--- | ---: | ---: | ---: |
| subtrackt, fitted | 0.8% | 1.3% | 1.3% |
| subtrackt, Arial | 0.8% | 1.3% | 1.3% |
| subtrackt, Liberation | 10.0% | 9.0% | 10.5% |
| seconv, Tesseract (apt) | 1.7% | 0.9% | 2.2% |
| seconv, Tesseract (best) | 1.6% | **0.8%** | 1.9% |
| pgsrip | **0.6%** | 3.5% | **0.8%** |
| PgsToSrt | 1.5% | 0.9% | 1.8% |
| seconv, nOCR | 96.8% | 97.0% | 97.0% |
| seconv, binary compare | 96.8% | 97.0% | 97.0% |
| **best competitor − subtrackt** | **−0.2** | **−0.5** | **−0.5** |
| **sidecar spread on this title** | **14.3** | **3.4** | **14.0** |

**Every gap is an order of magnitude smaller than the instrument's own uncertainty.** Scoring one
engine against the other English sidecar in the same folder moves Cloverfield by 14.3 points and Gone
Girl by 14.0. Against that, a 0.2-point lead means nothing at all.

So the honest verdict on all three discs is **inconclusive**, and it is inconclusive in the direction
that costs us: taken at face value a competitor is ahead on every one of them. What the numbers
support is *"on real Blu-rays, subtrackt and a Tesseract wrapper read within half a point of each
other, and this instrument cannot separate them"*. They do not support a claim that either wins.

### Table 4 — cost

Gone Girl, 2,442 cues, median of three:

| engine | wall | CPU s | %CPU | ms/cue wall | ms/cue CPU |
| :--- | ---: | ---: | ---: | ---: | ---: |
| **subtrackt, Arial** | **1.99 s** | **1.70** | 85% | **1** | **1** |
| seconv, nOCR | 13.40 s | 13.41 | 99% | 5 | 5 |
| seconv, binary compare | 14.30 s | 14.39 | 100% | 6 | 6 |
| PgsToSrt | 132.23 s | 522.15 | **394%** | 54 | 214 |
| seconv, Tesseract (apt) | 212.88 s | 274.65 | 129% | 87 | 112 |
| seconv, Tesseract (best) | 270.21 s | 365.18 | 135% | 111 | 150 |
| pgsrip | 345.87 s | 439.37 | 127% | 142 | 180 |

**Wall clock alone would flatter the competition badly.** PgsToSrt looks 2.6x faster than
`seconv-tesseract` in wall clock and costs **1.9x more CPU**; it runs at 394% on a four-CPU
container. subtrackt is single-threaded at 85%. The wall figure is what a user feels and the CPU
figure is what a queue pays, so both are here.

**sovereign#328's cost model transfers.** It measured Tesseract at 55–102 ms/frame on 1080p PGS;
`seconv-tesseract` lands at **76–88 ms/cue wall** and 99–114 ms CPU across all six items. The
per-cue rate is stable to within about 15% across titles from 1988 to 2016.

Whole corpus, one pass over all six items:

| engine | wall | CPU |
| :--- | ---: | ---: |
| **subtrackt, Arial** | **6.1 s** | **5.2 s** |
| seconv, nOCR | 41.5 s | 41.8 s |
| PgsToSrt | 586.5 s | 2,320.2 s |
| pgsrip | 607.6 s | 900.6 s |
| seconv, Tesseract (apt) | 684.0 s | 877.7 s |

**Death Race 2, one track measured and 37 projected** — the arithmetic row, against #328's 8.4 hours
for the naive route:

| engine | one track | x37 wall | x37 CPU |
| :--- | ---: | ---: | ---: |
| **subtrackt, Arial** | **1.0 s** | **37 s** | **37 s** |
| seconv, nOCR | 6.6 s | 4.1 min | 4.1 min |
| pgsrip | 59.3 s | 0.61 h | 1.08 h |
| PgsToSrt | 67.5 s | 0.69 h | **2.74 h** |
| seconv, Tesseract (apt) | 103.8 s | 1.07 h | 1.36 h |
| seconv, Tesseract (best) | 131.7 s | 1.35 h | 1.83 h |

#328's 8.4 hours was the naive per-frame route; a real tool doing the same work costs 0.6–1.4 hours
of wall clock and up to 2.7 core-hours. This pipeline costs **37 seconds**.

### Table 5 — memory

| engine | peak RSS (`time -v`) | cgroup `memory.peak` | disagreement |
| :--- | ---: | ---: | ---: |
| **subtrackt** | **214 MB** | 216 MB | 4% |
| seconv, Tesseract | 158 MB | 148 MB | 20% |
| seconv, nOCR | 219 MB | 178 MB | **40%** |
| seconv, binary compare | 221 MB | 180 MB | **37%** |
| PgsToSrt | 678 MB | 573 MB | 8% |
| pgsrip | **1,259 MB** | **1,858 MB** | **33%** |

§(j) was right to insist on the cross-check. For `pgsrip`, which spawns `tesseract` as a child,
`time -v` **understates the tree by a third** — 1,259 MB reported against 1,858 MB actually charged
to the cgroup. Quoting the single-process figure would have flattered it in the one column where it
loses worst.

**pgsrip also scales with track length rather than streaming**: 423 MB on the 500-cue fixture,
456 MB on Cloverfield, 724 MB on Wanda, 1,259 MB on Gone Girl. Reproducible to the megabyte across
all three repeats. Anyone running it under a container memory limit should size for the longest film,
not the average one.

### Table 6 — the correctors, and what they touch

| corrector | ships | cues changed, fixture | cues changed, Cloverfield | Cloverfield CER |
| :--- | :--- | ---: | ---: | :--- |
| subtrackt post-correct | **on** since #188 | 90 of 500 | **3 of 822** | 0.8% either way |
| seconv "fix common errors" | **off** | 350 of 500 | **354 of 822** | 1.7% → **0.8%** |

**#131 predicted this asymmetry and predicted it backwards.** It expected Subtitle Edit to run an
OCR-fix pass by default and subtrackt's corrector to ship off. Both are the other way round: seconv's
pass is opt-in, and #188 turned ours on after v0.0.3-alpha was tagged.

The interesting half is the blast radius. seconv's corrector **rewrites 43% of the cues on a disc**
and takes its CER from 1.7% to 0.8% — a real improvement, and enough to close most of the gap to
subtrackt. Ours rewrites **three cues in 822**. On the fixture the same pass makes seconv *worse*,
9.1% to 11.2%, because it is a spell-corrector making confident guesses about text it cannot read: on
familiar English dialogue the guesses land, and on `Està más allá` they do not.

That is the README's argument with a number attached, in both directions. A corrector that touches
43% of the output is not recovering information, it is supplying it — and nothing downstream can tell
which cues were supplied.

### Table 7 — capability

The table the argument actually rests on. If a competitor reads these discs better than SubTrackt,
that goes in the headline and the argument rests entirely on this table — which is where it always
claimed to rest.

| | detectable no-match | per-glyph confidence | italic tag | unattended | gateable failure | aimed at a typeface | ranks candidates against the track |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **subtrackt** | **yes** — U+FFFD, counted per glyph and per track | **yes** | yes, measured | yes | **yes** — `--on-unmatched fail-track` / `threshold` with `--min-matched` | **yes** — a `.ttf`, headless, seconds | **yes** — `fit` scores every candidate and ranks them |
| seconv, Tesseract | no | no | no | yes | no | no | no |
| seconv, nOCR | a literal `*` inline in the text — a **marker, not a count** | no | no | yes, with a database fetched separately | no | partly — real fonts, but **system fonts only, GUI only, additive** | no |
| seconv, binary compare | a literal `*` inline in the text | no | no | yes, with a database fetched separately | no | **no** — human-trained, a person typing corrections during a pass | no |
| pgsrip | no | no | no | yes | no | no | no |
| PgsToSrt | no | no | no | yes | no | no | no |

Two columns there are not in [#131][issue-131]'s original scope and were added because the axis the
argument rests on was missing from the table that carries it.

**Aimed at a typeface.** Subtitle Edit has a font-driven nOCR trainer — `NOcrTrainer.cs`, whose own
doc comment describes rendering characters with real fonts, white fill and black outline, "like
typical image subtitles" — with settings for font list, size, bold, italic, segment count and
ligature-prone merged pairs. That is a direct analogue of `gen-reference`, and #131's framing of "a
person and a GUI session per typeface" is right about the session and wrong about the person: nobody
types a correction, they tick fonts in a list. But it builds its list from `FontHelper.GetSystemFonts()`
— there is no path that takes a `.ttf` — it exists only as an Avalonia window with no `seconv` flag,
and it trains *additively*, generating segments only for glyphs the database does not already know,
so a second typeface yields a union rather than a second candidate. The three Tesseract wrappers
cannot do this in any sense a user would recognise: `-l` and `--tesseractlanguage` choose a *language
model*, not a typeface, and LSTM Tesseract is font-agnostic by design.

**Ranks candidates against the track.** Nobody current. The precedent is twenty years old and worth a
paragraph, because "nobody else does this" is weaker and less interesting than "somebody did, and
here is the mechanism that makes ours different". SubRip's release notes describe *Matrix
AutoDetection* over a `ChMatrix` folder, and explicitly recommend "multiple small Matrix files (one
Matrix per font)" — `--references ./sets`, two decades early.

| | SubRip AutoDetect | `subtrackt fit` |
| :--- | :--- | :--- |
| Selector | first matrix clearing a `MatchSet` threshold, with an option to try the last known-good first | every candidate scored; lowest mean distance per glyph wins |
| Unmatched glyphs | not charged; they are what the threshold counts up to | charged the 51-cell ceiling |
| Output | a selection | a ranked table, and with `-o` the selection |
| Says what it cannot do | no | *"`fit` proposes; it does not certify"* |

First-past-the-post picks *an adequate* matrix; ranking picks *the closest*. SubRip's threshold
inherits the blind spot [`fit-confidence.md`](fit-confidence.md) documents — a systematically wrong
set is by construction a low-distance one — and adds one of its own: it stops looking the moment
something clears the bar, so a better candidate two files down is never scored.

**The `*` is the finding that sharpens prediction 9 rather than breaking it.** `NOcrOcrEngine.cs`
emits a literal `*` into the output where `_db.GetMatch` returns null, so "zero for every competitor
on every item" does not survive as written. It survives where it matters, and the line is worth
drawing precisely: a `*` inline in the subtitle text is a **marker, not a count**. A caller cannot
gate on it without parsing the output, and cannot distinguish it from a `*` the disc actually
displayed. The counts are reported above beside subtrackt's, rather than letting that row read as a
zero it is not.

## Where the comparison is unfair, and to whom

**Unfair to the competition:**

- **The fixture is accent-dense and their headline model was English.** The largest single
  distortion in the document, and the one that was measured rather than caveated: the right language
  models are worth **1.9 points** to `seconv-tesseract` on that row (9.1% to 7.2%). It does not
  change the ordering, and it was worth knowing that before publishing a 4x claim that is really 3x.
- **`subtrackt-fitted` chose from six candidates built from the same six typefaces the project has
  always used**, one of which is the disc's actual typeface. That is the intended workflow, but it is
  a luxury none of the competitors were offered, because none of them can accept it.
- **The discs are Arial titles.** All three were chosen years ago because they fit Arial, which is
  the typeface this pipeline is best at. A Tesseract engine does not care what the typeface is; we
  care enormously.
- **`seconv`'s corrector was off in its headline arm**, because that is its default. Table 6 shows it
  is worth nearly a point on Cloverfield, and turning it on would have made seconv's headline row
  better.

**Unfair to us:**

- **The paired gap is biased toward zero.** Where the sidecar differs from the disc, an engine that
  also misreads that span is charged once rather than twice, so the measured gap understates whoever
  is ahead.
- **Two of the discs are scored against a transcript from a different release.** Gone Girl's sidecars
  come from a different release than the rip.

**Unfair to nobody, and worth saying:** every engine read byte-identical input, was scored by one
instrument against one frozen transcript, ran in one container with the same limits, and produced
byte-identical output across three repeats.

### Predictions, scored

Committed before anything ran. Four are lost, one had its premise falsified before the run started,
and the one the project would be most damaged by losing survives with a sharper edge than it was
written with.

| # | prediction | verdict |
| :--- | :--- | :--- |
| 1 | 20x faster per cue than the fastest Tesseract tool, ratio larger in CPU than wall | **Right, and by a distance.** 48–79x wall, 110–307x CPU. Both clauses |
| 2 | subtrackt under 60 MB; .NET tools over 200 MB; Tesseract tools over 300 MB; ratio 5x | **Lost, three clauses of four.** subtrackt reaches **214 MB** on Gone Girl, `seconv-tesseract` only **158 MB**. Only the 5x ratio survives, via pgsrip's 1,259 MB |
| 3 | We lose the fixture — a Tesseract tool under 3% CER | **Wrong.** Best Tesseract is **9.1%**, and **7.2%** given the right language models, against **2.3%** |
| 4 | We win the discs when fitted, gap under 3 points somewhere | **Lost at face value, and inconclusive on the evidence.** A competitor leads on all three, by 0.2–0.5 points — every gap far inside the 3.4–14.3 point sidecar spread |
| 5 | We lose out of the box | **Right.** Liberation Sans reads 9.0–10.5% where the best Tesseract wrapper reads 0.6–0.8% |
| 6 | nOCR and binaryocr cannot run unattended at all | **Premise dead before the run; conclusion right beyond expectation.** Both ran unattended. Both read **100.0% CER** — a generic Latin database matched *not one glyph* on any item |
| 7 | #328's 55–102 ms/frame transfers within 2x | **Right.** `seconv-tesseract` at **76–88 ms/cue** wall, 99–114 ms CPU, stable across titles from 1988 to 2016 |
| 8 | Cue counts agree within 1% across engines | **Lost, once.** pgsrip drops 69 of Wanda's 1,396 cues — **4.9%**. Everywhere else exact |
| 9 | Zero machine-readable "could not read" for every competitor | **Right for the three Tesseract wrappers — literally zero across 24,267 cues.** Sharpened for Subtitle Edit's matchers, which emit a `*` inline: 24,267 of them, one per cue, a marker rather than a count |
| 10 | Every competitor over 100 MB; subtrackt plus a set under 2 MB | **Both halves lost, narrowly.** subtrackt plus a set is **2.09 MB**; `seconv` is 90.7 MB and PgsToSrt 21.3 MB before its runtime |

**Prediction 4 is the one worth dwelling on.** It was written expecting a win, and the run produced a
loss that the instrument cannot certify either. That is a more useful result than either: it says the
release-sidecar method has reached its resolution limit, and that separating these engines on real
discs needs ground truth this project does not have. `scripts/truth/` — 300 cues of Wanda read off the
disc by eye — is the shape of what would answer it.

## What it settles

**The README's argument is about the failure mode, and the failure mode is where the gap is.** Across
24,267 cues, the three Tesseract wrappers reported **zero** glyphs they could not read. Not few —
zero, because there is no mechanism in any of them to say so. This pipeline reported 651, each with a
location, and refused a whole track outright when pointed at a Times New Roman set: 77.8% of glyphs
matched against a 90% floor, an error rather than 47.3% of confidently wrong text. That is the entire
claim `README.md` has been making without a measurement, and it is now measured.

**The accuracy argument is much weaker than the project has been implying.** On real Blu-rays a
Tesseract wrapper reads within half a point of this pipeline, and on two of three discs it reads
*better*. The gaps are inside the instrument's uncertainty, so nobody wins — but "nobody wins" is a
long way from what a reader would have inferred. Only the fixture separates them, and the fixture is
ours.

**Out of the box we lose, unambiguously.** A generic Liberation Sans set reads 9–10.5% where Tesseract
reads 0.6–0.8%. Anyone who cannot supply the typeface is better served by an OCR engine today. That
sentence is now in `README.md` §Shortcomings.

**The cost argument is entirely intact and is the least interesting one.** 48–79x wall, 110–307x CPU,
37 seconds against 1.4 core-hours for Death Race 2's 37 tracks. But **nOCR is 5 ms/cue against our
1 ms**, so most of that gap is *glyph matching versus neural OCR* rather than anything about this
implementation. Against the same class of algorithm the speed advantage is about 5x, not 60x, and
saying so is the difference between a benchmark and an advertisement.

**A generic glyph database is worthless, and that is the strongest vindication in the document.**
`Latin.nocr` and `Latin.db` ship from the same project, are freely obtainable, and read **100.0% CER**
on every item — `*` for every glyph of all 24,267 cues. `docs/reference-set.md` refuses to embed a
reference set on exactly this reasoning, and here is the same decision taken by someone else and
measured: a database that is nobody's typeface reads nothing at all.


## Reproducing

```console
$ python scripts/alternatives/bench.py build
$ python scripts/alternatives/bench.py floor
$ python scripts/alternatives/bench.py cold-warm
$ python scripts/alternatives/bench.py run --repeats 3
$ python scripts/alternatives/analyse.py score
$ python scripts/alternatives/analyse.py tables
```

The corpus is not shareable and is not baked into the image: `bench/corpus/` is populated locally
from the media share, and `corpus.json` names every file and pins every sidecar by SHA-256. This is
**not run in CI** — Docker, about 1.5 GB of images, network downloads at build time and copyrighted
media — for the same reasons the library sweep is not.
