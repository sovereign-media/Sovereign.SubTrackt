---
title: Quick start
label: Quick start
description: From an empty machine to a subtitle file, in four commands, with the one prerequisite spelled out before you hit it.
---

# Quick start

Four commands take you from nothing to a `.srt`. You run one of them once ever and the other three
per film.

One thing surprises people, so it is here before you start rather than after: **SubTrackt ships
with no reference set, so until you build one every character comes back unread and you get no
output.** That is deliberate, and [Reference sets](/guide/reference-sets) has the reasoning. In
practice it means step 3 is not optional and you need some font files.

## 1. Install

Tagged releases carry a self-contained binary for Linux and Windows, x86-64 and ARM64, with a
`SHA256SUMS` covering the set. The Linux builds are statically linked against musl: no glibc
version to match, no runtime to install, no model files. The artifact is the whole dependency.

Take the tag from the
[releases page](https://github.com/sovereign-media/Sovereign.SubTrackt/releases) and put it in
`$tag` below. It is named rather than resolved through `/releases/latest/`, because the asset
filename carries the version, so a `latest` URL would have to guess it.

**Linux**

```console
$ tag=v1.0.0
$ base=https://github.com/sovereign-media/Sovereign.SubTrackt/releases/download/$tag
$ curl -LO $base/subtrackt-$tag-x86_64-unknown-linux-musl
$ curl -LO $base/SHA256SUMS && sha256sum -c --ignore-missing SHA256SUMS
$ install -m 755 subtrackt-$tag-x86_64-unknown-linux-musl /usr/local/bin/subtrackt
```

**Windows**, in PowerShell:

```powershell
$tag = 'v1.0.0'
$base = "https://github.com/sovereign-media/Sovereign.SubTrackt/releases/download/$tag"
Invoke-WebRequest "$base/subtrackt-$tag-x86_64-pc-windows-msvc.exe" -OutFile subtrackt.exe
Invoke-WebRequest "$base/SHA256SUMS" -OutFile SHA256SUMS
Get-FileHash subtrackt.exe -Algorithm SHA256   # compare against the matching line in SHA256SUMS
```

Four targets are published per tag: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
`x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`. There is no macOS build because nobody has
asked for one.

## 2. Check what you installed

```console
$ subtrackt --version
subtrackt 1.0.0 (reference set: empty, 0 glyphs)
```

Two things decide what an extraction says: the code, and the reference data it matched against.
The version string names both. `reference set: empty, 0 glyphs` is not a fault, it is the shipped
state, and it is why the next step exists.

## 3. Build reference sets from fonts you have

Point [`gen-reference`](/usage/gen-reference) at a directory of fonts and get one candidate set per
font:

```console
$ subtrackt gen-reference /usr/share/fonts ./sets
```

On Windows that directory is `C:\Windows\Fonts`. Each set is a few kilobytes, so collect broadly.
You are not choosing anything yet, only assembling a library of candidates for `fit` to rank. This
is the one-off, and you come back to it when you meet material nothing in the library fits.

If you already have the italic and bold cuts of a typeface, a set that carries them reads a film's
italic passages markedly better. That is one font at a time rather than a directory:

```console
$ subtrackt gen-reference Arial.ttf ./sets/arial.subtref --italic Ariali.ttf --bold Arialbd.ttf
```

## 4. See what is in the file

```console
$ subtrackt list movie.mkv
  0  hdmv_pgs_subtitle    eng   1920x1080  Full
  1  hdmv_pgs_subtitle    eng   1920x1080  Forced
```

The first column is the stream index, which is what `--stream` takes elsewhere. Only picture tracks
are listed. A text track in the same file is skipped, because there is nothing here for it to gain.

If this command reports no streams, stop here. The rest of the pipeline has nothing to read. See
[`list`](/usage/list).

## 5. Pick the set that fits this title

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

A few seconds even against a multi-gigabyte file, because it samples the opening rather than
reading the whole film. `-o` copies the winner out ready for the next step.

Read the caveat under the table. It is the honest limit of this command rather than boilerplate:
the score ranks the candidates **against each other** and cannot certify the winner.
[`fit`](/usage/fit) has the detail.

## 6. Read the track

```console
$ subtrackt extract movie.mkv --reference movie.subtref --format srt -o movie.en.srt --report
822 cues from 822 images (1644 packets); glyphs 20597 matched / 17 unmatched
  / 2237 ambiguous (99.9% read); fit 10.7; cache 100%; defused 87; corrections 3 (context)
```

You now have `movie.en.srt`. `--report` is the line on stderr, and it is worth asking for every
time. It counts what the tool did and what it declined to do, which is what makes this different
from an OCR run. [`extract`](/usage/extract) reads the fields one at a time.

Drop `--report` and `-o` and the subtitle goes to standard output, so it pipes.

## 7. Read a few cues

This is a real step, it takes a minute, and skipping it is the most common way to end up with a bad
file and not know.

Open the output and read the first page. Not for accuracy, for sanity. You are looking for the
signature of a near-miss typeface, which is not gibberish but words that are almost right: `Iater`,
`rnodern`, `hoIiday`. A wrong set produces text that reads fine at a glance.

If it looks right you are done, and the `.subtref` is now a property of that title, so keep it
beside the film. If it does not, [Reading a track](/guide/reading-a-track) has the failures in order
of how often each one is the answer.

## The whole thing

```console
$ subtrackt gen-reference /usr/share/fonts ./sets                                  # once
$ subtrackt list movie.mkv                                                         # per title
$ subtrackt fit movie.mkv --references ./sets -o movie.subtref
$ subtrackt extract movie.mkv --reference movie.subtref -o movie.en.srt --report
```

If you already know the typeface, skip `fit` and hand the set straight to `extract`.

## Where to go next

- **It did not work, or you want to know why it is built this way.**
  [How it works](/guide/what-this-is) covers that in seven short pages.
- **You want every flag.** The rest of this section is one page per command, starting with
  [`list`](/usage/list).
- **You are automating this.** [Global options](/usage/global-options) covers exit status, what goes
  to which stream, and turning the colour and progress off.
