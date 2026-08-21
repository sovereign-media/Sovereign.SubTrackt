# Sovereign.SubTrackt

Extract plain text from bitmap image-based subtitle streams — Blu-ray PGS and DVD VOBSUB — without
human intervention, and without a general OCR engine.

**Status: scaffold.** The pipeline is wired end to end and the plumbing is tested, but the decode,
segmentation and matching stages are stubs that return an error naming the issue tracking them.
Nothing extracts real text yet. See [issue #1][issue-1] for the design and
[`docs/architecture.md`](docs/architecture.md) for how it is laid out.

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

## Usage

```console
$ subtrackt list movie.mkv
  0  hdmv_pgs_subtitle    eng   1920x1080  Full
  1  hdmv_pgs_subtitle    eng   1920x1080  Forced

$ subtrackt extract movie.sup --format vtt --output movie.en.vtt --report
```

Input can be a container, a raw PGS `.sup` dump, or a VOBSUB `.idx`/`.sub` pair. Output is SubRip or
WebVTT.

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

Rust 1.85 or newer. No system dependencies, and none planned for the library crates — every crate
except the CLI depends only on the standard library. That is what keeps the single-static-binary
option in [#16](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/16) open.

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

## Contributing

Work is tracked as sub-issues of [#1][issue-1]. Two of them gate much of the rest and are worth
reading before picking anything up:

- [#8](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/8) — the typeface survey. Issue
  #1 calls typeface coverage "the whole risk", and the answer decides whether the fixed reference
  set in the design works at all.
- [#14](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/14) — whether one reference
  vector per character survives bold, italic, anti-aliasing and outline variation. If it does not,
  the session cache stops being an optimisation and becomes the mechanism.

Neither needs the pipeline finished to answer.

## Licence

MIT. See [LICENSE](LICENSE).
