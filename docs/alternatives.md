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
timings and no accuracy, this project had accuracy and no comparison. This closes both, on the same
media, through one instrument.

[issue-328]: https://github.com/sovereign-media/sovereign/issues/328
[issue-131]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/131
[issue-209]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/209

## What this can and cannot claim

**Permitted:** *on this track, against this transcript, A is X points ahead of B.*
**Forbidden:** *B's error rate is X%.*

Both sentences belong here, and the reason is that 23 of the 24 scored items are scored against a
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
sidecar in a title's own folder moves the number by up to **81 points** on this corpus — that is
Rambo: First Blood Part II, whose two candidate sidecars read 0.9% and 82.3%. Any inter-engine gap
smaller than that title's spread is reported as too close to call, and the spread is printed beside
the gap rather than left for a reader to go looking for.

**Breadth is what keeps that caveat from being fatal.** Nine of the 24 titles have only one candidate
sidecar and therefore a spread of zero, which sounds like precision and is not — it means the
instrument has nothing to say about its own uncertainty there. Per title, the honest verdict on most
of this corpus is *inconclusive*. What 24 titles buy is a verdict that does not depend on any one of
them: a 2-7 record across titles is a statement no single title's spread can undo.

## Method

Every engine reads the same flat `.sup`. `xtask dump-sup` is byte-exact — `docs/error-census.md`
records that extracting the rip and extracting the dump produce byte-identical subtitles — so no
demux difference can contaminate an accuracy or a timing figure. Nothing here is given an `.mkv`.

Each measurement is one container: `--network none`, fixed `--cpus` and `--memory`, corpus mounted
read-only, outputs to a tmpfs and copied out only after `time -v` has exited. Strictly serial, and
round-robin by repeat so drift hits every engine equally. `scripts/alternatives/` holds the harness;
`bench.py` has the four reasons it is a sibling of `scripts/accuracy/sweep.py` rather than a flag on
it, the first of which is that `sweep.py` defaults to four workers and a stopwatch inside a parallel
script is how a benchmark starts lying quietly.

### One run for accuracy, five items repeated for cost

Every engine here is deterministic: **every repeated (engine, item) pair produces byte-identical
output**, so a repeat buys no accuracy resolution at all. Repeating all 26 items three times would
cost nine hours and return the median of three identical numbers.

Repeats are not worthless, though. One engine's cost is genuinely unstable:

| unit | wall, three runs | spread |
| :--- | :--- | ---: |
| `pgstosrt--deathrace2` | 254.8 / 67.5 / 67.3 s | **278%** |
| `pgstosrt--clover` | 108.2 / 42.5 / 43.4 s | **152%** |
| `pgstosrt--wanda` | 254.7 / 235.6 / 77.9 s | **75%** |

CPU-seconds move with wall — 996 s against 266 s on Death Race 2 — so this is a four-fold difference
in work actually done rather than stopwatch noise, and PgsToSrt's `OMP_THREAD_LIMIT=1` arm is stable
at 8.5%, which points at thread scheduling. Every other engine's median spread is 2.6%. A flat
`--repeats 1` would have published that engine's cost as a lottery ticket, wrong by up to 3.8x, with
nothing in the data to reveal it.

So the repeat count is a property of the item. Five items carry `cost: true` in `corpus.json` and are
repeated; **every wall, CPU, %CPU and RSS figure in this document is drawn from those five and from
nowhere else**, and the tables name the count behind each. Every other item runs once. The
determinism section says over how many pairs it checked, because a byte-identity claim covering items
measured a single time would be vacuous rather than merely narrow.

Scoring runs on the host, outside every timed region, through the same `xtask srt-score` that
produced every accuracy figure this project has published. **One instrument for every row** is the
whole method: a figure produced by a second code path would have to be independently
trusted, and nothing would be able to say whether a gap was the engines or the scorers.

### The sidecar is chosen once, and frozen

`scripts/accuracy/analyse.py` picks the best-agreeing sidecar per title. That rule is right there and
would be poison here: applied per engine it lets each engine be scored against whichever transcript
flatters it most, which is the most effective way to produce a wrong ranking that looks careful.

So the sidecar was chosen **once**, by one engine (`subtrackt-arial`), before any other engine ran,
and then frozen into `corpus.json` with its SHA-256. What the selector saw is recorded beside the
choice, and the spread between candidates is the instrument's own uncertainty on that title.

