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

**Status: alpha.** The pipeline runs end to end on real media, but it ships with **no reference
glyph set embedded** — on purpose, and the reasoning is in
[`docs/reference-set.md`](docs/reference-set.md). You generate a set from fonts you already have and
let `subtrackt fit` pick between them. Until you do, every glyph honestly comes back unmatched.

---

## Install

Tagged releases carry a self-contained binary for Linux and Windows, x86-64 and ARM64, with a
`SHA256SUMS` covering the set. The Linux builds are statically linked against musl, so there is no
glibc version to match and no runtime to install — the artifact is the whole dependency.

**Linux**

```console
$ tag=v0.0.2-alpha
$ base=https://github.com/sovereign-media/Sovereign.SubTrackt/releases/download/$tag
$ curl -LO $base/subtrackt-$tag-x86_64-unknown-linux-musl
$ curl -LO $base/SHA256SUMS && sha256sum -c --ignore-missing SHA256SUMS
$ install -m 755 subtrackt-$tag-x86_64-unknown-linux-musl /usr/local/bin/subtrackt
```

**Windows** (PowerShell)

```powershell
$tag = 'v0.0.2-alpha'
$base = "https://github.com/sovereign-media/Sovereign.SubTrackt/releases/download/$tag"
Invoke-WebRequest "$base/subtrackt-$tag-x86_64-pc-windows-msvc.exe" -OutFile subtrackt.exe
Invoke-WebRequest "$base/SHA256SUMS" -OutFile SHA256SUMS
Get-FileHash subtrackt.exe -Algorithm SHA256   # compare against the matching line in SHA256SUMS
```

Four targets are published per tag: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
`x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`. There is no macOS build because nobody has
asked for one; it is one row in the release matrix if that changes.

The tag is named rather than resolved through `/releases/latest/`, because that path skips
pre-releases and every tag so far is one — and because the asset filename carries the version, so a
`latest` URL would have to guess it.

Confirm what you installed, including which reference data it carries:

```console
$ subtrackt --version
subtrackt 0.0.2-alpha (reference set: empty, 0 glyphs)
```

