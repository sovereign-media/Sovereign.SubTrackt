# Sovereign.SubTrackt

Extract plain text from bitmap image-based subtitle streams — Blu-ray PGS and DVD VOBSUB — without
human intervention, and without a general OCR engine.

**Status: reads real media end to end and picks its own reference set from ones you supply. It
ships with none embedded, on purpose.**

Point it at a Blu-ray rip and it demuxes the Matroska, decodes the PGS, segments the bitmaps into
glyphs and emits timed cues with a confidence tally — 1,111 cues from a 5.5 GB film in 22 seconds.
Out of the box it names none of those glyphs, because nothing is embedded:

```console
$ subtrackt extract 'Dr. No (1962).mkv' --format vtt --on-unmatched placeholder --report
1111 cues from 1111 images (2222 packets); glyphs 0 matched / 35516 unmatched; cache 99%
```

`subtrackt fit` scores a directory of reference sets against the title and proposes the best:

```console
$ subtrackt fit '10 Cloverfield Lane (2016).mkv' --references ./sets -o clover.subtref
400 cues, 10195 glyphs, 134 distinct shapes

  reference set               score       read
  arial-ri                     12.5      96.5%
  arial                        13.6      95.6%
  tahoma                       20.8      93.0%

  score is mean distance per glyph, charging unread glyphs the 51-cell ceiling.
  Lower fits better. Nothing here checks whether the winner is good enough --
  no measured statistic can. Read a few cues before trusting a track to it.
```

Three seconds against a 1.8 GB file, because it samples the first few hundred cues — a typeface does
not change halfway through a film. Then the same pipeline reads it:

```console
$ subtrackt extract '10 Cloverfield Lane (2016).mkv' --reference arial.subtref \
      --on-unmatched placeholder --post-correct --format srt -o out.srt --report
822 cues from 822 images (1644 packets); glyphs 19566 matched / 958 unmatched
  / 3878 ambiguous (95.3% read); fit 11.7; cache 100%; corrections 362 (context)
```

**5.5% character error** on that track's 775 upright cues, scored against the English subtitle
shipped beside the rip — see [`docs/reference-set.md`](docs/reference-set.md), which also says why
that comparison is evidence rather than ground truth.

