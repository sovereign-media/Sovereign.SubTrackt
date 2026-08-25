---
title: subtrackt list
label: list
description: What subtitle tracks a file carries, which index to pass to --stream, and why a text track is not shown.
---

# `subtrackt list`

Ask a file what picture subtitle tracks it carries. This is where you start with any file you have
not met before, and it is the only command that reads nothing but the container's headers.

```console
$ subtrackt list movie.mkv
  0  hdmv_pgs_subtitle    eng   1920x1080  Full
  1  hdmv_pgs_subtitle    eng   1920x1080  Forced
```

## Usage

```console
$ subtrackt list <INPUT>
```

| Argument | Meaning |
| :--- | :--- |
| `<INPUT>` | The file to inspect. A container, a raw `.sup` dump, or a VOBSUB `.idx`/`.sub` pair — point it at the `.idx`. |

It takes no options of its own. The [global options](/usage/global-options) work here as everywhere.

## The columns

| Column | What it is |
| :--- | :--- |
| **Index** | The stream number. This is what `--stream` takes on `extract`, `fit` and `glyphs`. |
| **Codec** | `hdmv_pgs_subtitle` for Blu-ray PGS, `dvd_subtitle` for DVD VOBSUB. |
| **Language** | The container's own language tag, where it carries one. |
| **Size** | The subtitle plane, in pixels — not the video's resolution, though they usually agree. |
| **Title** | The track name the container carries, if any: `Full`, `Forced`, `SDH`, whatever the author wrote. |

Every other command defaults to the first picture track it finds, so on a file with one track you
never need `--stream` at all. On the file above, the forced track — signage and foreign dialogue
only — is stream `1`, and reading it means saying so.

## What it reads

| Format | Extension | Notes |
| :--- | :--- | :--- |
| Matroska | `.mkv` | The common case for a Blu-ray or DVD rip. |
| Transport stream | `.ts`, `.m2ts`, `.mts` | Broadcast captures and raw Blu-ray streams. |
| Raw PGS | `.sup` | A dumped PGS track with no container around it. |
| VOBSUB | `.idx` / `.sub` | DVD. Point it at the `.idx`; the `.sub` beside it is found. |

**MP4 is not supported.** Pointed at one, it says so and stops rather than guessing at the container.

## Why your text track is not listed

Because there is nothing here for it to gain.

A Matroska text track — `S_TEXT/UTF8`, `S_TEXT/ASS`, and the rest — is already the output this tool
exists to produce. It is skipped rather than reported, and a file carrying nothing but text tracks
fails with a demux error saying so. That failure is the correct answer: you want a muxer, not this.
[What this is](/guide/what-this-is) draws the line in full.

## When it finds nothing

```console
$ subtrackt list movie.mkv
no bitmap subtitle streams found
```

That is a **successful** run — the file was opened, read, and honestly has nothing this tool can
work with, which is a fact rather than a fault. The exit status is `0`. A file that could not be
opened or parsed at all is the other thing, and it exits `1` with the reason on stderr, so a script
can tell the two apart without reading either message.

Three things the empty answer usually means, in order of likelihood:

- **The rip did not include the subtitle tracks.** Many ripping presets drop them by default. Check
  the source.
- **The subtitles are burned into the video image**, so there is no track to find. Nothing can help
  here; the words are pixels of the film.
- **The tracks are text**, per the section above.

Note that `extract` and `fit` are stricter about the same file: with nothing to read they fail
rather than producing an empty subtitle, because an empty subtitle file and a subtitle track that
was never found are indistinguishable to whatever consumes them next.

Next: [`gen-reference`](/usage/gen-reference), which is the one-off you do before anything can be
read.