To build from source instead, see [Building](#building).

---

## Quick start

Three commands: make reference sets from fonts, pick the one that fits the title, read the track.

```console
$ subtrackt gen-reference /usr/share/fonts ./sets      # one .subtref per font
$ subtrackt fit movie.mkv --references ./sets -o movie.subtref
$ subtrackt extract movie.mkv --reference movie.subtref --format srt -o movie.en.srt
```

Step one is a one-off. `./sets` is a library of candidates you accumulate; steps two and three are
what you run per title.

If you already know the typeface, skip `fit` and pass the set straight to `extract`.

---

## How it works

A subtitle track on a disc is not text. It is a sequence of pictures of text, and turning one back
into characters means having something to compare each shape *against*. That comparison set is what
you build first, once; everything after it is per title.

### 1. Build reference sets from the fonts you have

`subtrackt gen-reference` rasterises a font and records one entry per character — the same 256-bit
feature vector the pipeline will later compute from a decoded subtitle bitmap, produced by the same
normalisation, plus the metrics the grid rounds away. Point it at a directory and you get one
`.subtref` per font, which is why the first command reads a whole system font folder:

```console
$ subtrackt gen-reference /usr/share/fonts ./sets
```

`./sets` is a library of *candidates*. You are not choosing a typeface at this point, you are
collecting the ones you could plausibly be asked to read, and each set costs a few kilobytes. Add
`--italic` and `--bold` and one set carries several cuts of the same typeface, which matters because
a film's italic act is not drawn in the same shapes as its dialogue.

**Nothing is embedded in the binary**, so this step is not optional — and that is a decision rather
than an omission. A shipped set would be built from some typeface that is not the one the disc was
authored in, and a near-miss reads as confidently wrong text that no counter can detect.
[`docs/reference-set.md`](docs/reference-set.md) has the measurement.

### 2. Fit picks the set that reads *this* title best

```console
$ subtrackt fit movie.mkv --references ./sets -o movie.subtref
```

`fit` samples the first few hundred cues, segments them into glyphs, and scores every candidate set
in the directory against those shapes — mean match distance per glyph, charging an unread glyph the
ceiling. Lowest wins. A typeface does not change halfway through a film, so a prefix is a fair
sample and this takes about three seconds against a 1.8 GB file rather than the seventy a full pass
would.

The output is a ranked table and, with `-o`, a copy of the winner ready to hand to `extract`.

**`fit` proposes; it does not certify.** The score ranks candidates against each other and says
nothing about whether the winner is good enough, because nothing measurable can — six statistics
have now failed to tell a good read from a bad one without ground truth. Read a few cues before
trusting a track to a set. [`docs/fit-confidence.md`](docs/fit-confidence.md) is the long answer.

### 3. Extract reads the track and writes a subtitle file

```console
$ subtrackt extract movie.mkv --reference movie.subtref --format srt -o movie.en.srt \
      --post-correct --report
```

The track is demuxed, decoded into bitmaps, binarized, split into connected components, grouped into
glyphs and lines, vectorised and matched against the set — then ordered into words, timed, and
written as **SubRip (`.srt`) or WebVTT (`.vtt`)** on stdout or to `-o`. Lines shown to lean are
tagged `<i>`.

What comes out alongside the text is the point of the design. `--report` prints a tally to stderr:

```
822 cues from 822 images (1644 packets); glyphs 20597 matched / 17 unmatched
  / 2237 ambiguous (99.9% read); fit 10.7; cache 100%; defused 87; corrections 3 (context)
```

Those are counts, not confidence scores. A glyph the reference set does not contain comes back as
**no match** — a fact a caller can act on, by routing the track to burn-in instead of shipping text
nobody wrote. `--on-unmatched` decides what happens when too many of them appear, and it defaults to
abandoning a track that read less than 90% of its glyphs.

---

## Commands

### `subtrackt list` — what is in the file

```console
$ subtrackt list movie.mkv
  0  hdmv_pgs_subtitle    eng   1920x1080  Full
  1  hdmv_pgs_subtitle    eng   1920x1080  Forced
```

Columns are stream index, codec, language, subtitle plane size, and the track title if the container
carries one. The index is what `--stream` takes; every other command defaults to the first bitmap
stream.

**Input** can be a Matroska container (`.mkv`), a raw PGS dump (`.sup`), or a VOBSUB `.idx`/`.sub`
pair — point it at the `.idx`.

**Only the two bitmap codecs are listed.** A Matroska text track — `S_TEXT/UTF8`, `S_TEXT/ASS` and
the rest — is skipped rather than reported, and a file carrying nothing else fails with a demux
error saying so. That is the intended answer: those tracks are already the output this tool exists
to produce, so there is nothing here for them to gain.

### `subtrackt gen-reference` — make a reference set

Nothing is embedded, so this is how a set comes to exist. Point it at one font for one `.subtref`,
or at a directory of fonts for one set per font.

```console
$ subtrackt gen-reference Arial.ttf arial.subtref --italic Ariali.ttf --bold Arialbd.ttf
$ subtrackt gen-reference /usr/share/fonts ./sets
```

| Flag | Meaning |
| :--- | :--- |
| `--name <NAME>` | Name recorded inside the set, and what `fit` prints when it ranks. Defaults to the font's filename stem. |
| `--italic <FONT>` | Italic cut of the same typeface, contributing its own vector for every character. |
| `--bold <FONT>` | Bold cut, likewise. |

**`--italic` is worth more than it looks and you do not have to use it.** A set that carries an
italic cut reads a real Blu-ray's italic act at 2.0% CER against 8.1% without one, so supply it if
you have the font. Where you do not, `extract` samples a leaning line along its own slant instead —
which is worth 47.1% down to 8.1% on the same act, and switches itself off the moment a set carries
an italic cut. [`docs/italic-slant.md`](docs/italic-slant.md) has the four-way table.

`--name`, `--italic` and `--bold` describe one typeface, so they cannot be combined with a directory
of fonts. `.ttf` and `.otf` are read; `.ttc` collections are skipped, because picking the first face
silently would file it under a name that does not describe it.

The vectors go through the same normalisation `extract` applies to a decoded subtitle bitmap. That
is why this is a subcommand and not a script: a reference built through any other transform produces
distances that mean nothing.

### `subtrackt fit` — choose the set that reads the title best

```console
$ subtrackt fit movie.mkv --references ./sets -o movie.subtref
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
not change halfway through a film, and cues are spread evenly, so a prefix is a fair sample.

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `-r`, `--references <DIR>` | *required* | Directory of candidates. Every `.subtref` in it is scored. |
| `-o`, `--output <FILE>` | — | Copy the winner here, ready to pass to `extract --reference`. |
| `-l`, `--limit <N>` | `400` | Cues to sample. Raise it if a title changes style partway. |
| `--show <N>` | `5` | Candidates to list. The winner is always listed. |
| `-s`, `--stream <N>` | first | Which stream to read. |
| `--include-outline` | off | Include the glyph outline in the foreground mask, not just the fill. |

Without `-o` it prints the `extract` command line to run. Files in the directory that are not
reference sets are skipped with a warning rather than failing the run, and sets built for a
different grid size are counted as unusable — so a candidate list that came out quietly smaller than
the directory looked still says so.

**The score ranks candidates against each other and says nothing about whether the winner is any
good.** That is not an oversight; see [Shortcomings](#shortcomings).

### `subtrackt extract` — read the track

```console
$ subtrackt extract movie.mkv --reference movie.subtref --format srt -o movie.en.srt \
      --post-correct --report
822 cues from 822 images (1644 packets); glyphs 20597 matched / 17 unmatched
  / 2237 ambiguous (99.9% read); fit 10.7; cache 100%; defused 87; corrections 3 (context)
```

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `--reference <FILE>` | *none embedded* | The `.subtref` to match against. Without it every glyph is unmatched. |
| `-o`, `--output <FILE>` | stdout | Where to write the subtitle. |
| `-f`, `--format <FMT>` | `srt` | `srt` (SubRip) or `vtt` (WebVTT). |
| `-s`, `--stream <N>` | first | Which stream to read. |
| `--on-unmatched <POLICY>` | `threshold` | What to do about glyphs the matcher cannot identify. See below. |
| `--min-matched <RATIO>` | `0.90` | With `threshold`, the fraction that must match. Rejected at parse time outside `0.0..=1.0`. |
| `--report` | off | Print the extraction summary to stderr. |
| `--defuse` / `--no-defuse` | on | Retry a component the matcher cannot read as two characters that touched. |
| `--post-correct` / `--no-post-correct` | off | Resolve ambiguous reads from surrounding characters. |
| `--track-vocabulary` / `--no-track-vocabulary` | off | Also resolve word-edge glyphs from words the same track read clearly. Needs `--post-correct`. |
| `--vocab-min-occurrences <N>` | library default | How often a word must be read clearly before it counts as evidence. |
| `--vocab-min-len <N>` | library default | Shortest word the vocabulary arm may correct. |
| `--vocab-prefix` / `--no-vocab-prefix` | on | Let a candidate match the *start* of a clear word, so `look` is supported by a clear `looking`. |
| `--include-outline` | off | Include the glyph outline in the foreground mask, not just the fill. |

**Italic lines are tagged.** A line the ink shows was set in a leaning face is written `<i>`, in both
formats. The decision is a measurement of the line's own slant rather than a reference lookup, so it
works on a set carrying no italic cut — and it agrees with a release subtitle's own tags on **98.8%**
of cues without changing a character of the text.
[`docs/italic-slant.md`](docs/italic-slant.md) has the figures and the two ways it is still wrong.

The paired flags exist so a wrapper script can append one to a command line that already carries the
other; the rightmost wins. Both are spelled out because the default is a measurement result rather
than a fixed property, and a caller that has pinned the behaviour it wants should not have to notice
when the measurement moves.

**Reading the report.** `matched` found a reference within threshold, `unmatched` found none,
`ambiguous` matched but with a runner-up too close to call. `fit` is the mean Hamming distance of
the glyphs that matched — coverage says how many glyphs found *a* reference, `fit` says how well
they fitted it, and a mean drifting up toward the threshold is the signal that a track is being read
confidently and wrongly. `cache` is the session-cache hit rate. With `--post-correct`, every
individual correction is listed under the summary, because a stage allowed to rewrite text has to
leave a trace: `3 corrections` cannot be checked by anybody, and `'I' -> 'l' in "jalapeño"` can.

### `--on-unmatched`: what happens when a glyph cannot be read

| Value | Behaviour |
| :--- | :--- |
| `threshold` | Abort only if fewer than `--min-matched` of glyphs were read. **Default, floor 0.90.** |
| `fail-track` | Abort on a single unread glyph. |
| `drop` | Omit any cue containing an unread glyph. |
| `placeholder` | Emit the cue with a replacement character. |

The floor is a floor and not a target: it catches a track that could not be *read*, and makes no
claim about one read *well*. `fail-track` was the default until it was measured — rejecting on a
*single* unread glyph refused essentially every track in the library, which is a gate that never
opens. 0.90 was bounded from above by the pipeline's own ceiling case at 93.9% coverage and from below by
the 48 of 56 surveyed titles that clear it. **Both bounds have since moved and neither has been
re-cut.** The ceiling is now 99.7% on the generated fixture and 99.5% on a real disc
([`docs/reference-rendering.md`](docs/reference-rendering.md)), and the library re-survey the floor
was always waiting on now exists: 47 titles read a median 99.7% of their glyphs where the reference
set fits, and 89.9% where it does not
([`docs/library-accuracy.md`](docs/library-accuracy.md#table-1--accuracy-over-the-sample)). So the
floor has headroom it did not have when it was chosen, and the evidence to raise it on — but
coverage is a weak predictor of correctness whatever value it takes, which is why raising it is not
worth much and has not been done. See
[`docs/architecture.md`](docs/architecture.md#the-accuracy-gate).

### `subtrackt glyphs` — dump shapes without reading them

```console
$ subtrackt glyphs movie.mkv --limit 200 > shapes.tsv
$ subtrackt glyphs movie.mkv --summary
```

One tab-separated row per glyph: cue, line, x, y, width, height, and the 256-bit feature vector as
hex. No reference set is involved, and feature vectors are comparable across titles — which is what
made the typeface survey and the reference-set work possible. `--summary` prints the one-line stream
header only. Takes `--stream`, `--limit` and `--include-outline`.

### Global flags

`-v` / `--verbose` (repeatable: warn, info, debug, trace), `--color <auto|always|never>`,
`--progress <auto|always|never>`, `--plain`. All four work after the subcommand, which is where a
hand reaches for them when a run turns out noisier than expected. `RUST_LOG` overrides the verbosity
filter.

### Output discipline

Data goes to stdout; everything the tool says about itself goes to stderr, coloured by severity and
with a spinner or progress bar while it works. Both switch themselves off when stderr is not a
terminal, so a piped run, a redirect and CI are already clean and need no flag — the flags are there
for the case detection gets it wrong. `--plain` is one flag for when it gets both wrong at once, and
it beats `--color always`. `NO_COLOR` is honoured.

**Not one escape byte reaches stdout, ever**: a coloured `.srt` is a corrupt `.srt`.

Exit status is `0` on success and `1` on failure, with the reason on stderr as `error: …` carrying
the full context chain — so a failing run says *where* it failed, not just that it did.

---

## Why this exists

### Why not just run OCR

Because a general OCR engine's failure mode is a confident wrong answer. Tesseract will read a glyph
it has never seen as something plausible and attach a probability to it, and a probability is not a
fact you can gate on. For an automated pipeline — no human in the loop, thousands of titles — that
is the difference between a track you can route to a fallback and a subtitle file full of invented
words that nothing downstream can detect.

This is a purpose-built glyph matcher instead. A character the reference set does not contain comes
back as *no match*: detectable, countable, and something a caller can act on by falling back to
burn-in rather than shipping text nobody wrote.

That property is the whole argument, and it is why the report counts glyphs rather than estimating
probabilities.

### Inside the pipeline

[How it works](#how-it-works) above is the workflow; this is what the code does with a track. The
workspace is a fan, not a chain: no stage crate depends on another, and every stage after demux is a
trait in `subtrackt-core::stage`.

1. **Demux.** A hand-rolled Matroska reader pulls codec packets out of the container. A library
   survey found 1,326 of 1,328 titles were Matroska, so a native parser covers 99.8% and pulling in
   `ffmpeg-next` would have bought 0.2%.
2. **Decode.** PGS and VOBSUB packets become indexed bitmaps. 83% of the PGS tracks in that library
   are stored zlib-compressed inside Matroska, which is why the one dependency in the library crates
   is `miniz_oxide`.
3. **Segment.** Bitmaps are binarized, split into connected components, and diacritics are grouped
   onto the letterforms they belong to.
4. **Match.** Each glyph is normalised onto a fixed grid and flattened to a **256-bit vector**, then
   compared against the reference set by **Hamming distance**, with the measurements the grid throws
   away added back as terms: how tall the glyph stands against its own line's cap height, how far it
   drops below the baseline, and how wide its ink is against its own height. That last one is what
   separates `l` from `I`, which are the *same* vector at the same height. A runner-up too close to
   call is flagged ambiguous rather than silently chosen.
5. **Render.** Glyphs are ordered into lines and words, timed, and written as SubRip or WebVTT.
   Word gaps are measured along the line's own slant, so a leaning line's spaces survive; a line
   shown to lean is tagged `<i>`.

[`docs/architecture.md`](docs/architecture.md) is the full map.

### Why it is fast

**1,111 cues and 35,516 glyphs out of a 5.5 GB Blu-ray rip in 22 seconds.** Four things account for
it, and none of them is clever:

- **Matching a glyph is a handful of instructions, not an inference.** XOR and popcount over four
  machine words, against a few hundred reference vectors. There is no model to load, no session to
  warm, no GPU to find.
- **Tens of thousands of glyphs collapse to a few hundred distinct shapes.** The same `e` is the
  same 256-bit vector every time it appears, so a session cache answers **99–100%** of glyphs
  without a scan. The rate is in the report; if it drops, something upstream has stopped
  normalising.
- **`fit` samples rather than reads.** A typeface does not change halfway through a film, so ranking
  candidates touches 400 cues and finishes in about three seconds against a 1.8 GB file instead of
  seventy.
- **Recovery work is rare enough to be free.** The de-fusing pass of
  [`error-census.md`](docs/error-census.md) recomputes an image's foreground mask only when
  something in that image failed to read — about a hundred images in a feature film — so it costs
  **2.4 seconds on a 1.7 GB rip**, 14.8 to 17.2, for 0.7 points of character error.
- **Startup is free.** Cold start, process spawn to exit, is **15.8–16.3 ms** — measured on Windows,
  which has the slowest process creation of the platforms here. Against a 22-second extraction, that
  settled the "CLI or `cdylib`" question outright: a caller can spawn one process per track and not
  measure it. [`docs/distribution.md`](docs/distribution.md) has the working.

The binary is 1.3–1.9 MB depending on target, statically linked on Linux. Adding colour, a spinner
and a progress bar cost 30.5 KB, because the bar is forty lines of arithmetic rather than five more
crates.

### How accurate it is

On a real Blu-ray, with a fitted reference set: **0.6% character error across all 818 scored cues**,
at **99.9%** glyph coverage. A reference set chosen to be wrong reads the same track at **61.6%**.
That gap is the entire argument for fitting, and it is why nothing ships embedded.

Scored against the English subtitle shipped beside the rip; [`docs/reference-set.md`](docs/reference-set.md)
explains why that is evidence rather than ground truth. [`docs/error-census.md`](docs/error-census.md)
says character by character where the error is, and it is far less spread out than the number
suggests: the rate fell from 5.5% to 0.6% by closing four classes, each of which was a fifth or more
of the total on its own. The pipeline's own ceiling — fixture and reference rendered from the same
font, so typeface mismatch is excluded by construction — is **1.2%** CER, and real material can only
do worse.

**That is one disc, chosen because it fits Arial.** Pointed at a library rather than at the three
titles the pipeline was tuned against, a single Arial set reads 47 titles at **13.56%** CER pooled
and **8.21%** median — and most of the upper tail is the instrument rather than the pipeline, since
a release subtitle is another transcript and not ground truth.
[`docs/library-accuracy.md`](docs/library-accuracy.md) separates the two.

---

## Shortcomings

Known, measured, and stated here rather than discovered later.

**You have to supply the reference set, and it reads best when it comes from the material's own
typeface.** This is the big one, and it is what stands between this and working out of the box.
Reading Arial-authored material with Liberation Sans — metric-compatible, openly licensed, the
obvious thing to ship — costs **11 points of CER**, which is Verdana's cost to within noise. Fitting
a set per title is as far as that can be taken without ground truth, and it is what `subtrackt fit`
does; on a real library a single Arial set already fits 34 of 47 titles, and mean match distance
names the ones it does not fit before any transcript is consulted.

**Nothing can tell a good fit from a bad one.**
[#63](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/63) tested five statistics and
all five failed, for two mechanisms. Four were functions of the matcher's own answer, and a
systematically wrong reference set is *by construction* a low-distance one — the thing that makes
the answer wrong is the thing that makes the evidence look right. The fifth escaped that by never
consulting a character at all, and broke on the channel instead: a typeface's style is measurable in
its font file and does not survive being decoded off a subtitle plane.
[#101](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/101) added a sixth that reads
the output *text* under a language prior, escaping both, and it fails for a third form of the first:
a statistic that cannot score a character scores the read on everything else, and the characters it
cannot score are never a random sample. So `fit` proposes and you decide — **read a few cues before
trusting a track to a set.** See [`docs/fit-confidence.md`](docs/fit-confidence.md).

**Style is finer-grained than typeface, and the italic act is still the weaker half.** A set
carrying only an upright cut reads one film's italic cues at 40.5% CER against 10.8% for an italic
set, and the italic set does the exact reverse — so a fit that is right about the typeface can still
be wrong about a reel. `gen-reference --italic/--bold` puts several cuts in one set, and the slant
measurement closes most of what is left where no italic cut exists; between them a real disc's
italic act reads at 2.0% against its upright dialogue's 0.5%. It is *closer*, not closed. Nothing
can choose between an italic cut and the slant measurement per title, for the same reason nothing
can grade a fit. [`docs/italic-slant.md`](docs/italic-slant.md) has the four-way table.

**Segmentation, not matching, is what is left.** On the disc that reads at 0.6%, the two largest
remaining classes are a `"` arriving as two `'` — 27% of the residual, and it costs a substitution
*and* an insertion each time — and a word space that was never cut, at 24%. Neither is a character
the matcher got wrong. Of the word spaces, 29 of 36 are on upright lines, where the slant
measurement cannot help and only a sharper decisiveness margin would. The colon was the largest of
these until #130, and it is worth knowing why it lasted: the rule holding its two dots together
allowed a gap of 200% of a dot height, and a colon's is 225–450%. It could never fire.

**Word spacing collapses on some all-caps lines.** `MAN ON INTERCOM: The red zone is` can come out
`MANONINTERCOM:Theredzoneis`. Seen on SDH speaker labels and not yet measured or explained.

**Post-correction ships off, and now has very little to do.** It resolved 363 characters on a real
Blu-ray before the matcher could tell `l` from `I` by ink width; it resolves **3** now, and none at
all on the ceiling fixture. That is the right direction — a correction is evidence about a glyph
that shape alone could not decide — but it means the stage is no longer a lever. It stays opt-in
because the only comparison available for a real track is another release's subtitle, which is
evidence rather than ground truth. `--track-vocabulary` adds a second arm for word-edge ambiguity,
which the context rule cannot reach because it needs a clear character on *both* sides. It was worth
another nine characters in twenty-four thousand when #60 measured it; on the three discs today it
fires **not once**, because the glyphs it fed on are no longer ambiguous.

**Format coverage.** Matroska, raw `.sup` and VOBSUB `.idx`/`.sub` only. MPEG-TS (`.m2ts`) returns
`Unsupported` naming [#86](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/86) rather
than guessing. VOBSUB was 4% of the surveyed library, so PGS got the attention.

**Every accuracy figure here is PGS.** VOBSUB decodes, and nothing has measured what it *reads*.
The typeface survey predates the decoder and was never re-run, so the DVD-era part of the library —
lower resolution, older, the part most likely to differ — is the largest unmeasured corner of this.

**Alpha, and versioned like it.** Every release so far is a pre-release, and the CLI surface is not
yet frozen.

---

## What the measurements found

Nothing here was assumed. Each write-up was measured — against a real 1,328-title library, or
against ground truth the tool renders itself — and each one changed the design, usually by killing
the plan it was meant to confirm.

| Document | What it settled |
| :--- | :--- |
| [`library-survey.md`](docs/library-survey.md) | 56 titles, 1950s–2020s, 149,604 glyphs. One glyph cluster covers 43 of 56 titles across seventy years, and fits **Arial or very close** — but a fixed set covers only **46%** of glyph instances, and "or very close" was the expensive half of that sentence. |
| [`glyph-stability.md`](docs/glyph-stability.md) | Why. Two renderings of the *same* character are typically further apart (median 46 cells) than two *different* characters are (median 31). A one-pixel shift in the binarization threshold costs 30 cells — as much as character identity itself. Rendering size and anti-aliasing cost 11 and 8: the axes normalisation was built to absorb, and it absorbs them. |
| [`error-census.md`](docs/error-census.md) | Where the error actually is, per character, on a real disc, and what closing each class was worth. Half of it was the full stop, a quarter was two characters touching and fusing into one component nothing could read, and two thirds of what remained was `l` read as `I` — the pair #10 measured at distance *zero*, separated in the end by a ratio the 16-cell grid rounds away. Between them, **5.5% to 0.7%**, and two further discs say the same. The italic gap below took it to **0.6%**, and what is left is a colon, a quotation mark and a word space — segmentation rather than matching. |
| [`reference-rendering.md`](docs/reference-rendering.md) | Why the full stop matched nothing, and the two-line change that halved the disc's error rate. The reference set letterboxed the *rasteriser's* box; the runtime letterboxes a connected component's. They differ by a pixel — 1.5% of an `M`, **15% of a period** — and controls show the box is the whole effect: a second entry at any size or threshold buys nothing unless it changes the box, and buys 2.7 points when it does. |
| [`post-correction.md`](docs/post-correction.md) | What is left once shapes are as good as they get. Resolving `0`/`O` and `1`/`l`/`I` from context takes 1.9–2.2 points off the ceiling fixture's CER and makes no line worse. |
| [`reference-set.md`](docs/reference-set.md) | Why nothing is embedded: a shipped set trades a detectable failure for an undetectable one. Also puts ten candidate typefaces to a real disc, where mean match distance **picks the right one** — 11.7 against 18.5 for the runner-up, 8.8% CER against 16.6%. |
| [`fit-confidence.md`](docs/fit-confidence.md) | Whether anything can tell a good fit from a bad one without ground truth. **No** — six statistics, three mechanisms. The fifth identifies a typeface from its font file 79–85% of the time and still cannot gate a track, because a decoded glyph drifts further from its own typeface than the typefaces sit apart. The sixth scores the read text against a language prior and overlaps at every threshold, because the characters a model cannot score are exactly the ones where the read went wrong. |
| [`library-accuracy.md`](docs/library-accuracy.md) | What the pipeline reads on a real library rather than on the three discs it was tuned against: 50 titles, 1950–2025, 268 GB. **13.56% CER** pooled, 8.21% median, and the upper tail is mostly the instrument — a title read at 99.6% coverage and a good fit still scores 77.6% when the sidecar it is scored against is a different transcript. Per-title font selection moves the corpus figure by 0.4 points; three characters fail *every single time*. |
| [`italic-slant.md`](docs/italic-slant.md) | Why the italic act is a third of the errors left on a real disc while being 6% of it. **27% of an italic line's gaps reach the spacing rule already saturated at zero**, against 0.7% of an upright line's, because a slanted box overhangs the box after it — a failure two stages before the matcher, which is why no reference set ever touched it. Measuring the gap between deskewed ink instead took **Gone Girl from 2.8% to 2.0% CER** and gave back 618 of its 783 missing word spaces, on a disc whose release marks none of its 18% italic lines. Sampling along the line's own slant takes #14's most expensive axis from 47 cells to 26 — and a disc then says a deskew and an italic reference cut are **alternatives**, not a stage and an improvement to it: **47.1% to 8.1%** on a set with no italic entries, and worse on a set that has them. One number tells the two acts apart at 99.4% per cue, and that ships as an `<i>` on the output at **98.8%** agreement with a release's own tags, without moving CER by a character. |
| [`distribution.md`](docs/distribution.md) | CLI over `cdylib`, static musl over glibc, and the binary-size and cold-start numbers behind both. |
| [`architecture.md`](docs/architecture.md) | How the workspace is laid out, and where each decision lives. |

**The consequence.** One reference vector per character cannot work, and enumerating styles does not
rescue it. Clustering a title's own repeated shapes was the obvious escape — the expensive axes are
constant within a stream — and it was built, swept and **measured worse at every radius**: no radius
groups a title's variation without first merging characters the vector never separated, since `I`,
`l` and `|` sit at distance zero from one another.

What did work was aiming at *separation* rather than at variance. Measuring each glyph against its
own text line took 5.8 to 8.1 points off the error rate, because it adds the one thing normalisation
deliberately throws away.

---

## As a library

The CLI is a thin shell over `subtrackt::Pipeline`; everything of substance is in the library, so
shipping a `cdylib` instead would replace one crate rather than restructure the workspace.

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

## Licence

MIT. See [LICENSE](LICENSE).
