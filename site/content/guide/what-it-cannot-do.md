---
title: What it cannot do
label: What it cannot do
description: The known limits, up front rather than buried at the bottom of a README.
---

# What it cannot do

All measured, all known, roughly in order of how likely each is to matter to you.

**You have to supply the reference set, and a near miss is not partial credit.** Nothing ships
embedded, so before anything works you need fonts and a `gen-reference` run — and it has to be the
material's own typeface *in the right weight*. *Excision* is set in Arial Bold; given Arial regular
and italic it reads at 24.8% with 2,666 unreadable characters, more than every other film in the
comparison combined. Given a folder that contains the bold weight, the same matcher finds it unaided
and reads 11.4%. [Why](/guide/reference-sets). Less lopsided than it sounds — the closest comparable
tool ships no character database in either of its Linux packages, and its documented way to aim at a
typeface is a human driving a training GUI.

**With a generic set, an OCR engine beats it.** Not marginally: 11.0% against 2.7% across 24 films.
If you can't get the right fonts, use OCR today; [the numbers](/guide/how-it-compares) say so in
those words.

**With a matched set it still loses on accuracy, by about a point.** Across those same 24 films
Subtitle Edit's Tesseract mode reads 2.7% against this tool's 3.5%, and head to head it's 2 wins to
7. What this tool is better at is [saying when it failed](/guide/how-it-compares) — which matters if
nobody is watching the pipeline, and matters much less if you are reading the output anyway.

**No accuracy figure on this site is objective, including the ones that flatter this tool.** A
subtitle is correct when a person reading it says so, and every percentage published here was
produced by scoring against a transcript some other person typed. Change the transcript and the
number changes — by up to 81 points on the films where that has been measured. Comparisons against
other tools are run the same way for every tool, which makes them fair to each other and still
leaves them a judgement rather than a measurement. Read them as "roughly this good", never as a
specification.

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
