---
title: What it cannot do
label: What it cannot do
description: The known limits, up front rather than buried at the bottom of a README.
---

# What it cannot do

All measured, all known, roughly in order of how likely each is to matter to you.

**You have to supply the reference set.** Nothing ships embedded, so before anything works you need
fonts and a `gen-reference` run, and it reads best from the material's *own* typeface rather than a
similar one. [Why](/guide/reference-sets). Less lopsided than it sounds — the closest comparable
tool ships no character database in either of its Linux packages, and its documented way to aim at
a typeface is a human driving a training GUI.

**With a generic set, an OCR engine beats it.** Not marginally. If you can't get the right fonts,
use OCR today; [the numbers](/guide/how-it-compares) say so in those words.

**Nothing can tell a good fit from a bad one.** `fit` ranks candidates against each other and can't
certify the winner, and [that can't be fixed](/guide/fitting-a-title). So: read a few cues before
trusting a track to a set.

**Style is finer-grained than typeface.** The right font's upright cut still reads a film's italic
passages worse than its dialogue. `--italic` and `--bold` close most of the gap when you have the
files, and the slant estimator covers some of the rest, but nothing closes it and nothing can pick
between the two approaches automatically. [The slant](../docs/italic-slant.md).

**What's left is cutting, not recognising.** On well-fitted material the remaining errors mostly
aren't the matcher misreading a shape — they're the stage before it cutting in the wrong place. A
double quotation mark arriving as two apostrophes, a word space that was never cut, two touching
characters read as one. There's a recovery pass for the last of those and it catches most. This is
the current frontier.

**Word spacing collapses on some all-caps lines.** `MAN ON INTERCOM: The red zone is` comes out as
`MANONINTERCOM:Theredzoneis`. Mostly speaker labels in subtitles for the deaf and hard of hearing.
Not yet explained, and the most visible unexplained defect in the output.

**No MP4.** Matroska, transport streams, raw `.sup` and VOBSUB `.idx`/`.sub` only. MP4 is refused by
name rather than guessed at. Tracks that are already text and subtitles burned into the video are
[out of scope entirely](/guide/what-this-is).

**DVD-era material is the least measured part.** It decodes and produces output, with far less
measurement behind it than Blu-ray. Lower resolution and older is the combination most likely to
behave differently, so it's where you should be most inclined to check the output yourself.
[What VOBSUB reads at](../docs/vobsub.md) is the first real measurement.

**A frozen surface isn't a frozen output.** From `v1.0.0` flags and output formats change on a
major and not before. That doesn't promise the same disc reads the same way forever — accuracy is
the ongoing work, so a minor that reads a cue better has changed your output without changing a
flag. Pin a version if you're diffing extractions rather than just running them.

---

If none of that is a blocker, [Quick start](/usage/quick-start) is the shortest way to a subtitle
file. If one of them is, better to find out here.
