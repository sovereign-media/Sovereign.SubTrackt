---
title: The pipeline
label: The pipeline
description: What happens between the file going in and the subtitle coming out, stage by stage, and which parts of it produce a number you can check.
---

# The pipeline

[What this is](/guide/what-this-is) gives the six-step version. This is the same journey with the
lid off — what each stage does, what comes out of it, and which of them produce a number you can
hold the run to.

None of this is needed to run the thing. [Quick start](/usage/quick-start) does that in seven
steps.

## Eight stages, in order

![The eight stages of the pipeline: demux, decode, binarize, segment, vectorize, match, assemble and write, with what each one does and what comes out of it, and the reference set feeding into the match stage.](/diagrams/pipeline.svg)

The counts on the right are from one real Blu-ray track, and they give the shape of the job: a
couple of thousand packets in, twenty-odd thousand character shapes in the middle, eight hundred
subtitles out.

### Getting the pictures back

**`demux`** opens the container and finds the subtitle track you asked for. A film file is a box
with numbered tracks in it — video, several audio languages, and often a lot of subtitles. There can
be dozens of them; one file in the library this was built against carries seventy.
[`list`](/usage/list) is the command that shows you them.

Only the track you named is pulled out; every other byte in the file is stepped over rather than
read. That is why a 5.5 GB rip takes about twenty seconds rather than several minutes.

**`decode`** turns those packets into pictures. They are not pictures yet: they are compressed
runs of colour, palettes that arrive separately from the image they colour, and single images sent
in several pieces. Decoding puts them back together into exactly what your player would have
pasted over the video. Blu-ray and DVD store this differently, so there are two decoders — but
what comes out of both is the same thing, a picture and two times.

This is the first place the project's one rule bites. A packet that ends halfway through an image
is **rejected**, never padded out with blank rows to make it decodable. A half-decoded subtitle
reads as a legitimately empty one, and an empty subtitle is indistinguishable from a subtitle that
had nothing to say.

### Turning a picture into shapes

**`binarize`** decides, for every pixel, ink or background.

That sounds trivial and isn't. Subtitle text is drawn with soft edges so it doesn't shimmer, and
almost always with a dark outline around it so it stays readable over a bright scene. So the
picture is full of in-between pixels that are neither clearly letter nor clearly background. This
stage commits to an answer for each one and hands on a picture with no greys left in it, so no
later stage has to re-litigate the decision. Whether the outline counts as part of the letter is
yours to say, with `--include-outline`.

**`segment`** cuts the mask into individual shapes, by finding blobs of ink that touch each other.

Two details are worth knowing, because both were decisions:

- Pixels that meet **only at a corner** count as touching. A diagonal stroke is one movement of a
  pen, and treating corner contact as a break would hand the next stage two half-letters instead
  of one `V`.
- An **accent is a separate blob**, since it doesn't touch the letter it belongs to. It gets put
  back. What tells a diaeresis apart from a colon is not the two dots — those are geometrically
  identical — but whether there's a full-height letter body sitting underneath them.

Letters that genuinely touch each other are the awkward case, and they get a second chance: a
shape nothing can read is retried as two characters that were joined, and the cut is kept only if
*both* halves then read cleanly. The report counts how often that rescued something.

### Turning a shape into a character

**`vectorize`** puts each shape onto a fixed 16 × 16 grid and records which of the 256 cells it
fills. That's the whole description of a character from here on: 256 yes-or-no answers.

Each cell measures how much of its patch of the original was ink, rather than sampling a single
point in the middle of it. The usual case is a large glyph shrinking onto a small grid, and
point-sampling at that ratio means whether a thin stroke survives depends on luck. Measuring area
means it survives in proportion to how much of it there was. The effect you want is that the same
character drawn at DVD size and at Blu-ray size lands in almost the same place on the grid, while
two different characters stay far apart — and both of those are checked by tests.

**`match`** compares that pattern against the [reference set](/guide/reference-sets) — the list of
shapes the tool knows. Distance is simply how many of the 256 cells disagree. Closest wins, and
only if it's close enough. There's more on this below, because it's where the design earns its
keep.

The same letters recur constantly through a film, so an answer, once worked out, is remembered for
the rest of the run. On a real track almost every shape is answered from memory rather than by
comparing against the set again, which is why `cache` in the report should always read close to
100%.

### Putting the text back together

**`assemble`** turns a bag of matched characters into a line someone can read. Characters go back
in reading order, top line first. Word spaces are worked out from the gaps actually observed on
*that* line, because a fixed number of pixels means different things at different resolutions —
and the gaps are measured between the ink rather than between bounding boxes, so a leaning line
doesn't lose its spaces.

That lean is measured too, and it is what decides whether a line comes out tagged as italic. The
tool reads the slant of the ink rather than looking anything up, so it works even against a
reference set with no italic entries in it.

