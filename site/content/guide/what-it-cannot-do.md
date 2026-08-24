---
title: What it cannot do
label: What it cannot do
description: The known limits, in plain language and up front rather than buried at the bottom of a README.
---

# What it cannot do

Every one of these is known, measured, and stated here rather than discovered by you later. They
are roughly in order of how likely each is to matter to you.

## You have to supply the reference set

This is the big one, and it is what stands between this tool and working the moment you install it.

There is no set embedded, so before you do anything you need fonts and a `gen-reference` run. And
the tool reads best when the set comes from the material's **own** typeface — not a similar one.
Reading material drawn in one typeface using a metrically-similar substitute is not nearly as good
as it sounds, and "metrically similar" is doing less work than people expect.

The reasoning is on [the reference sets page](/guide/reference-sets), and the short version is that
the alternative — shipping a set and letting near-misses through — trades a failure you can detect
for one you cannot. Fitting a set per title is as far as this can be taken without already knowing
the right answer, and that is what `fit` does.

It is also worth saying that this is less lopsided than it reads. Comparable tools need something
supplied too: the closest one ships no character database at all in either of its Linux packages,
and the documented way to aim its matcher at a particular typeface is a human driving a training
GUI.

## Out of the box, with a generic set, an OCR engine beats it

Stated plainly because it is true and measured: if you cannot supply the typeface the material was
drawn in, and you use a generic set instead, a good OCR engine reads these discs better than this
does — not marginally, but by a wide margin.

The trade only pays when the set fits. If you have no way to get the right fonts, use OCR today.
[Five other tools](/research/alternatives) says so in those words, with the numbers.

## Nothing can tell a good fit from a bad one

`fit` ranks candidates against each other and cannot certify the winner. This is not a missing
feature; several approaches have been tried and each failed for a structural reason, laid out on
[the fitting page](/guide/fitting-a-title) and in full in
[Telling a good fit from a bad one](/research/fit-confidence).

The practical consequence is one instruction: **read a few cues before trusting a track to a set.**

## Style is finer-grained than typeface

Getting the typeface right is not the same as getting the *drawing* right. A set built only from the
upright cut of the correct font still reads a film's italic passages markedly worse than its
dialogue.

`gen-reference --italic --bold` closes most of that when you have the other font files, and where
you do not, the pipeline measures how far a line leans and compensates. Between them the gap gets
small. It does not get closed, and nothing can choose automatically between the two approaches for
a given title, for the same reason nothing can grade a fit. See [The slant](/research/italic-slant).

## What is left is cutting, not recognising

The errors that remain on well-fitted material are mostly not the matcher misreading a shape. They
are the stage before it, cutting a picture into shapes, getting the cut wrong:

- **A double quotation mark arriving as two apostrophes**, because the two marks were never joined.
- **A word space that was never cut**, so two words run together.
- **Two characters that touch** being read as one shape. There is a recovery pass for this and it
  catches most of them.

These are being worked on and they are the current frontier.

## Word spacing collapses on some all-caps lines

`MAN ON INTERCOM: The red zone is` can come out as `MANONINTERCOM:Theredzoneis`.

Seen mostly on speaker labels in subtitles for the deaf and hard of hearing. It is not yet
explained, and it is the most visible unexplained defect in the output.

## Formats it does not read

It reads Matroska (`.mkv`), broadcast transport streams (`.ts`, `.m2ts`, `.mts`), raw PGS dumps
(`.sup`), and DVD VOBSUB (`.idx`/`.sub` — point it at the `.idx`).

**MP4 is not supported.** Pointed at one, it says so and stops, rather than guessing at the
container and producing something wrong.

And, as covered on [the first page](/guide/what-this-is), tracks that are already text are out of
scope entirely, as are subtitles burned into the video image.

## DVD-era material is the least measured part

The DVD format decodes and produces output, and far less measurement exists for it than for
Blu-ray. It is lower resolution and older, which is exactly the combination most likely to behave
differently — so it is the corner where you should be most inclined to check the output yourself.
The first real measurement of it is [What VOBSUB reads at](/research/vobsub).

## It is alpha

Every release so far is a pre-release, and the command-line surface is not frozen. Pin a version if
you are automating against it.

---

If none of the above is a blocker for you, go back to [what this is](/guide/what-this-is) and run
the three commands. If one of them is, that is the point of this page — better to find out here.
