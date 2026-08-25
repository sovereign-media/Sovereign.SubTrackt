# Sovereign.SubTrackt

Extract plain text from bitmap image-based subtitle streams — Blu-ray PGS and DVD VOBSUB — without
human intervention, and without a general OCR engine.

A 5.5 GB Blu-ray rip reads in **22 seconds** from a single **1.3–1.9 MB** static binary with no
runtime, no model files and no system dependencies.

> **Image-based subtitles only.** This reads subtitle tracks that are stored as *pictures* — PGS on
> Blu-ray, VOBSUB on DVD — and turns them into characters. Tracks that are already text (SubRip,
> ASS/SSA, WebVTT, MP4 timed text, Matroska's `S_TEXT/*`) are outside its scope entirely: there is
> nothing to recognise, and `list` will not show them. If a track is text, you want a muxer, not
> this. **Output** is text either way — SubRip or WebVTT.

**Status: 1.0.** The pipeline runs end to end on real media and the command-line surface is frozen:
flags and output formats change on a major, not before. What it does not carry is a
**reference glyph set** — on purpose. You generate one from fonts you already have and let
`subtrackt fit` pick between them. Until you do, every glyph honestly comes back unmatched, and
[Reference sets](https://sovereign-media.github.io/Sovereign.SubTrackt/guide/reference-sets) has the
reasoning.

---

## Documentation

**[sovereign-media.github.io/Sovereign.SubTrackt](https://sovereign-media.github.io/Sovereign.SubTrackt/)**

The documentation is there, in two sections:

- **[How it works](https://sovereign-media.github.io/Sovereign.SubTrackt/guide/what-this-is)** —
  eight short pages assuming nothing. What an image-based subtitle is, why an unmatched glyph is
  better news than a guessed one, [how it measures up against five other
  tools](https://sovereign-media.github.io/Sovereign.SubTrackt/guide/how-it-compares) over 24 films,
  and [what it cannot
  do](https://sovereign-media.github.io/Sovereign.SubTrackt/guide/what-it-cannot-do).
- **[Usage](https://sovereign-media.github.io/Sovereign.SubTrackt/usage/quick-start)** — installing
  a release binary, building a reference set, reading a track. Then one page per command with every
  flag on it, and worked examples of whole jobs.

The research corpus lives in this repository, unpublished and canonical, in [`docs/`](docs/) — the
surveys, the measurements and the decisions the numbers above came out of.

---

## Quick start

Three commands: make reference sets from fonts, pick the one that fits the title, read the track.

```console
$ subtrackt gen-reference /usr/share/fonts ./sets      # one .subtref per font
$ subtrackt fit movie.mkv --references ./sets -o movie.subtref
$ subtrackt extract movie.mkv --reference movie.subtref --format srt -o movie.en.srt
```

Step one is a one-off; steps two and three are what you run per title. Release binaries for Linux
and Windows, x86-64 and ARM64, are on the [releases
page](https://github.com/sovereign-media/Sovereign.SubTrackt/releases) — the
[quick start](https://sovereign-media.github.io/Sovereign.SubTrackt/usage/quick-start) has the
install commands and the checksum step.

## As a library

The CLI is a thin shell over `subtrackt::Pipeline`; everything of substance is in the library.

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

**The library crates take no dependencies** beyond `miniz_oxide` in `subtrackt-demux` — see
[CLAUDE.md](CLAUDE.md) for why, and for the one optional exception. `subtrackt-glyph`'s font
rasteriser sits behind an off-by-default `font` feature, so the released binary can generate its own
reference sets while a library consumer who does not opt in keeps the dependency-free crate.

## Building

```console
$ cargo build --release
$ cargo test --workspace
```

Rust 1.98 or newer, and no system dependencies — which is what makes cross-compilation to
`linux/arm64` uneventful.

Run `scripts/check.sh` before pushing. It runs what CI runs, in CI order: fmt, clippy at pedantic
with `-D warnings`, tests, dependency discipline, docs. Clippy is the gate that catches the most and
the one easiest to skip.

`cargo run -p xtask -- accuracy` is the measurement that matters: it generates a fixture and a
reference set from the same font, runs the pipeline, and scores the output against known ground
truth. Treat its number as a ceiling — fixture and reference share a font, so real material can only
do worse.

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
| [`site/`](site) | The documentation site, built from `site/content/` and deployed to GitHub Pages |

## Licence

MIT. See [LICENSE](LICENSE).
