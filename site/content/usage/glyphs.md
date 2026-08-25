---
title: subtrackt glyphs
label: glyphs
description: Dumping the shapes a track is made of, without reading them. The command behind the typeface survey, and the one to reach for when an extraction makes no sense.
---

# `subtrackt glyphs`

Dumps the character shapes a subtitle track is made of, **without trying to read them**. No
reference set is involved and nothing is matched against anything. The pipeline stops one stage
short of the matcher and hands you what it found.

```console
$ subtrackt glyphs movie.mkv --limit 200 > shapes.tsv
$ subtrackt glyphs movie.mkv --summary
```

Most people never need this. It is here because two of the measurements that shaped the tool were
built on it, and because it is the right instrument when an extraction is wrong in a way the report
cannot explain.

## Usage

```console
$ subtrackt glyphs [OPTIONS] <INPUT>
```

| Argument | Meaning |
| :--- | :--- |
| `<INPUT>` | The file to read. Same formats [`list`](/usage/list) reads. |

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `-s`, `--stream <N>` | first picture track | Which subtitle stream to read. |
| `-l`, `--limit <N>` | no limit | Stop after this many cues. |
| `--include-outline` | off | Include the glyph outline in the foreground mask as well as the fill. |
| `--summary` | off | Print a one-line summary to stderr instead of per-glyph rows. |

Plus the [global options](/usage/global-options).

## What comes out

One tab-separated row per glyph, on stdout:

| Column | Meaning |
| :--- | :--- |
| `cue` | Which subtitle the shape came from. |
| `line` | Which line within that subtitle. |
| `x`, `y` | Where the shape sits on the subtitle plane. |
| `width`, `height` | Its size in pixels. |
| *vector* | The 256-bit feature vector, as hex. The same description the matcher would have compared against a reference set. |

Because the vector is produced by the same normalisation everywhere in the pipeline, **vectors are
comparable across files**. Two shapes from two different discs can be measured against each other
directly, which is what made the typeface survey and the reference-set work possible at all.
[Typeface survey](../docs/library-survey.md) and
[Glyph vector stability](../docs/glyph-stability.md) are both built on this output.

`--limit` matters here for the same reason it does on [`fit`](/usage/fit): cues are spread evenly
through a film, so a few hundred touches only that fraction of a multi-gigabyte file, and a typeface
does not change halfway through.

## The summary line

Every run prints one line to stderr before the rows, naming the file, the codec, the language, the
plane, and three counts:

```console
$ subtrackt glyphs movie.mkv --summary
movie.mkv  hdmv_pgs_subtitle  lang=eng  1920x1080  cues=822  glyphs=20597  shapes=134
```

`--summary` suppresses the rows and leaves that line alone. It is the cheap "does this file
decode?" check: it exercises demux, decode, binarisation and segmentation with no reference set
anywhere in the picture, and stdout stays empty, so it costs nothing to run against a
multi-gigabyte file.

`shapes` is the count of *distinct* shapes, roughly how much alphabet the track uses, before you
have committed to a reference set for it.

## When to reach for it

**An extraction is wrong and the report looks fine.** Everything up to the matcher is visible here.
If the shapes are wrong (glyphs fused together, a line cut in the wrong place, a count nothing like
the number of characters on screen) then the problem is upstream of the reference set, and no
amount of refitting will help.

**You are deciding whether a title needs a new reference set.** The `shapes` count on the summary
line tells you roughly how much alphabet the track uses before you have committed to anything.

**You are building reference material.** This is the raw form the survey work consumed, and it is
the same output a new measurement would start from.

For what the vector actually is and why it is shaped that way, see
[Architecture](../docs/architecture.md).

Next: [Global options](/usage/global-options), which apply to every command on this list.
