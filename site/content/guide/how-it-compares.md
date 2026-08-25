---
title: How it compares
label: How it compares
description: Five other tools read the same three Blu-rays. What that showed, and what it did not.
---

# How it compares

Five other tools read the same three Blu-rays: Subtitle Edit's command-line converter driving
Tesseract, the same converter in two of its other modes, pgsrip, and PgsToSrt. Same subtitle
streams, same scoring program, one run at a time on the same machine.

Three discs, 4,660 subtitles. **The accuracy column is a judgement rather than a measurement**,
for reasons worth having in hand before quoting any figure in it.

| tool | characters wrong | worst of the three discs | CPU for all three |
| :--- | ---: | ---: | ---: |
| **SubTrackt**, typeface matched | **1.1%** | **1.3%** | **4 seconds** |
| PgsToSrt | 1.4% | 1.8% | 27 minutes |
| Subtitle Edit, Tesseract | 1.4% | 1.9% | 12 minutes |
| pgsrip | 1.6% | 3.5% | 11 minutes |
| **SubTrackt**, wrong typeface | 9.8% | 10.5% | 3 seconds |

Subtitle Edit's other two modes aren't in the table because they read almost nothing. Both want a
shape database that's downloaded separately and trained by hand, and with the stock one, 97
characters in every 100 came back wrong. Not a fair test of Subtitle Edit, but a fair warning about
what "supports this format" can mean.

## The accuracy column is a judgement, not a measurement

**Whether a subtitle is right is something a person decides by reading it.** There's no ground
truth for a real Blu-ray. What exists is subtitle files other people typed from the same discs, and
those carry one human's choices — where a line breaks, whether a sound cue reads `[DOOR OPENS]` or
a musical note, whether the speaker label is there at all. They disagree with the disc and with
each other. Scoring one tool against a different file from the same folder moved its result by up
to 14 points on two of these three discs.

So a figure in that column isn't "how much this tool got wrong". It's "how far this tool's reading
sat from one particular person's reading", and choosing a different person moves it.

**Every tool here was scored against the same transcript, which makes the comparison fair — not
objective.** All five are being measured against a judgement call, so the ranking inherits the
judgement. Where a tool and the transcript disagree about a convention rather than about a
character, the tool is charged for it just the same.

Every gap in the column is a fraction of a point, well inside that noise. Four of these tools read
a clean Blu-ray about equally well, and no ranking between them survives the measurement. If you
need to know which one reads *your* material better, run both over a track you care about and read
the output — that's the same instrument, applied honestly, and it's the only one there is.

Two things in the table do survive.

## Cost isn't close

Four CPU-seconds against nine minutes for the cheapest competitor and twenty-seven for PgsToSrt,
and nothing was bought with the difference — the tools spending two orders of magnitude more CPU
aren't more accurate for it.

That's the design rather than any cleverness. Comparing a shape against a few hundred known shapes
is arithmetic, and the same letter in the same film is the same shape every time, so most
characters come from a cache without any comparison at all.

## The wrong typeface costs more than everything else combined

Look at the last row: 9.8% against 1.1%, worse than anything else in the table. That's what
refusing to guess costs when the shapes you know are the wrong ones.

Avoiding that row is what [fitting a title](/guide/fitting-a-title) is for, and it's why
[nothing ships embedded](/guide/reference-sets).

## Nobody else has a failure column

None of the five can tell you it failed. They've no way to report a shape they didn't recognise,
because they always recognise something.

Subtitle Edit's database modes come closest, dropping a `*` into the text where the database found
nothing. That's a marker in the middle of a subtitle rather than a count, so a program can't act on
it and can't tell it apart from a `*` the disc really displayed.

SubTrackt counts unread characters per subtitle and per track, and will stop the run outright if
you ask. The table can't show that, because there's no column for a tool that never fails out loud.

## The full version

Method, seven tables, versions of everything, where the comparison is unfair and to whom, and the
predictions written down before the runs and scored after:
[docs/alternatives.md](../docs/alternatives.md).