What is still missing is where the candidate sets come from: `.subtref` files are generated from
fonts by `cargo run -p xtask -- gen-reference`, which needs the repository. Putting a font
rasteriser in the shipped binary would raise the MSRV from 1.85 to 1.87, so that is
[#16](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/16)'s call rather than a thing
to slip in.

See [issue #1][issue-1] for the original design, [`docs/architecture.md`](docs/architecture.md) for
how it is laid out, and the two measurement write-ups below for where the design has moved.

[issue-1]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1

## Why not just run OCR

Because a general OCR engine's failure mode is a confident wrong answer. Tesseract will read a
glyph it has never seen as something plausible and attach a probability to it, and a probability is
not a fact you can gate on.

This is a purpose-built glyph matcher instead. Each glyph is normalised onto a fixed grid, flattened
to a 256-bit vector, and compared against a reference set by Hamming distance. A character the
reference set does not contain comes back as *no match* — detectable, countable, and something a
caller can act on by falling back to burn-in rather than shipping invented text.

That property is the whole argument, and it is why `Confidence` counts glyphs rather than estimating
probabilities.

## What the measurements found

Nothing here was assumed. Each of these was measured — against a real 1,328-title library, or
against ground truth the tool renders itself — and each one changed the design, usually by killing
the plan it was meant to confirm.

**[`docs/library-survey.md`](docs/library-survey.md)** — 56 titles, 1950s–2020s, 149,604 glyphs.
A dominant glyph family runs through the library and one cluster covers 43 of 56 titles across
seventy years. Fitting rendered fonts against it identifies the typeface as **Arial or very close**:
the most frequent extracted shapes land on `i` at distance 0, `t` at 6, `a` at 10, `o` at 13. But a
fixed set covers only **46%** of glyph instances, and "or very close" turned out to be the expensive
half of that sentence — see below.

**[`docs/glyph-stability.md`](docs/glyph-stability.md)** — why. Two renderings of the *same*
character are typically further apart (median 46 cells) than two *different* characters are
(median 31). A one-pixel shift in where the binarization threshold falls costs 30 cells, as much as
character identity itself. Rendering size and anti-aliasing, by contrast, cost 11 and 8 — the axes
the normalisation was built to absorb, and it absorbs them.

**[`docs/post-correction.md`](docs/post-correction.md)** — what is left once shapes are as good as
they get. Resolving the pairs a binarized glyph cannot separate (`0`/`O`, `1`/`l`/`I`) from the
characters around them takes **2.1 points** off the ceiling fixture's character error rate and makes
no line worse. On a real Blu-ray it takes 1.4 points off 818 cues and makes **no cue worse**. It
still ships switched off, and the reason is now narrow: the only comparison available for a real
track is another release's subtitle, which is evidence rather than ground truth.

**[`docs/reference-set.md`](docs/reference-set.md)** — why nothing is embedded, and what to fit
instead. Reading Arial-authored material with a Liberation Sans reference set — metric-compatible,
openly licensed, the obvious thing to ship — costs **11 points of CER**, which is Verdana's cost to
within noise. A shipped set would trade a detectable failure for an undetectable one.

The same document puts the choice to a real disc: ten candidate typefaces over one Blu-ray track,
and the mean match distance the extraction already reports **picks the right one**, 11.7 against
18.5 for the runner-up and 8.8% CER against 16.6%. It also finds the level below typeface — an
italic reference set reads that film's italic act at 10.8% and its upright dialogue at 40.5%, and
the upright set does the exact reverse.

**The consequence.** One reference vector per character cannot work, and enumerating styles does not
rescue it. Clustering a title's own repeated shapes was the obvious escape — the expensive axes are
constant within a stream — and it was built, swept and **measured worse at every radius**: no radius
groups a title's variation without first merging characters the vector never separated, since `I`,
`l` and `|` sit at distance zero from one another.

What did work was aiming at *separation* rather than at variance. Measuring each glyph against its
own text line — how tall it stands relative to that line's cap height, how far it drops below the
baseline — took 5.8 to 8.1 points off the error rate, because it adds the one thing normalisation
deliberately throws away. The reference set still has to come from the material's own typeface,
which is [#43](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/43) and is what stands
between this and working out of the box.

## Usage

```console
$ subtrackt list movie.mkv
  0  hdmv_pgs_subtitle    eng   1920x1080  Full
  1  hdmv_pgs_subtitle    eng   1920x1080  Forced

$ subtrackt fit movie.mkv --references ./sets -o movie.subtref

$ subtrackt extract movie.sup --format vtt --output movie.en.vtt --report
```

Input can be a Matroska container, a raw PGS `.sup` dump, or a VOBSUB `.idx`/`.sub` pair. Output is
SubRip or WebVTT.

There is also `subtrackt glyphs <file>`, which dumps normalised glyph shapes without trying to read
them. Feature vectors are comparable across titles even with no reference set, which is what made
the two measurement write-ups possible.

The flag worth knowing about is `--on-unmatched`, which decides what happens when a glyph cannot be
identified:

| Value | Behaviour |
| :--- | :--- |
| `threshold` | Abort only if fewer than `--min-matched` of glyphs were read. **Default, floor 0.90.** |
| `fail-track` | Abort on a single unread glyph. |
| `drop` | Omit any cue containing an unread glyph. |
| `placeholder` | Emit the cue with a replacement character. |

The floor is a floor and not a target: it catches a track that could not be *read*, and makes no
claim about one read *well*. `fail-track` was the default until it was measured — rejecting on a
*single* unread glyph refused essentially every track in the library, which is a gate that never
opens. 0.90 is bounded from above by the pipeline's own ceiling case at 93.9% and from below by the
48 of 56 surveyed titles that clear it. See
[`docs/architecture.md`](docs/architecture.md#the-accuracy-gate).

## Building

```console
$ cargo build --release
$ cargo test --workspace
```

Rust 1.85 or newer, and no system dependencies. The library crates take exactly one third-party
crate between them — `miniz_oxide`, because 83% of the PGS tracks in a real library are stored
zlib-compressed inside Matroska and refusing it meant failing on most of the library. Everything
else is the standard library, which is what keeps the single-static-binary option in
[#16](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/16) open and cross-compilation
to `linux/arm64` uneventful.

Run `scripts/check.sh` before pushing: it runs what CI runs, including clippy at pedantic and the
1.85 MSRV build, which are the two gates that catch the most.

## As a library

The CLI is a thin shell over `subtrackt::Pipeline`; everything of substance is in the library so
that shipping a `cdylib` instead would replace one crate rather than restructure the workspace.

```rust
use subtrackt::{Config, Pipeline, UnmatchedPolicy};

let config = Config {
    unmatched: UnmatchedPolicy::Threshold { min_ratio: 0.98 },
    ..Config::default()
};

let outcome = Pipeline::new(config).run("movie.sup")?;
println!("{}", outcome.report);
print!("{}", outcome.render(&config)?);
```

## Layout

| Crate | Role |
| :--- | :--- |
| [`subtrackt-core`](crates/subtrackt-core) | Types, errors and stage traits every other crate shares |
| [`subtrackt-demux`](crates/subtrackt-demux) | Containers and sidecar files in, codec packets out |
| [`subtrackt-decode`](crates/subtrackt-decode) | PGS and VOBSUB packets in, indexed bitmaps out |
| [`subtrackt-glyph`](crates/subtrackt-glyph) | Bitmaps in, identified characters out |
| [`subtrackt-text`](crates/subtrackt-text) | Characters in, SRT or WebVTT out |
| [`subtrackt`](crates/subtrackt) | The pipeline, the accuracy gate and the report |
| [`subtrackt-cli`](crates/subtrackt-cli) | The `subtrackt` binary |
| [`xtask`](xtask) | Development tooling: renders fonts into reference sets, measures glyph stability. Not shipped |

## Contributing

Work is tracked as sub-issues of [#1][issue-1], and [`CLAUDE.md`](CLAUDE.md) records the conventions
the project runs on — most of them written down because something broke when they were not followed.

Every stage of the pipeline is built and every measurement issue is answered; their write-ups are in
`docs/` and are the reason the remaining work looks nothing like the original plan. Four things that
were on this list are done and worth knowing about, because each closed by measuring that the plan
was wrong:

- **#10** was to cluster a stream's own shapes and match the centroids. Built, swept, and **shipped
  off** — no radius groups a title's variation without first merging characters the vector never
  separated, since `I`, `l` and `|` sit at distance *zero* from each other.
- **Reducing edge sensitivity in binarization** was the largest cheap term left. Two approaches,
  both **neutral-to-worse**. The lever was the binary mask itself, not where the threshold fell.
- **#9** was to embed a reference set. Measured, and **not worth shipping**: see above.
- **#15** built the scoring harness, which is what turned every claim in this repository from
  coverage into correctness — and immediately showed the two are barely related.

What is actually left, in order of leverage:

- **[#62](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/62)** — fit the reference
  set to the title rather than to the binary. This is the one that makes the tool work out of the
  box, and the half of it that works: selecting by mean match distance costs 0.2 points of CER on
  average across eight fixtures and picked the right typeface out of ten on a real disc. It ships as
  a proposal a user accepts rather than a decision the tool makes, because of the next bullet.
  Style is a scope question inside it — an italic reference set reads one film's italic act at 10.8%
  and its upright dialogue at 40.5%, and the upright set does the reverse, so a fit that is right
  about the typeface can still be wrong about a reel.
- **[#63](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/63)** — whether anything can
  tell a good fit from a bad one. Four statistics say no, and they fail for one mechanism rather
  than four: a systematically wrong set is *by construction* a low-distance one, and a systematically
  shared confusion is *by construction* an agreed one. Both times the thing that makes the answer
  wrong is the thing that makes the evidence look right, so nothing computed from the candidate sets
  alone can see it. This is the gap between "reads well" and "known to read well".
- **Punctuation segmentation**, untracked. Eleven of the thirteen unmatched glyphs in the ceiling
  fixture are punctuation: `.` matches nothing, and `:` and `ï` each shatter into two placeholders.
- **Grouping**, two measured gaps in what a mark attaches to.
  [#57](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/57): an accent over a capital
  sits above every letterform the charset can spell, so in a line of nothing but letters it bands as
  a line of its own and `À` segments as a bare `A` plus a floating grave — 25 of 51 marks reach
  their body there against 44 with a `$` on the line.
  [#58](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/58): the overlap rule is a
  fraction of the *mark's* width, so a mark wider than the letter under it can never reach it and
  `Î Ï î ï` never group at all.
- **[#60](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/60)** — correct word-edge
  ambiguity from the track's own vocabulary. Post-correction needs evidence on both sides of a
  glyph, so `Iazy` stays wrong at a word edge; a word the same track already read clearly is
  evidence from the material rather than an assertion about a language.
- **[#16](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/16)** — distribution. A
  decision more than a build; the throughput numbers already weaken the `cdylib` case.

## Licence

MIT. See [LICENSE](LICENSE).
