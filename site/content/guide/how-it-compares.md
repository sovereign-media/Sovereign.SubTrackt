---
title: How it compares
label: How it compares
description: Five other tools read the same three Blu-rays. What that showed, and what it did not.
---

# How it compares

The previous page argues that a tool able to say *I don't know* is worth more than one that is
usually right. Here is what happened when that was measured.

Five other tools read the same three Blu-ray discs: Subtitle Edit's command-line converter driving
Tesseract, the same converter in two of its other modes, pgsrip, and PgsToSrt. Every tool got the
same subtitle streams, every result was scored by the same program, and every run happened one at a
time on the same machine.

## The numbers

Three discs, 4,660 subtitles between them.

| tool | characters wrong | worst of the three discs | CPU time for all three |
| :--- | ---: | ---: | ---: |
| **SubTrackt**, typeface matched | **1.1%** | **1.3%** | **4 seconds** |
| PgsToSrt | 1.4% | 1.8% | 27 minutes |
| Subtitle Edit, Tesseract | 1.4% | 1.9% | 12 minutes |
| pgsrip | 1.6% | 3.5% | 11 minutes |
| **SubTrackt**, wrong typeface | 9.8% | 10.5% | 3 seconds |

Two of Subtitle Edit's other modes are missing from that table because they read almost nothing.
Both work from a database of shapes that is downloaded separately and trained by hand, and with the
stock database 97 characters in every 100 came back wrong. Neither is a fair test of Subtitle Edit,
and both are a fair warning about what "supports this format" can mean.

## The accuracy column proves less than it looks like it proves

Ground truth for a real Blu-ray does not exist. What exists is subtitle files other people made
from the same discs, and those disagree with the disc and with each other. Scoring one tool against
a different file from the same folder moved its result by up to 14 points on two of these three
discs.

Every gap in the accuracy column is a fraction of a point, so all of them are inside that noise.
The honest reading is that four of these tools read a clean Blu-ray about equally well, and no
ranking between them survives the measurement.

Two things do survive it.

## Cost is not a rounding error

These three discs took SubTrackt four CPU-seconds. The cheapest of the others took nearly nine
minutes and PgsToSrt took twenty-seven. Nothing was bought with that time: the tools spending two
orders of magnitude more CPU are not more accurate for it, so on this corpus there is no trade to
weigh up.

The reason is the design rather than any cleverness. Comparing a shape against a few hundred known
shapes is arithmetic, and the same letter in the same film is the same shape every time, so most
characters are answered from a cache without any comparison at all.

## The wrong typeface costs more than every other difference combined

The last row is the one to take seriously. Give SubTrackt a reference set built from a typeface the
disc does not use and it reads at 9.8% instead of 1.1%, far worse than anything else in the table.
That is what refusing to guess costs when the shapes you know are the wrong ones, and it is real.

The whole of [fitting a title](/guide/fitting-a-title) is about avoiding that row. It is also why
nothing ships embedded: a set that quietly does not match your material would turn a visible
failure into an invisible one.

## The column nobody else has

None of the five can tell you it failed.

They have no way to report a shape they did not recognise, because they always recognise something.
Subtitle Edit's shape-database modes come closest, dropping a `*` into the text where the database
found nothing. That is a marker in the middle of a subtitle rather than a count, so a program
calling the tool cannot act on it, and cannot tell it apart from a `*` the disc really displayed.

SubTrackt reports unread characters per subtitle and per track, and will stop the run outright if
you ask it to. The table above cannot show that difference, because there is no column for a tool
that never fails out loud.

## The full version

The complete write-up has the method, all seven tables, the versions of everything, the ways the
comparison is unfair and to whom, and the predictions that were written down before the runs and
scored afterwards. It lives in the repository, at
[docs/alternatives.md](../docs/alternatives.md).
