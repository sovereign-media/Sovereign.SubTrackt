---
title: How it compares
label: How it compares
description: Five alternative tools read the same 24 films. On accuracy SubTrackt comes second, and the reason is one film.
---

# How it compares

Five alternative tools read the same 24 films: Subtitle Edit's command-line converter driving
Tesseract, the same converter in two of its other modes, pgsrip, and PgsToSrt. Same subtitle streams,
same scoring program, one run at a time on the same machine.

**24 films, 33,755 subtitles.**

| tool | characters wrong | worst film | CPU |
| :--- | ---: | ---: | ---: |
| Subtitle Edit, Tesseract | **2.7%** | **10.0%** | 18 minutes |
| **SubTrackt**, typeface fitted from 128 | 3.3% | 11.4% | **7 seconds** |
| **SubTrackt**, one matched typeface | 3.5% | 24.8% | **5 seconds** |
| PgsToSrt | 3.7% | 27.3% | 37 minutes |
| pgsrip | 7.8% | 22.3% | 14 minutes |
| **SubTrackt**, wrong typeface | 11.0% | 16.8% | 5 seconds |

Subtitle Edit's other two modes aren't in the table because they read almost nothing. Both want a
shape database that's downloaded separately and trained by hand, and with the stock one, 97
characters in every 100 came back wrong. Not a fair test of Subtitle Edit, but a fair warning about
what "supports this format" can mean.

## Subtitle Edit reads more accurately than SubTrackt

Head to head across the 24 films it's 2 wins to SubTrackt, 7 to Subtitle Edit, and 15 too close to
call.

The 24 films are drawn from a real library by a rule that doesn't look at what any tool makes of
them, which matters: a handful of discs picked for suiting one tool will flatter it, and a result
that rests on three films rests on nothing.

## The whole gap is one film

Take *Excision* out of the table and SubTrackt leads both columns — 2.6% against 2.7%, worst film
8.7% against 10.0%. It's left in, because a film your subtitle tool falls over on is exactly the
thing you'd want to know about. But it's worth knowing *why* it falls over.

*Excision* is set in **Arial Bold**. The shapes SubTrackt was given were Arial regular and Arial
italic. That one missing weight accounts for 2,666 unreadable characters — more than every other film
in the comparison put together. Point the same matcher at a folder containing the bold weight and it
finds it by itself, and the film goes from 24.8% to 11.4%.

So the ceiling here isn't the matching. It's whether somebody built the right set of shapes, which is
what [fitting a title](/guide/fitting-a-title) is for and why [nothing ships
embedded](/guide/reference-sets).

## No tool here is good on all 24

Every alternative tool's bad films are more than three times worse than its typical one. PgsToSrt
matches SubTrackt's typical film exactly and then reads *Lilo & Stitch* at 27.3%. pgsrip reads *How
to Train Your Dragon* at 22.3%.

That's the honest shape of the result: nobody in this table reads a library evenly, SubTrackt
included.

## The accuracy column is a judgement, not a measurement

**Whether a subtitle is right is something a person decides by reading it.** There's no ground truth
for a real Blu-ray. What exists is subtitle files other people typed from the same discs, and those
carry one human's choices — where a line breaks, whether a sound cue reads `[DOOR OPENS]` or a
musical note, whether the speaker label is there at all. They disagree with the disc and with each
other. Scoring one tool against a different file from the same folder moves its result by up to **81
points** on one of these films.

So a figure in that column isn't "how much this tool got wrong". It's "how far this tool's reading sat
from one particular person's reading", and choosing a different person moves it.

**Every tool was scored against the same transcript, which makes the comparison fair — not
objective.** Where a tool and the transcript disagree about a convention rather than about a
character, the tool is charged for it just the same.

On any single film that noise swamps the gaps. What twenty-four films buy is a result that doesn't
rest on any one of them: a 2-7 record is something no single film's transcript can undo.

## Cost isn't close

Five CPU-seconds against fourteen minutes for the cheapest alternative and thirty-seven for PgsToSrt.
The two tools that read more accurately spend **166 and 221 times the CPU** to gain about a point.

That's the design rather than any cleverness. Comparing a shape against a few hundred known shapes is
arithmetic, and the same letter in the same film is the same shape every time, so most characters
come from a cache without any comparison at all.

Choosing the right typeface is cheap too: scanning 128 candidate typefaces instead of six costs two
CPU-seconds across five films, and it's what fixes the worst result in the table.

## Nobody else has a failure column

None of the five can tell you it failed. They've no way to report a shape they didn't recognise,
because they always recognise something. Across 33,755 subtitles the four Tesseract-based tools
reported **zero** unreadable characters. SubTrackt reported 4,434, each with a location — and on
*Excision*, given the wrong typeface entirely, it refused the film rather than hand back a file.
pgsrip read that same film and returned clean-looking subtitles with no sign anything was unusual.

Subtitle Edit's database modes come closest, dropping a `*` into the text where the database found
nothing. That's a marker in the middle of a subtitle rather than a count, so a program can't act on
it and can't tell it apart from a `*` the disc really displayed.

This is the part of the comparison the tool exists for. If you're reading subtitles by hand, accuracy
is what matters and Subtitle Edit is the better answer. If you're running a pipeline over a library
with nobody watching, a tool that says "I couldn't read this one" is worth more than a point of
character error.

## The full version

Method, seven tables, versions of everything, where the comparison is unfair and to whom, and the
predictions written down before the runs and scored after:
[docs/alternatives.md](../docs/alternatives.md).