This stage may also change a character it was handed. A drawn zero and a drawn capital `O` can be
the same shape; so can `1`, `l` and `I`. Where the shape genuinely cannot decide, the characters
around it can — and every such change is printed underneath the report, because a stage allowed to
rewrite text has to leave a trace. [`extract`](/usage/extract#post-correction) has the switches.

**`write`** attaches the times and produces SubRip or WebVTT. Where the format has a syntax for
it, the file also records what read it and what reference set was used, which is the half of a bad
extraction that is otherwise untraceable six months later.

## What happens to one shape

Stages 2 to 6, on a single subtitle:

![One subtitle picture reading CAFÉ, shown at five stages: decoded from the disc, reduced to solid ink, cut into four shapes with the accent reattached to its letter, one letter scaled onto a sixteen by sixteen grid, and that grid compared against the nearest entries in the reference set.](/diagrams/one-shape.svg)

Nothing in that sequence consults a dictionary, a language model or a training set. It is a
measurement of a shape against a list of shapes, and that is the entire mechanism.

## The three answers

`match` can end in three places, and the difference between them is the reason this tool exists
rather than an [OCR engine](/guide/why-not-ocr).

![Three panels showing the distance from a shape to the closest reference entry and to the runner-up. Matched: the closest is well within the limit and the runner-up is far behind. Ambiguous: the closest is within the limit but the runner-up is nearly as close. Unmatched: nothing is within the limit at all.](/diagrams/three-answers.svg)

**Matched** is the ordinary case. **Ambiguous** means it picked a winner but the second-placed
shape was nearly as close, so the character is written out and the count of near-calls is
reported. **Unmatched** means nothing the set knows came close at all — so nothing is written, and
the shape is counted.

That third answer is the whole design. An OCR engine has no equivalent: it would return the
nearest letter in all three cases, and attach a confidence score to the last one.

## No stage knows about its neighbours

Each of the eight is defined by what it must accept and what it must produce, and none of them
imports another. That has one practical consequence worth stating: **a stage that cannot do its
job returns an error saying so**, rather than crashing or improvising.

MP4 is the standing example. There is no MP4 reader, so asking for one gets a specific refusal
naming the missing feature. It does not get a guess at the container, and it does not get a panic
that takes a worker down with it — a caller can catch it and fall back.

## Where the numbers come from

Every run can print a line of counts. Each field belongs to a particular stage, and knowing which
is most of knowing what a bad number means.

![The eight stages along the bottom with the report fields stacked above the stage that produced each. Demux produces the packet count, decode the picture count, segment the de-fused count, match the matched, unmatched and ambiguous counts along with the percentage read, the fit and the cache rate, and assemble the cue count and correction count. Binarize, vectorize and write produce none.](/diagrams/where-the-numbers-come-from.svg)

Three stages produce no number at all, and that is not an oversight — there is nothing countable
about deciding that a pixel is ink.

The two worth watching are marked. **`unmatched`** is the count of specific characters the tool
declined to guess at. **`fit`** is how far, on average, the shapes that matched sat from the
entries they matched. Coverage tells you how many shapes found *an* answer; `fit` tells you how
well they fitted it, and a mean creeping up toward the limit is what a track being read
confidently and wrongly looks like. [Reading a track](/guide/reading-a-track) is the field guide.

Then the gate. If too little of the track could be read, the run **fails and writes no file**,
which is what makes this safe to leave unattended: the calling script gets a non-zero exit rather
than a plausible-looking subtitle. The floor and its four settings are on
[`extract`](/usage/extract#the-accuracy-gate).

## The two rulers

Everything above says how far a run got. Neither those counts nor anything else the tool prints
says whether the *characters that came out are the right ones* — that needs text to compare
against, and an extraction has none. So there are two separate instruments, and they measure
different things.

![Two panels. The ceiling: one font draws the pictures and builds the reference set, the pipeline reads them, and the result is scored against the words that were drawn in the first place. The floor: nine real disc tracks each with a transcript typed by a person, read before and after a change, scored, and the lines the change made worse are counted.](/diagrams/two-rulers.svg)

The **ceiling** is a fixture the project generates itself. Because one font both draws the pictures
and builds the reference set, a typeface mismatch is impossible by construction — so whatever
error remains belongs to the tool rather than to the disc. Real material can only do worse, which
is precisely what makes it an upper bound rather than a result.

The **floor** is nine tracks off real discs, each paired with a subtitle file somebody typed by
hand. That pairing is the part that most often goes wrong: a transcript written to a different
convention than the disc — sound effects in brackets against a transcript carrying none — scores
real improvements as *regressions*, which is worse than measuring nothing at all. Two of those
nine were paired wrongly for a long time, and what found them was checking every candidate
transcript against the shape of the extraction rather than reading the scores.

And the number to read there is the count of lines a change made **worse**, not the average. An
average can improve across nine tracks while one disc quietly gets ruined; that has happened, and
it was caught only because more than one disc was being scored.

## The thing neither ruler can do

Neither of them can tell you that the reference set you picked is the right one for the disc in
front of you.

A set built from a near-miss typeface matches almost everything, sits at a plausible distance,
clears the gate comfortably, and reads a good fraction of the text wrongly. Six different
statistics have been tried against that problem and all six failed for structural rather than
fixable reasons. So the tool reports the choice instead of making it.

That's [Fitting a title](/guide/fitting-a-title), and the practical instruction at the end of it is
one line: read a few cues before trusting a track to a set.

---

The implementation notes behind this page, including what was measured and what was rejected, are
[Architecture](../docs/architecture.md#the-pipeline). Why a fixed list of shapes can't work on its
own is [Glyph stability](../docs/glyph-stability.md); why nothing ships embedded is
[Reference sets](../docs/reference-set.md).
