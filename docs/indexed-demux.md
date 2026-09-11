# Finding a track through the file's own index

What changed in #258, what it measured on the file it was tested against, and what it has not been
tested on yet.

## The problem

The Matroska reader found a subtitle track by walking every cluster in the file. That was the
right way to walk: `ebml.rs` reads straight through every forward jump because seeking per element
cost ten minutes on a 5.5 GB film. But it meant an extraction read the whole file whatever size
the track was. `docs/cost-baseline.md` had already timed it: Gone Girl's 3.4 GB `.mkv` spent 13.6 s
in decode against 13.5 s for a plain read of the same file, and 0.6 s with the cache warm.

The file already says where every subtitle block is. mkvmerge's default is `--cues iframes` for
video *and* subtitle tracks, every subtitle block is a keyframe, and since 2013 each `CuePoint`
carries `CueRelativePosition`, the block's offset inside its cluster. Trickster built on this first
([Sovereign.Trickster#32](https://github.com/sovereign-media/Sovereign.Trickster/issues/32)), and
the reader here follows it.

## What the reader does

1. The `SeekHead` at the front of the file says where `Cues` is. A second `SeekHead` it points at
   is followed one level, and a `Cues` element before the first cluster is found without one.
2. `Cues` is read once, for every bitmap subtitle track: 2.3 MB at the tail of this file.
3. For each entry on the selected track, in file order: the cluster's first 64 bytes for its
   timestamp, then a 64 KiB window at the block. A window that already holds the next block is
   reused.

The index path reads through its own file handle. The walk's is buffered to read straight through
and would fetch a megabyte for every block.

### The one departure from Trickster

Trickster finds most blocks with a single read by *assuming* each cluster header is as long as the
last one, and takes the block's time from the index. Here the cluster header is always read, so a
block's time is its cluster's timestamp plus its own offset: the walk's arithmetic, from the same
bytes. The two paths then agree by construction rather than because the muxer wrote a consistent
index.

What that costs, measured below: **3,864 reads against Trickster's 1,956** for the same track,
which is 1.98 reads per block. A display set and its erase are seconds apart and rarely share a
cluster, so the header read is nearly one extra read for every block.

### When the index is not followed

| Case | What happens | What `--report` says |
| :--- | :--- | :--- |
| No `Cues` element | The track is walked | `container read through (the file has no Cues element)` |
| `Cues` has no entries for this track | The track is walked; a neighbouring track that does have entries still uses the index | `(the index has no entries for this track)` |
| Entries without `CueRelativePosition` (pre-2013) | The track is walked | `(the index gives clusters but not block positions)` |
| An entry that does not lead to a block on the track | The walk takes over from that entry's cluster, skipping everything the index already yielded | `container indexed: ... read through from pts N` |
| An entry that is missing | **The block is not read, and nothing says so** | `container indexed: ...` |

The last row is the one risk this path carries. The walk sees every block; the index sees the
blocks the muxer chose to enter. Nothing in the file says an entry is missing, so the reader has
nothing to refuse on. The test `an_index_missing_an_entry_loses_that_cue_and_nothing_says_so` pins
that behaviour rather than hiding it, and the protection is a measurement of how often real muxers
do it, which is #259.

## The test

*Blade Runner 2049*, the FraMeSToR 2160p remux: 78,775,024,506 bytes, 42 PGS tracks, stream 0,
read over SMB from the NAS. It is the file Trickster measured, so Trickster's figures are a
cross-check. `main` at `b47c532` against the #258 branch, same reference set (`arial-ri`).

### Predicted, before any of it ran

Recorded on #258:

- The walk takes 5 to 8 minutes.
- The index path reads under 200 MB and finishes in under 15 s end to end, cold.
- All 1,948 packets match: 974 display sets and 974 erases.

### Measured

| Run | Wall clock | Decode phase | Read by the process | Adapter received |
| :--- | ---: | ---: | ---: | ---: |
| `main`, walking | **307.4 s** | 306.5 s | the whole file | 77,973 MiB |
| #258, through the index | **3.68 s** | 2.8 s | 124.1 MiB in 3,864 reads | 195.1 MiB |

Both produced **974 cues from 1,948 packets**, and the two SRT files are **byte-identical**
(SHA-256 `65EF40A5…29AAC`, 63,856 bytes). `xtask demux-compare` then read the track both ways in
one process: **all 1,948 packets identical**, timestamp and payload.

The predictions held, with one figure to state precisely. The walk took 5.1 minutes, and
`demux-compare`'s walk of the same file took 477 s (8.0 minutes): a spread of 55% between two runs
of one code path, which only the network's variance explains. The reader's own count, 130.1 MB,
is under the predicted 200 MB. The adapter's figure is 204.6 MB, just over it, and it counts
traffic that was not this run's.

End to end that is **83 times faster** against the quicker walk, and the extraction is no longer mostly a file read: of 3.3
seconds of pipeline, 0.5 are segmentation. Warm, the demux alone takes **0.04 s** for the whole
track.

Against Trickster's figures for the same track: 130.2 MB in 1,956 reads and 4.05 s cold for demux
and PNG encoding, where this reads 124.1 MiB (130.1 MB) in 3,864 reads and 3.68 s including the
OCR. The bytes are the same, because both read one 64 KiB window per block.

### Conditions, stated because they move the numbers

"Cold" here means "straight after a walk of the same file". A 78.8 GB walk evicts almost all of
itself from the client's cache, but the tail of the file and the NAS's own cache may hold some of
what the index run wanted. Trickster's cold runs first read 36 GB of *other* files, which is the
stricter method. So 3.68 s may flatter a truly cold read, and the adapter figure is the check on
it: 195.1 MiB crossed the wire against 124.1 MiB asked for, so the reads were not being served from
the local cache.

The adapter counts everything the machine received, so it is an upper bound on what this run pulled
from the NAS, and the only figure that could see readahead the process never asked for.

### The random-access hint

Trickster found that readahead turned 3.4 MB of wanted bytes into 96.7 MB on Linux over NFS, and
opens its file with the platform's random-access hint. The index path does the same on Windows,
where `std` can pass `FILE_FLAG_RANDOM_ACCESS`. Priced against a handle opened without it, straight
after a walk each time:

| | Adapter received | Time reading the track |
| :--- | ---: | ---: |
| With the hint: the extraction above, decode phase | 195.1 MiB | 2.8 s |
| Without: `demux-compare --index-only --no-hint` | 145.8 MiB | 1.50 s |

That is one run of each, from two commands, and the adapter counts traffic that is not the run's.
It does not show the hint helping, and it cannot show it hurting either.
What it does show is that Windows' SMB client does not read ahead the way Linux's NFS client did:
at most 71 MiB of overhead on 124 MiB, not 28 times. The hint stays because it costs nothing and is
what Trickster measured a need for.

**The deployment is a Linux container, and there the hint is not applied.** `posix_fadvise` needs
`unsafe`, which the workspace forbids, or a crate, which the library crates do not take. Whether the
container's mount reads ahead the way Trickster's NFS mount did is unmeasured, and #260 is that
measurement.

## Smaller files, VobSub, and the bench

Blade Runner 2049 is the best case: a very large file with a track of ordinary size. #262 measured
the ordinary cases the same day, over the same share, with `b47c532` against `0a66c82`.

| | Before: walking | After: through the index | What came out |
| :--- | ---: | ---: | :--- |
| Dr. No (1962), 5.9 GB, PGS, 1,111 cues | 22.8 s | **4.2 s** | SRT byte-identical |
| The Karate Kid (1984), 6.4 GB, VobSub, in a bench pass | 26.1 s | **3.2 s** | SRT identical; 1,469 packets identical |
| Training Day (2001), 5.8 GB, VobSub, in a bench pass | 51.3 s | **3.6 s** | SRT identical; 1,421 packets identical |
| A full `run.py score` pass, nine tracks | 84.6 s | **14.7 s** | all nine SRTs identical |
| `run.py dump`, six PGS tracks from 45 GB of containers | 174.1 s | **22.1 s** | all six `.sup` files byte-identical, to each other and to the existing cache |

**The saving scales with the file, and the cost with the track.** Dr. No is 5.4 times faster, where
Blade Runner was 83. The index path read 113.5 MiB of Dr. No in 3,273 reads, 2% of the file, and
its 2,222 blocks cost about the same wherever they sit. The walk's cost is the file's size. A small
file with a long track gains least.

**Dr. No is also where "cold" is stricter.** It had not been read that day, and the index ran first.
Warm, both paths are cheap: 0.8 s through the index and 1.3 s walking. On a network share what the
index saves is the network.

**VobSub goes through the same index and reads the same.** The two DVD-sourced tracks were the last
codec the index had not been measured on. Both produced packets identical to the walk, one block per
cue: a VobSub packet carries its own end time, where PGS spends a second block on an erase. Training
Day's walk took 51.3 s in the pass and 21.9 s in `demux-compare` a few minutes later, which is the
same network variance Blade Runner showed.

The seventh dump entry, Cloverfield, fails both before and after. The library has replaced that
title's file with a release that carries no PGS track, which is #263 and nothing to do with the
index.

## What this has not measured

- **Whether the library's muxers index every subtitle block.** Ten files from the library now read
  identically both ways: three compared packet for packet, six dumped to byte-identical `.sup` files,
  and Dr. No to a byte-identical SRT. Which muxer wrote each one was not recorded. FFmpeg's muxer
  indexes every subtitle packet, and Trickster's FFmpeg-muxed fixtures read identically both ways.
  Other muxers, MakeMKV above all, are unmeasured.
  `xtask demux-compare` is the instrument: it reads a track both ways and names the first packet
  they disagree on. #259 is the survey.
- **Readahead on the Linux container.** See above; #260.