**The best-agreeing sidecar is not good enough on its own, and #175 is why.** A Fish Called Wanda was
scored for months against a sidecar carrying none of its 85 bracketed sound cues; more than half its
measured error was the missing cues. Airplane! is worse — the disc renders sound cues as brackets and
its own SDH sidecar renders them as musical notes, so neither candidate matches and it cannot be
scored at all. For six items that check was done by eye. For twenty it is `select.py`, and the
best-agreeing candidate now has to pass a **shape check** before it is used. Three tests, and the
third exists because the first two are blind to a whole class of mismatch:

| test | catches | threshold |
| :--- | :--- | :--- |
| cues paired | a sidecar for a different edit | more than 10% of extracted cues unpaired |
| sound-cue counts | SDH against dialogue-only, brackets against musical notes | the two sides within 2x, once either passes 20 |
| confident read | two transcripts of the same film from **different releases** | over 99% of glyphs read at a fit under 12, and still over 15% CER |

Every threshold is a fraction of something measured, per the house rule.

The first two tests read *structure*, and structure survives a garbled extraction. What they cannot
see is a sidecar in the same convention from a different release: it pairs by timing and carries the
same bracketed lines while sharing almost no words. *Insomnia* is exactly that, and it passed both
tests reading **77.2%** — 1,213 extracted cues against a 1,974-cue sidecar in block capitals.

The third test is [`library-accuracy.md`](library-accuracy.md)'s own measurement turned into a rule.
That document found "12 of 47 titles read more than 99% of their glyphs confidently, at a good fit,
and still score over 15% CER", named *Insomnia* at 77.3% as the case, and concluded that "a title
read confidently and scored badly is evidence about two transcripts, not about the matcher."

**What makes this legitimate rather than circular is that the third test is conditional on the
matcher being sure.** A title the reference set fits badly is kept no matter how badly it scores —
Excision is in the corpus at 22.9% selector CER on a fit of 30.5. Titles an engine finds hard are
kept; only sidecars transcribing a different thing are dropped. The cost is stated rather than
hidden: admitting titles on the selector's own confidence under-represents a title no reference set
can read, relative to one Arial merely finds hard.

Seventeen of the forty records the draw walked were rejected, and each is printed with its reason.
A draw that walked past that many silently would read as twenty titles from the library when it is
not.

### Where the titles came from

The six original items were chosen years apart for particular reasons. The rest are drawn from the
fifty-title library sample behind [`library-accuracy.md`](library-accuracy.md), by that document's
own hash-of-folder rule — ordering independent of year, size, codec and of anything the pipeline
does — so the draw re-derives rather than being hand-picked.

Not from the 21 titles the sidecar corroborates. That restriction reads as the careful choice and is
the opposite: `library-accuracy.md` calls it *circular*, "since it selects titles by the outcome
being measured", and a corpus assembled that way would flatter every engine by an amount nobody
could estimate.

VOBSUB titles are skipped. Every engine here reads the same flat `.sup`, which holds PGS and nothing
else, and handing one engine an `.mkv` while the rest get a `.sup` would reintroduce exactly the
demux difference this corpus exists to exclude. The other codec is measured on `scripts/bench/`,
where #140 put two entries for that reason.

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

**26 items, 24 of them scored, 33,755 cues, spanning 1964 to 2025.** Six were chosen by hand for
particular reasons; the other twenty were drawn by `select.py` from the library sample.

