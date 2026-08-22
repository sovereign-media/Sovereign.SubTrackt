# Sovereign.SubTrackt

Extract plain text from bitmap image-based subtitle streams — Blu-ray PGS and DVD VOBSUB — without
human intervention, and without a general OCR engine.

**Status: reads real media end to end; cannot yet name the characters it finds.**

Point it at a Blu-ray rip and it demuxes the Matroska, decodes the PGS, segments the bitmaps into
glyphs and emits timed cues with a confidence tally — 1,111 cues from a 5.5 GB film in 22 seconds.
What it cannot do yet is say which character each glyph *is*, because that needs a reference set and
the measurements below changed what one has to look like.

```console
$ subtrackt extract 'Dr. No (1962).mkv' --format vtt --on-unmatched placeholder --report
1111 cues from 1111 images (2222 packets); glyphs 0 matched / 35516 unmatched; cache 99%
```

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

Three things were measured against a real 1,328-title library rather than assumed. Each changed the
design.

**[`docs/library-survey.md`](docs/library-survey.md)** — 56 titles, 1950s–2020s, 149,604 glyphs.
A dominant glyph family runs through the library and one cluster covers 43 of 56 titles across
seventy years, so a fixed reference set is worth having. Fitting rendered fonts against it
identifies the typeface as **Arial or very close**: the most frequent extracted shapes land on
`i` at distance 0, `t` at 6, `a` at 10, `o` at 13. But a fixed set covers only **46%** of glyph
instances.

**[`docs/glyph-stability.md`](docs/glyph-stability.md)** — why. Two renderings of the *same*
character are typically further apart (median 46 cells) than two *different* characters are
(median 31). A one-pixel shift in where the binarization threshold falls costs 30 cells, as much as
character identity itself. Rendering size and anti-aliasing, by contrast, cost 11 and 8 — the axes
the normalisation was built to absorb, and it absorbs them.

**[`docs/post-correction.md`](docs/post-correction.md)** — what is left once shapes are as good as
they get. Resolving the pairs a binarized glyph cannot separate (`0`/`O`, `1`/`l`/`I`) from the
characters around them takes **3.1 points** off the ceiling fixture's character error rate and makes
no line worse. It ships switched off: the corrector refuses more than it accepts by construction,
but one generated fixture is not a corpus to decide a default that rewrites what a viewer reads.

**The consequence.** One reference vector per character cannot work, and enumerating styles does not
rescue it. The session cache stops being an optimisation and becomes the mechanism: the expensive
axes are constant *within* a stream, so clustering a title's own repeated shapes cancels exactly the
variation that defeats a fixed set. That is the second of the two outcomes §4 of #1 anticipated.

## Usage

```console
$ subtrackt list movie.mkv
  0  hdmv_pgs_subtitle    eng   1920x1080  Full
  1  hdmv_pgs_subtitle    eng   1920x1080  Forced

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
| `fail-track` | Abort the track so the caller can fall back to burn-in. **Default.** |
| `threshold` | Abort only if fewer than `--min-matched` of glyphs were read. |
| `drop` | Omit any cue containing an unread glyph. |
| `placeholder` | Emit the cue with a replacement character. |

The default is conservative on purpose: a partially-read subtitle track is not obviously better than
one that kept its pixels. Which default is right is a measurement nobody has taken yet — see
[#13](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/13).

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

The two issues that gated everything else, #8 and #14, are both answered; their write-ups are in
`docs/` and are the reason the remaining work looks different from the original plan. What is left,
roughly in order of leverage:

- **[#10](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/10)** needs *redesigning*,
  not implementing. It is written as per-glyph matching against a fixed set, and the measurements
  say that cannot work. It should become: cluster a stream's own shapes, then match cluster
  centroids against the reference.
- **Reducing edge sensitivity in binarization.** At 30 cells it is the largest term that is cheap to
  attack, and unlike weight and slant it is an artefact of our own thresholding.
- **[#3](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/3)** — VOBSUB. Only 4% of
  titles, but the entire DVD-era half of the library is unmeasured without it.
- **[#15](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/15)** — ground truth.
  Everything measured so far is coverage, not correctness, and nothing can tell the difference
  until this exists.

## Licence

MIT. See [LICENSE](LICENSE).
