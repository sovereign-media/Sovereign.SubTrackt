# What a run costs

The bench has always answered *how well did it read*. This answers *what did it cost*, which until
[#154] nothing in the tree measured at all — no timing, no memory, no `criterion`, no `[[bench]]`.

The reason to write it down is [#144]: a sweep of the codebase proposed nine refactors, seven of
them justified by cost, and the justifications were arithmetic done by reading code. Three of them
were wrong. This file is the measurement that says so, and the thing any future performance claim
has to be argued against.

## How to take it again

```console
$ scripts/bench/run.py dump  --cache bench-cache
$ scripts/bench/run.py score --cache bench-cache --reference arial-ri.subtref --out before.json
```

`--report` prints cost on its own second line, and `run.py` records it per track. The reference set
used below is Arial regular plus its italic cut, rendered from the system fonts:

```console
$ subtrackt gen-reference C:/Windows/Fonts/arial.ttf arial-ri.subtref \
      --name arial-ri --italic C:/Windows/Fonts/ariali.ttf
```

## The baseline

Taken 2026-08-23 against the seven-track roster, extracting from the `.sup` dump cache so that
container demuxing is excluded. Peak is the process working set, measured by the harness from
outside; the two resident figures are the pipeline's own accounting of what it holds.

| track | total | peak | decode | segment | cluster | read | images | glyphs |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| cloverfield | 0.35 s | 71 MiB | 0.0 s | 0.3 s | 0.0 s | 0.0 s | 40.0 MiB | 4.1 MiB |
| gonegirl | 1.17 s | 213 MiB | 0.1 s | 1.0 s | 0.0 s | 0.0 s | 132.3 MiB | 13.1 MiB |
| wanda | 0.60 s | 121 MiB | 0.1 s | 0.5 s | 0.0 s | 0.0 s | 76.4 MiB | 7.5 MiB |
| prestige | 0.90 s | 178 MiB | 0.1 s | 0.8 s | 0.0 s | 0.0 s | 118.5 MiB | 10.2 MiB |
| kingkong | 0.63 s | 128 MiB | 0.1 s | 0.5 s | 0.0 s | 0.0 s | 75.6 MiB | 7.4 MiB |
| airplane | 0.67 s | 130 MiB | 0.1 s | 0.6 s | 0.0 s | 0.0 s | 78.1 MiB | 7.6 MiB |
| mexico-forced | 0.06 s | 14 MiB | 0.0 s | 0.0 s | 0.0 s | 0.0 s | 4.5 MiB | 0.4 MiB |

Gone Girl is the p100 of the library survey — 66,438 glyphs, the longest track the bench holds — so
its row is the ceiling rather than a typical case.

**Repeatability.** Two passes over one cache produce identical `cues`, `matched`, `unmatched`,
`ambiguous`, `read`, `fit`, `cer` and `wer` on every track. Wall clock varies by up to 5%, which is
the noise floor any timing claim has to clear.

## What this overturned

Three of [#144]'s cost arguments do not survive contact with the numbers.

**Peak memory is not a problem.** The pipeline holds every decoded bitmap resident before it
segments anything, which is structurally true and was ranked first. It costs **132 MiB on the
longest track in the library**, inside a 213 MiB process. Restructuring the pipeline to stream would
buy back memory nobody is short of.

**The Matroska reader is I/O bound, not allocation bound.** Extracting Gone Girl from its 3,440 MiB
`.mkv` spends **13.6 s** in decode against 1.0 s segmenting. That looks like the demuxer dominating
until you time the file itself:

| | |
| :--- | ---: |
| decode phase, from `.mkv` | 13.6 s |
| raw sequential read of the same file, doing nothing with it | 13.5 s |
| effective throughput | 255 MB/s |

The reader is keeping up with the disk to within a percent. The per-block allocation [#144] row 2
objects to is real, and it is spending its time waiting on the network. **No allocation change to
this path can buy more than about a second of fourteen**, and only on a file that is already cached.

**The vocabulary arm's quadratic is invisible.** Gone Girl carries 7,046 ambiguous glyphs, which is
the worst case that argument was built on. Extraction takes 1.1 s with post-correction off, 1.1 s
with it on, and 1.1 s with the vocabulary arm on as well. The complexity is real; the constants are
not big enough for it to matter.

## What survives

**Segmentation is the only real CPU in the pipeline**, at 85–90% of every row above. If any
performance work is worth doing on the extraction path it is there — the binarizer's per-pixel
bounds checks, the mask's byte-per-pixel storage, the labelling passes.

**`xtask srt-score` costs more than an extraction does.** Scoring one film against its release
sidecar takes **7.4 s**, against 1.1 s to produce the extraction being scored. It is the largest
single measured cost anywhere in the tree, and it is on the bench path — which is to say it is paid
by whoever is doing the measuring, repeatedly, all day.

## The rule this suggests

Everything above is under two seconds except the two things nobody had measured. That is the general
shape of it: **the costs here are not where reading the code says they are**, and this project
already knows that about accuracy. `docs/glyph-stability.md` is a record of sound reasoning that
measured wrong, and the answer there was to build the instrument first. Same answer here.

[#144]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/144
[#154]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/154