| item | title | cues | truth |
| :--- | :--- | ---: | :--- |
| `fixture` | generated | 500 | **true**, `xtask make-fixture --repeat 50` |
| `clover` | 10 Cloverfield Lane (2016) | 822 | pinned sidecar · `cost` |
| `wanda` | A Fish Called Wanda (1988) | 1,396 | pinned sidecar · `cost` |
| `gonegirl` | Gone Girl (2014) | 2,442 | pinned sidecar · `cost` |
| `bighero6` | Big Hero 6 (2014) | 1,745 | **none — smoke** · `cost` |
| `deathrace2` | Death Race 2 (2010) | 1,184 | none — arithmetic · `cost` |
| `goldfinger` | Goldfinger (1964) | 1,137 | pinned sidecar |
| `therescuers` | The Rescuers (1977) | 970 | pinned sidecar |
| `rambofirstbloodp` | Rambo: First Blood Part II (1985) | 442 | pinned sidecar |
| `batman` | Batman (1989) | 1,108 | pinned sidecar |
| `theparenttrap` | The Parent Trap (1998) | 1,751 | pinned sidecar |
| `theblairwitchpro` | The Blair Witch Project (1999) | 1,341 | pinned sidecar |
| `toystory2` | Toy Story 2 (1999) | 1,188 | pinned sidecar |
| `highfidelity` | High Fidelity (2000) | 1,934 | pinned sidecar |
| `moulinrouge` | Moulin Rouge! (2001) | 1,500 | pinned sidecar |
| `winniethepoohave` | Winnie the Pooh: A Very Merry Pooh Year (2002) | 840 | pinned sidecar |
| `ironman` | Iron Man (2008) | 1,339 | pinned sidecar |
| `thor` | Thor (2011) | 1,191 | pinned sidecar |
| `excision` | Excision (2012) | 905 | pinned sidecar |
| `divergent` | Divergent (2014) | 1,635 | pinned sidecar |
| `shazam` | Shazam! (2019) | 2,155 | pinned sidecar |
| `littlewomen` | Little Women (2019) | 1,664 | pinned sidecar |
| `howtotrainyourdr` | How to Train Your Dragon: The Hidden World (2019) | 1,545 | pinned sidecar |
| `hearteyes` | Heart Eyes (2025) | 1,898 | pinned sidecar |
| `liloandstitch` | Lilo & Stitch (2025) | 1,985 | pinned sidecar |
| `ifihadlegsidkick` | If I Had Legs I'd Kick You (2025) | 2,067 | pinned sidecar |

`cost` marks the five repeated items, the only ones any timing figure comes from. Big Hero 6's 1,745
bitmaps reproduce sovereign#328's count exactly, which is the cross-check that the two analyses are
looking at the same track.

**The one title that matters more than the others is `excision`**, and it is worth naming here rather
than leaving to Table 3. It is set in **Arial Bold**, the reference directory carried no bold cut, and
it accounts for 2,666 of the corpus's 4,434 unread glyphs on the single-Arial arm — more than every
other title combined. It is also the only track any engine refused outright. A corpus of three discs
chosen for fitting Arial contained nothing like it.

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

Before a single figure is quoted, the harness has to reproduce what this project already
publishes. It does — and finding out cost two published figures, because they had gone stale.

| check | result |
| :--- | :--- |
| **Reproduce Cloverfield** — same set, flags and sidecar, through the container | **0.4% / 0.8%** over **818** paired cues at **99.9%** coverage and **98.8%** italic agreement. The *shape* matches the published figure exactly; the *rate* does not — see below |
| **Reproduce the fixture ceiling** — `xtask accuracy` vs the CLI scored against the new `synthetic.srt` | Both **0.0%** over the 328-character single pass. The two paths agree, which is what this check exists to test |
| **Identity** — score a file against itself | 0.00% CER/WER, 0 unpaired |
| **Empty** — empty SRT against a sidecar | 100% CER, no crash |
| **Determinism** — SHA-256 of every output across repeats | **Byte-identical on all 69 repeated (engine, item) pairs.** No engine needed a median, which is what licensed one run per accuracy item |
| **Alignment agreement** — `(scale, offset_ms)` per engine per title | **Every engine agrees on 23 of 24 titles.** Goldfinger is the exception: pgsrip aligns at `+6200 ms` where the other nine agree on `+6050`, a 150 ms disagreement on a title everyone reads at 8.7–9.9%. No engine gained a pairing advantage anywhere else, and no `srt-score` flag was needed |
| **Nothing downloaded** — `--network none` on all 380 units | All 380 completed. Nothing was fetched at run time |
| **Failures** | **1 of 380, and it is a result rather than a failure.** `subtrackt-liberation` refused Excision at the threshold gate — 72.2% of glyphs read against a 90% floor — and is recorded with its status rather than dropped from a pooled figure. Every other engine completed every item |
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

That is a configuration *we* chose for the alternatives, so it was re-run rather than argued about:

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

### Table 3 — accuracy over 24 titles, against what it cost to get there

33,755 cues. Track-level CER per engine, as a distribution rather than a column per title — and the
cost of getting there, because an accuracy table with no cost column silently prices a 200x
difference at zero.

