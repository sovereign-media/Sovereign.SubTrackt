---
title: What this is
label: What this is
description: What a bitmap subtitle track is, why it is a picture rather than text, and what comes out the other side.
---

# What this is

SubTrackt reads the subtitles off a Blu-ray or DVD and turns them into a text file.

There is more work in that sentence than it looks. This page unpacks it, and by the end you will
know what the tool is for, why it has to exist, and what it hands you.

## Subtitles come in two kinds

A subtitle file you download for a film is **text**. Open it in a text editor and you can read it:
a line of words, a start time, an end time, over and over. A computer can search it, translate it,
restyle it or feed it to anything, because the words are in the file.

A subtitle track on a **disc** is usually not that. It is a sequence of **pictures**.

For each subtitle the disc carries a small image of the words already drawn, with the typeface,
colour, outline and screen position baked in, plus the time to put it up and the time to take it
down. Your player never draws the subtitle. It pastes a picture over the video.

Nowhere in that file is there a letter `a`. There is an arrangement of pixels that a human reads as
an `a`.

## Why a disc does it that way

Because the disc has to look the same on every player ever built.

If the disc carried text, every player would need the right font, and would have to lay the line
out, break it, position it and draw it. Every player would do that slightly differently, or would
be missing the font, or would be missing the letters some language needs. Studios did not want a
subtitle that looks one way on a set-top box in one country and another way on a games console
somewhere else, and they very much did not want one that comes out as empty boxes.

So the studio draws the subtitle once, when the disc is authored, and ships the drawing. Every
player gets it pixel for pixel, because every player is only copying pixels. That is the right call
for a disc, and it is why this tool has to exist.

You will meet two formats: **PGS** on Blu-ray and **VOBSUB** on DVD. They differ in the details and
are the same idea.

## What that costs you

Everything downstream that wanted words gets none.

You cannot search the subtitles. You cannot re-time or restyle them without redrawing them. You
cannot feed them to a translation system, an index, a search engine or an accessibility tool. You
cannot put them on a web player that expects a text track. A media library that wants to say "this
film contains this line of dialogue" has nothing to look at.

Getting words back means something has to **look at the pictures and decide what letters they are**.
That is the whole job, and it is what SubTrackt does.

## What it actually does

Given a disc rip, SubTrackt:

1. Finds the subtitle track inside the file and pulls out its packets.
2. Turns those packets back into the pictures the player would have shown.
3. Decides which pixels are ink and which are background.
4. Cuts each picture into individual character shapes, separating the letters and reattaching
   accents to the letters they belong to.
5. Compares each shape against a set of shapes it already knows, and takes the closest one if
   anything is close enough.
6. Puts the characters back in order, works out where the word spaces go, attaches the times and
   writes the result out.

Everything else is arranged around step 5, which is what the next two pages are about:
[why it is a comparison rather than a guess](/guide/why-not-ocr), and [what the set of known shapes
is and where you get one](/guide/reference-sets).

## What comes out

A subtitle file, in **SubRip** (`.srt`) or **WebVTT** (`.vtt`), the two formats everything reads.
Lines the disc drew in a leaning typeface come out marked as italic.

And, if you ask for it, **a count of what could not be read**. That count is the reason to use this
tool over another one, and the next page is about why.

## What it is not for

**Tracks that are already text.** Some containers carry a text subtitle track alongside the video:
SubRip, ASS/SSA, WebVTT and the rest. There is nothing to recognise in those, because the words are
already there. SubTrackt will not list them and will not touch them. If your track is text you want
a tool that copies it out, not this one.

**Burned-in subtitles.** If the words are part of the video image rather than a separate track,
there is no subtitle track to read and this tool has nothing to work with.

## The three commands

In the order you would run them:

```console
$ subtrackt gen-reference /usr/share/fonts ./sets      # once: learn what letters look like
$ subtrackt fit movie.mkv --references ./sets -o movie.subtref   # per title: which typeface?
$ subtrackt extract movie.mkv --reference movie.subtref -o movie.en.srt   # per title: read it
```

The first is a one-off that builds a library of candidates from fonts you already have. The other
two are what you run for each film. The rest of this section covers those three commands, one page
each, then how it measures up against other tools, then what it cannot do.

There is also `subtrackt list`, which tells you what subtitle tracks a file contains. Start there
when you meet a new file.

If you would rather run the thing than read about it, [Quick start](/usage/quick-start) is the same
four commands with the installation in front of them, and one page per command after.
