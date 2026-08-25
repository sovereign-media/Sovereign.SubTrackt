---
title: Examples
label: Examples
description: Six whole jobs, start to finish. A box set, a library running unattended, a disc with three tracks, a DVD, a pipe, and the one where nothing fits.
---

# Examples

[Quick start](/usage/quick-start) does one film from an empty machine. These are the jobs after
that one.

[The last one](#when-nothing-fits) is verbatim — real commands against a short test file, real
output, real exit status. Its cue counts are small for that reason.

## A box set

Every episode of a season was authored in the same pass, so they share a typeface. Fit once, reuse
the set:

```console
$ subtrackt fit season1/S01E01.mkv --references ./sets -o season1.subtref
$ for ep in season1/*.mkv; do
>   subtrackt extract "$ep" --reference season1.subtref -o "${ep%.mkv}.en.srt" --report
> done
```

Fitting each episode separately would work and would waste a few seconds apiece deciding the same
thing. Keep `season1.subtref` next to the season — it's what makes a bad extraction diagnosable
later, and refitting a disc you've already fitted buys nothing.

Check the first episode's output by eye before letting the loop run. That's the whole of
[fitting a title](/guide/fitting-a-title).

## A library, unattended

The shape that behaves when nobody is watching:

```bash
for input in library/**/*.mkv; do
  out="${input%.mkv}.en.srt"
  [ -e "$out" ] && continue

  subtrackt extract "$input" \
      --reference "$(dirname "$input")/title.subtref" \
      --format srt \
      --output "$out" \
      --report \
      --plain \
    2>> extract.log \
    || burn_in_fallback "$input"
done
```

`--plain` keeps colour codes and progress frames out of the log. `--report` puts a line of counts
in it, which is what you grep three months later. `--output` rather than a redirect means a failed
run leaves no file behind to be mistaken for a good one.

The `||` is the part that matters. A track too badly read to trust exits non-zero and writes
nothing, so the fallback fires instead of a wrong subtitle shipping quietly. See
[the gate](/usage/extract#the-accuracy-gate).

## A disc with three tracks

Most retail discs carry more than one:

```console
$ subtrackt list movie.mkv
  0  hdmv_pgs_subtitle    eng   1920x1080  Full
  1  hdmv_pgs_subtitle    eng   1920x1080  Forced
  2  hdmv_pgs_subtitle    eng   1920x1080  SDH
```

The first column is what `--stream` takes. Leave it off and you get the first *bitmap* stream,
which is not always index 0 — a text track earlier in the file keeps its index and is skipped
rather than renumbering everything after it.

```console
$ subtrackt extract movie.mkv --reference movie.subtref -s 0 -o movie.en.srt
$ subtrackt extract movie.mkv --reference movie.subtref -s 1 -o movie.en.forced.srt
$ subtrackt extract movie.mkv --reference movie.subtref -s 2 -o movie.en.sdh.srt
```

One reference set covers all three, because they were drawn in the same pass.

Forced tracks are short, a hundred cues against a few thousand. The gate is a ratio so the floor
doesn't move, but a handful of unread glyphs is a far larger share of a small track, so a forced
track trips it on damage a full track would absorb. Working as intended, and worth knowing before
it surprises you.

## A DVD

VOBSUB comes as an `.idx` and a `.sub`. Point everything at the `.idx`; the `.sub` beside it gets
found:

```console
$ subtrackt list movie.idx
$ subtrackt fit movie.idx --references ./sets -o movie.subtref
$ subtrackt extract movie.idx --reference movie.subtref -o movie.en.srt --report
```

Identical from there on. DVD subtitles are drawn at a much lower resolution than Blu-ray, so
[check the output yourself](/guide/what-it-cannot-do) more readily than you would for a Blu-ray —
it's the least measured part of this.

## Without touching disk

No `-o`, and the subtitle goes to stdout. Everything else the tool says goes to stderr, so the pipe
stays clean:

```console
$ subtrackt extract movie.mkv --reference movie.subtref | grep -c "^"
$ subtrackt extract movie.mkv --reference movie.subtref | grep -n "red zone"
```

Useful for asking a question of a disc you don't want a file from. Does this cut carry this line,
how many cues are there, does it decode at all:

```console
$ subtrackt glyphs movie.mkv --summary
movie.mkv  hdmv_pgs_subtitle  lang=eng  1920x1080  cues=822  glyphs=20597  shapes=134
```

That last one runs the whole pipeline up to the matcher without a reference set anywhere, so it's
the cheapest way to find out whether a file is readable at all.

## When nothing fits

The one worth seeing, because it's the case the whole design is built around.

`fit` proposes a winner from whatever you gave it. Give it four candidates when the disc was drawn
in a fifth and it still ranks them, still prints a score, and the score doesn't look alarming:

```console
$ subtrackt fit sample.sup --references ./sets
6 cues, 128 glyphs, 50 distinct shapes

  reference set               score       read
  arial-ri                      9.9     100.0%
  verdana                      22.7      93.8%
  tahoma                       23.6      95.3%
  georgia                      36.1      74.2%
```

Hand it the wrong one and this is what comes out:

```console
$ subtrackt extract sample.sup --reference ./sets/georgia.subtref --on-unmatched placeholder
1
00:00:01,000 --> 00:00:03,000
T�o 9�!o� ör0�� f0x j���s

2
00:00:04,000 --> 00:00:06,000
0vor l�o !�zy ö09�
```

That's *The quick brown fox jumps over the lazy dog* read through the wrong typeface, with each
unread shape written out as `�`. Obvious here, because the mismatch is total. A near-miss is
the dangerous one: it produces `Iater` for `later` and `rnodern` for `modern`, which reads fine at
a glance.

You had to ask for that output, though. `--on-unmatched placeholder` was doing the work. Without
it:

```console
$ subtrackt extract sample.sup --reference ./sets/georgia.subtref -o sample.en.srt
error: extracting subtitles from sample.sup: track rejected by the threshold gate:
  95 of 128 glyphs read (74.2%), floor is 90.0%
$ echo $?
1
```

No file written, non-zero exit, and a message naming the number and the floor it missed. That's a
result the caller can act on. It's also the answer with no reference set at all:

```console
$ subtrackt extract sample.sup -o sample.en.srt
no --reference: nothing is embedded, so every glyph will come back unmatched
error: extracting subtitles from sample.sup: track rejected by the threshold gate:
  0 of 128 glyphs read (0.0%), floor is 90.0%
```

Either way you get told. [Why that's the point](/guide/why-not-ocr).