| engine | mean | median | p90 | worst | CPU s | vs subtrackt |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| seconv, Tesseract (best) | **2.67** | 1.81 | **6.52** | **10.0** | 1,104 | 221x |
| seconv, Tesseract (apt) | 2.78 | 2.04 | 6.78 | 9.1 | 828 | 166x |
| subtrackt, fitted from 128 | 3.27 | 2.33 | 7.74 | 11.4 | 7.2 | 1.4x |
| **subtrackt, Arial / fitted** | 3.53 | **1.81** | 7.67 | 24.8 | **5.0** | — |
| PgsToSrt | 3.73 | 1.81 | 9.87 | 27.3 | 2,217 | 443x |
| pgsrip | 7.78 | 5.95 | 18.91 | 22.3 | 864 | 173x |
| subtrackt, Liberation | 10.97 | 10.54 | 13.55 | 16.8 | 5.1 | 1.0x |
| seconv, nOCR | 96.93 | 96.97 | 97.39 | 97.5 | 40 | 8x |
| seconv, binary compare | 96.93 | 96.97 | 97.39 | 97.5 | 42 | 8x |

CPU is the sum over the five repeated `cost` items, which is the only set any timing figure may come
from. `subtrackt, fitted` is byte-identical to `subtrackt, Arial` on every item and shares its row.

**Both Tesseract arms in Subtitle Edit beat subtrackt on mean and on worst case, and the head-to-head
resolves against us: 2 wins, 7 losses, 15 too close to call.** The corpus is drawn from the library
by a rule blind to what any engine makes of a title, which is what the verdict rests on: a corpus
assembled from titles that suit one engine will flatter it by an amount nobody can estimate.

Two things about *how* it lost, both of which matter more than the ranking.

