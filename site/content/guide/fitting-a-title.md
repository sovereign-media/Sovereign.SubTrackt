---
title: Fitting a title
label: Fitting a title
description: What fit decides for you, and the part everyone gets wrong, which is what it does not and cannot certify.
---

# Fitting a title

This film was drawn in one typeface and you don't know which. [`fit`](/usage/fit) asks.

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

It reads the first few hundred subtitles, cuts them into shapes the way a real extraction would,
and measures how far those shapes sit from the nearest entry in each candidate set. A shape nothing
in a set can account for is charged the worst possible distance, so a set that's simply missing
characters can't win by ignoring them. Lowest average wins.

Sampling the opening is enough because a typeface doesn't change halfway through a film. It takes a
few seconds against a multi-gigabyte file instead of a minute or more.

## What it decides, and what it doesn't

It decides which of the sets you gave it is closest to this material, and where the right typeface
is among your candidates it generally finds it. That's worth having: the gap between the right set
and a plausible wrong one is not subtle.

It doesn't decide whether the winner is any *good*. The score is relative — it ranks candidates
against each other and says nothing about whether the best of them is close enough to trust. Give
it ten typefaces when the disc was drawn in an eleventh and you still get a winner, still ranked
first, with a score that doesn't look obviously wrong. Scores only look wrong next to better ones.

## Why that can't be fixed

Six ways of grading a fit have been tried and all six failed structurally rather than for want of a
better idea. The short version:

Most candidate statistics are functions of the matcher's own answer, which is circular. A set
that's systematically wrong is, by construction, one that finds close matches — that's *why* it
produces wrong characters. The thing making the reading wrong is the thing making the evidence look
reassuring.

Avoid the circle by identifying the typeface from the shapes without matching anything, and you hit
a harder fact: a character decoded off a subtitle plane has drifted further from its own typeface
than typefaces sit apart from each other.

Judge the output text against what real language looks like and you're back in the circle, because
you can only score the words the model can score, and the ones it can't aren't a random sample.
They're concentrated where the reading went wrong.

Underneath all three: a tool can't grade its own reading without something to compare against, and
if you had that you wouldn't need the tool. [Telling a good fit from a bad
one](../docs/fit-confidence.md) has all six attempts.

## So read a few cues

Run the extraction, open the output, read the first page. Not for accuracy — for sanity.

A wrong typeface doesn't produce gibberish. It produces words that are almost right: `Iater`,
`rnodern`, `hoIiday`. Text that reads fine at a glance and falls apart when you look at it.

If it reads like English, or whatever it should be, you're done. Keep the `.subtref` beside the
film; refitting is cheap but you only need it once.

If it doesn't, [reading a track](/guide/reading-a-track) has the failures in order of how often
each one is the answer.