**The loss is one title.** Remove Excision and subtrackt's mean is 2.61 against Tesseract-best's
2.71, and its worst case is 8.7 against 10.0 — it wins both. That is not a licence to remove it, and
this document does not: a single catastrophic title is exactly the failure mode a library owner
cares about, and [the bench roster's rule](../CLAUDE.md) is to read the `worse` column rather than
the average. But it does say what the fix is, and it is not "make the matcher better".

**Excision is set in Arial Bold.** The 128-candidate arm identifies it immediately — `fit` ranks
`arialbd` at 11.5 with 99.6% of glyphs read — and takes the title from **24.8% to 11.4%** and its
unread glyphs from **2,666 to 52**. Across the whole corpus the single-Arial arm leaves 4,434 unread
glyphs and 2,666 of them are that one film, which had no bold cut to match against. So the corpus's
worst result is a *missing reference set*, not a limit of glyph matching — and it is the concrete
form of the Shortcoming [`reference-set.md`](reference-set.md) already names, that nothing is
embedded and the user must bring a set.

The wide arm does not simply win, and the reason is worth stating because it is fixable too.
`gen-reference` over a directory writes one set per font file, so `arialbd` and `ariali` become
separate sets rather than cuts of one, and the arm carries **no italic pairing** where the curated
`arial-ri` set does. It pays for the typeface win on the italic-heavy titles — Gone Girl 1.3% to
2.3%, Cloverfield 0.8% to 1.0% — which is why its median is worse while its worst case is less than
half. An arm that could do both is a set builder that pairs cuts, not a different matcher.

**What survives the widening, and what does not.**

The consistency claim survives, and it now has a number rather than an anecdote. Every alternative tool's
p90 is more than three times its median — 3.3x, 3.6x, 3.2x and 5.5x — so no engine tested is good on
all 24 titles. pgsrip is the clearest case: median 5.95 with a p90 of 18.9, reading How to Train Your
Dragon at 22.3% and Heart Eyes at 19.9%. PgsToSrt shares subtrackt's median of 1.81 exactly and then
reads Lilo & Stitch at **27.3%**, the worst single result of any working engine.

The cost claim survives untouched and gets larger. Reading the five repeated items costs subtrackt
**5.0 CPU-seconds** against the cheapest alternative's **828** — 166x — and PgsToSrt's 2,217, which is
443x. Nothing here trades accuracy for speed at a rate that makes the alternative tools' cost look bought:
the two engines that beat us on accuracy do so by 0.9 and 0.8 points of mean CER, for two orders of
magnitude more CPU.

So the defensible sentence has changed, and it is shorter than the one it replaces: *on 24 titles
drawn from a real library, Subtitle Edit's Tesseract arms read more accurately than this pipeline by
about a point of mean CER, at 166–221x the CPU; the whole of the gap is one film whose typeface was
not in the reference directory; and no engine tested is consistent across the corpus.*

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

**Wall clock alone would flatter the alternatives badly.** PgsToSrt looks 2.6x faster than
`seconv-tesseract` in wall clock and costs **1.9x more CPU**; it runs at 394% on a four-CPU
container. subtrackt is single-threaded at 85%. The wall figure is what a user feels and the CPU
figure is what a queue pays, so both are here.

**sovereign#328's cost model transfers.** It measured Tesseract at 55–102 ms/frame on 1080p PGS;
`seconv-tesseract` lands at **79–88 ms/cue wall** and 99–114 ms CPU across the five repeated items.
The per-cue rate is stable to within about 10% across titles from 1988 to 2016.

The five repeated `cost` items, 7,589 cues — the only set a timing figure may be summed over:

| engine | wall | CPU | vs subtrackt, CPU |
| :--- | ---: | ---: | ---: |
| **subtrackt, Arial / fitted** | **5.9 s** | **5.0 s** | — |
| subtrackt, Liberation | 5.9 s | 5.1 s | 1.0x |
| subtrackt, fitted from 6 | 8.9 s | 6.6 s | 1.3x |
| subtrackt, fitted from 128 | 10.2 s | 7.2 s | **1.4x** |
| seconv, nOCR | 39.6 s | 39.8 s | 8x |
| seconv, binary compare | 41.9 s | 42.1 s | 8x |
| PgsToSrt | 559.9 s | **2,217.2 s** | **443x** |
| pgsrip | 592.8 s | 864.2 s | 173x |
| seconv, Tesseract (apt) | 646.1 s | 827.8 s | 166x |
| seconv, Tesseract (best) | 822.9 s | 1,103.7 s | 221x |

**Fitting is cheap and scanning 128 candidates is barely dearer than scanning six.** The whole
fitting apparatus — the thing that takes Excision from 24.8% to 11.4% — costs 2.2 CPU-seconds across
five films, and is still 118x cheaper than the cheapest alternative. Whatever the argument against
building reference sets per title is, it is not the price of choosing one.

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
| subtrackt, fitted from 128 | 216 MB | 218 MB | 4% |
| seconv, Tesseract | 158 MB | 148 MB | 20% |
| seconv, nOCR | 219 MB | 181 MB | **38%** |
| seconv, binary compare | 221 MB | 184 MB | **36%** |
| PgsToSrt | 678 MB | 634 MB | 7% |
| pgsrip | **1,259 MB** | **1,859 MB** | **33%** |

Maxima over the five repeated items. Holding 128 candidate sets in memory to choose between them
costs **2 MB** over the single-set arm, which is the same answer the cost table gave: choosing a
reference set is not what is expensive about this pipeline.

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

The table the argument actually rests on, and it is not a hypothetical: two alternative tools read
these titles more accurately than SubTrackt does. That is in the headline, and the argument rests
entirely on this table. This is the table.

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

**The last column earns itself on exactly one title, and the size of the effect there is the reason
it is in the table.** Given six candidate sets, `fit` chooses Arial for all 26 items and its arm is
byte-identical to the single-Arial arm — an apparatus doing nothing, because the answer is not in the
directory. Given 128, it identifies Excision as Arial **Bold** at a mean distance of 11.5 with 99.6%
of glyphs read, against 30.5 and 90.1% for the regular cut, taking the title from 24.8% CER to 11.4%
and its unread glyphs from 2,666 to 52. No other engine in this comparison has a mechanism that could have found
that, and the two that read the title correctly did so without being able to report that anything
about it was unusual.

The cost of the mechanism is in Table 4 and it is small: 2.2 CPU-seconds across five films to scan
128 candidates rather than 6, and 2 MB of peak RSS. What it cannot yet do is pair cuts — a directory
of fonts becomes one set per file, so `arialbd` and `ariali` are separate candidates rather than cuts
of one set, and the wide arm therefore has no italic pairing at all. It wins the typeface question
and loses the italic one, which is a set-builder problem rather than a matcher problem.

**The `*` is the finding that sharpens prediction 9 rather than breaking it.** `NOcrOcrEngine.cs`
emits a literal `*` into the output where `_db.GetMatch` returns null, so "zero for every alternative tool
on every item" does not survive as written. It survives where it matters, and the line is worth
drawing precisely: a `*` inline in the subtitle text is a **marker, not a count**. A caller cannot
gate on it without parsing the output, and cannot distinguish it from a `*` the disc actually
displayed. The counts are reported above beside subtrackt's, rather than letting that row read as a
zero it is not.

## Where the comparison is unfair, and to whom

**Unfair to the alternatives:**

- **The fixture is accent-dense and their headline model was English.** The largest single
  distortion in the document, and the one that was measured rather than caveated: the right language
  models are worth **1.9 points** to `seconv-tesseract` on that row (9.1% to 7.2%). It does not
  change the ordering, and it was worth knowing that before publishing a 4x claim that is really 3x.
- **Both fitted arms choose from a directory of candidate typefaces**, and the wide one gets 128 —
  every font on the machine. That is the intended workflow, but it is a luxury none of the
  alternative tools were offered, because none of them can accept it. A Tesseract engine does not
  care what the typeface is; we care enormously, and the gap between our two fitted arms on Excision
  is 13.4 points of exactly that caring.
- **This corpus is still mostly Arial.** `fit` chose regular Arial on 25 of 26 items, so a single
  Arial set is close to optimal almost everywhere here — which is a fact about the library rather
  than a favour to us, but it is a fact that favours us: a single Arial set gives up only **0.6**
  points of median CER against the three titles it was tuned on. A pipeline that must be aimed at a
  typeface benefits from that uniformity in a way an OCR engine does not.
- **`seconv`'s corrector was off in its headline arm**, because that is its default. Table 6 shows it
  is worth nearly a point on Cloverfield, and turning it on would have made seconv's headline row
  better.

**Unfair to us:**

- **The paired gap is biased toward zero.** Where the sidecar differs from the disc, an engine that
  also misreads that span is charged once rather than twice, so the measured gap understates whoever
  is ahead.
- **Many titles are scored against a transcript from a different release.** Nine of the 24 have only
  one candidate sidecar, so there was no choice to make and no spread to report; the selector's shape
  check confirms the convention matches but cannot confirm the release does.
- **Excision is scored, and it is our worst result by a factor of three.** It could have been
  excluded on the grounds that the reference directory had no bold cut and the failure is therefore a
  missing input rather than a matcher limit. It was not, because that is the argument every vendor
  makes about its worst case. Without it we win the mean and the worst case; with it we lose both,
  and the number with it is the one quoted.

**Unfair to nobody, and worth saying:** every engine read byte-identical input, was scored by one
instrument against one frozen transcript, ran in one container with the same limits, and produced
byte-identical output on every repeated item — 69 (engine, item) pairs, no exceptions.

### Predictions, scored

Committed before anything ran. Four are lost, one had its premise falsified before the run started,
and the one the project would be most damaged by losing survives with a sharper edge than it was
written with.

| # | prediction | verdict |
| :--- | :--- | :--- |
| 1 | 20x faster per cue than the fastest Tesseract tool, ratio larger in CPU than wall | **Right, and by a distance.** 48–79x wall, 110–307x CPU. Both clauses |
| 2 | subtrackt under 60 MB; .NET tools over 200 MB; Tesseract tools over 300 MB; ratio 5x | **Lost, three clauses of four.** subtrackt reaches **214 MB** on Gone Girl, `seconv-tesseract` only **158 MB**. Only the 5x ratio survives, via pgsrip's 1,259 MB |
| 3 | We lose the fixture — a Tesseract tool under 3% CER | **Wrong.** Best Tesseract is **9.1%**, and **7.2%** given the right language models, against **2.3%** |
| 4 | We win the discs when fitted, gap under 3 points somewhere | **Lost.** 2 wins, 7 losses and 15 too close to call against Subtitle Edit's Tesseract arms, which lead on mean and worst case both |
| 5 | We lose out of the box | **Right.** Liberation Sans reads **10.97% mean** across 24 titles against Tesseract-best's 2.67%, and it is the only arm that refused a track outright |
| 6 | nOCR and binaryocr cannot run unattended at all | **Premise dead before the run; conclusion right beyond expectation.** Both ran unattended. Both read **100.0% CER** — a generic Latin database matched *not one glyph* on any item |
| 7 | #328's 55–102 ms/frame transfers within 2x | **Right.** `seconv-tesseract` at **76–88 ms/cue** wall, 99–114 ms CPU, stable across titles from 1988 to 2016 |
| 8 | Cue counts agree within 1% across engines | **Lost, once.** pgsrip drops 69 of Wanda's 1,396 cues — **4.9%**. Everywhere else exact |
| 9 | Zero machine-readable "could not read" for every alternative tool | **Right: literally zero from all four Tesseract engines across 33,755 cues**, against our 4,434 located markers and one refused track. Sharpened for Subtitle Edit's matchers, which emit a `*` inline: 52,914 of them, one per cue, a marker rather than a count |
| 10 | Every alternative tool over 100 MB; subtrackt plus a set under 2 MB | **Both halves lost, narrowly.** subtrackt plus a set is **2.09 MB**; `seconv` is 90.7 MB and PgsToSrt 21.3 MB before its runtime |

**Prediction 4 is the one worth dwelling on.** It was written expecting a win and it lost, but the
useful part is what makes the loss *sayable at all*. Per title it still is not: every margin here
sits inside its title's sidecar spread, so no single disc decides anything. What decides it is
breadth. A 2-7 record across 24 titles is a statement no one title's spread can undo, and it needs no
better truth per disc to hold. `scripts/truth/` — cues transcribed by eye — is the right instrument
for "what is the true CER of this one disc"; it is the wrong one for "which engine reads a library
better", and that distinction is worth more than the prediction was.

### Corpus predictions, scored

Committed in [#209][issue-209] before the corpus was assembled or a figure was seen.

| # | prediction | verdict |
| :--- | :--- | :--- |
| 1 | The head-to-head resolves: some engine wins a majority of titles by margins surviving the sidecar spread | **Half right, and the half that failed is the interesting one.** It resolves — 2-7 against us is not ambiguous. But *nobody* wins a majority outright against all nine others: PgsToSrt takes 5 titles, the Tesseract arms 1 each, subtrackt 0. The corpus separates the engines pairwise and still finds no dominant one |
| 2 | subtrackt-arial degrades by more than 5 points of median CER; subtrackt-fitted by less than 2 | **Lost on both clauses, for two different reasons.** Arial degraded **0.6 points**, not 5 — a single Arial set holds up across a library far better than the "chosen because they fit Arial" framing implied. And the second clause was unmeasurable: fitted was byte-identical to Arial on all 26 items |
| 3 | Every Tesseract wrapper has a p90 more than 3x its median | **Right, all four.** 3.3x, 3.6x, 3.2x, 5.5x. No engine tested is good on all 24 titles |
| 4 | The cost subset stays byte-identical across its repeats | **Right.** 69 (engine, item) pairs, every one identical |
| 5 | pgstosrt remains the only engine with a cost spread over 20% | **Right.** 152%, 75% and 278% on the three items where it moves; every other engine inside 20% everywhere |

**Prediction 2 is the one that taught something.** It assumed the original three discs were an easy
corpus for a single Arial set and that widening would punish it. The punishment was 0.6 points of
median. What actually punished the Arial arm was not typeface *diversity* across the library but one
title in a *cut of Arial the set did not contain* — a bold face, worth 13.4 points on that title and
nothing anywhere else. Breadth found it; more of the same three discs never would have.

## What it settles

**The README's argument is about the failure mode, and the failure mode is where this pipeline is
furthest ahead.** Across 24 titles and 33,755 scored cues, the
four Tesseract-based engines reported **zero** glyphs they could not read. Not few — zero, because
there is no mechanism in any of them to say so. This pipeline reported **4,434**, each with a
location, and on Excision it went further: pointed at a Liberation set it **refused the track**,
72.2% of glyphs matched against a 90% floor, an error rather than a file of confidently wrong text.
pgsrip read that same track and returned clean-looking subtitles with no indication of anything. That
is the entire claim `README.md` has been making without a measurement, and twenty-four titles measure
it more convincingly than three did.

**The accuracy argument is weaker than the failure argument, and this is where it is weakest.** Both
Tesseract arms in Subtitle Edit read these titles better than SubTrackt: 2.67% and 2.78% mean against
3.53%, and 10.0% and 9.1% worst case against 24.8%. Head to head it is **2 wins, 7 losses, 15 too
close to call**. The corpus is drawn from the library by a rule that never consults an engine's
output, so the verdict is not an artefact of which titles were chosen.

**But the loss is one film, and the fix is a reference set rather than a matcher.** Excision is
authored in Arial Bold. The reference directory held regular and italic cuts only, so 2,666 of the
corpus's 4,434 unread glyphs are that single title. Given 128 candidates to choose from, `fit` names
`arialbd` at once and the title goes from 24.8% to 11.4%. Excluded, our mean is 2.61% against
Tesseract-best's 2.71% and our worst case 8.7% against 10.0% — we win both. It is not excluded here,
because one catastrophic title is exactly what a library owner is entitled to care about. What the
number says is that the ceiling is a set-building problem, and `reference-set.md` already names it as
the Shortcoming.

**No engine tested is consistent across the corpus, and that claim is now a statistic rather than an
anecdote.** Every alternative tool's p90 is more than three times its median: 3.3x, 3.6x, 3.2x, 5.5x.
PgsToSrt shares our median of 1.81% exactly and then reads Lilo & Stitch at **27.3%**, the worst
single result of any working engine. pgsrip reads How to Train Your Dragon at 22.3% and Heart Eyes at
19.9%. The corpus does not name an alternative tool that is simply better; it shows that every
engine here, including this one, has titles it falls over on.

**Out of the box we lose, unambiguously, and by more than before.** A generic Liberation Sans set
reads 10.97% mean against Tesseract's 2.67%, and it is the only arm that failed a track outright.
Anyone who cannot supply the typeface is better served by an OCR engine today. That sentence is in
`README.md` §Shortcomings and the wider corpus has not softened it.

**The cost argument is entirely intact and larger than it was.** Reading the five repeated items
costs subtrackt **5.0 CPU-seconds** against 828 for the cheapest alternative, 1,104 for the most
accurate one and 2,217 for PgsToSrt — **166x, 221x and 443x**. The two engines that beat us on
accuracy do so by about 0.9 points of mean CER for two orders of magnitude more CPU. That is a price,
not a trade. But **nOCR is 5 ms/cue against our 1 ms**, so most of the gap is *glyph matching versus
neural OCR* rather than anything about this implementation. Against the same class of algorithm the
advantage is about 5x, and saying so is the difference between a benchmark and an advertisement.

**A generic glyph database is worthless, and that is still the strongest vindication in the
document.** `Latin.nocr` and `Latin.db` ship from the same project, are freely obtainable, and read
**97% track CER** on every one of the 24 titles — a literal `*` for every glyph of all 52,914 cues.
`docs/reference-set.md` refuses to embed a reference set on exactly this reasoning, and here is the
same decision taken by someone else and measured: a database that is nobody's typeface reads nothing
at all.

**What breadth settles that depth cannot.** Per title this comparison decides nothing: every margin
sits inside its sidecar spread, and nine titles have no spread to report because they carry one
candidate transcript. Across 24 it decides plainly — against this pipeline on accuracy, for it on
cost and on failure reporting. A verdict that rests on no single title is worth more than a
per-title verdict that rests on a transcript somebody typed, and it costs three hours rather than
nine because repeats sit where the variance is: all 69 repeated (engine, item) pairs are
byte-identical, and the only engine whose *cost* moves is PgsToSrt, at a 278% spread.

## Reproducing

```console
$ python scripts/alternatives/bench.py build
$ python scripts/alternatives/bench.py stage          # dump the corpus `.sup`s from the share
$ python scripts/alternatives/bench.py floor
$ python scripts/alternatives/bench.py cold-warm
$ python scripts/alternatives/bench.py run            # one run per item, three on the `cost` five
$ python scripts/alternatives/analyse.py score
$ python scripts/alternatives/analyse.py tables
$ python scripts/alternatives/analyse.py predictions
```

Re-deriving the corpus itself, which only needs doing when the sample changes:

```console
$ scripts/accuracy/inventory.py --csv image-based-subs-report.csv --out inventory.json
$ scripts/accuracy/sample.py --inventory inventory.json --out sample.json --count 50
$ scripts/accuracy/sweep.py --inventory sample.json --out sweep/ --reference arial-ri.subtref
$ python scripts/alternatives/select.py --sweep sweep/ --count 20 --out picked.json
```

The corpus is not shareable and is not baked into the image: `bench/corpus/` is populated locally
from the media share, and `corpus.json` names every file and pins every sidecar by SHA-256. This is
**not run in CI** — Docker, about 1.5 GB of images, network downloads at build time and copyrighted
media — for the same reasons the library sweep is not.
