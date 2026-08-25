---
title: What this is
label: What this is
description: What a bitmap subtitle track is, why it is a picture rather than text, and what the pipeline does with one.
---

# What this is

SubTrackt reads the subtitles off a Blu-ray or DVD and gives you a text file.

## The problem

A subtitle file you download is text. Open it in an editor and there are the words, with a start
and end time on each.

A subtitle track on a disc usually isn't. It's a sequence of pictures: for each subtitle, a small
image of the words already drawn, plus the times to show and hide it. Your player doesn't draw the
subtitle, it pastes a picture over the video.

There's no letter `a` anywhere in that file. There's a shape a human reads as one.

Discs do it that way so they look the same on every player ever built. Text would need each player
to hold the font and lay the line out itself, and they'd all do it slightly differently, or be
missing the font, or be missing the letters some language needs. Drawing it once at authoring time
sidesteps all of that.

Good decision for a disc. Awkward for everything downstream, which now has no words: no search, no
translation, no re-timing, no web player, no accessibility tooling. Getting the words back means
looking at the pictures and working out what letters they are.

You'll meet two formats. PGS on Blu-ray, VOBSUB on DVD. Different details, same idea.

## What it does with one

1. Finds the subtitle track and pulls out its packets.
2. Rebuilds the pictures the player would have shown.
3. Decides which pixels are ink and which are background.
4. Cuts each picture into character shapes, reattaching accents to their letters.
5. Compares each shape against a set of shapes it knows, and takes the closest one if anything is
   close enough.
6. Reassembles the characters, works out the word spaces, attaches the times, writes it out.

Step 5 is the interesting one and the subject of [the next page](/guide/why-not-ocr). The set of
known shapes is something [you build](/guide/reference-sets); nothing ships embedded.

Out the other end comes SubRip (`.srt`) or WebVTT (`.vtt`), italic lines tagged, and on request a
count of what couldn't be read.

## What it won't touch

Tracks that are already text — `S_TEXT/UTF8`, ASS/SSA, WebVTT and the rest. Nothing to recognise,
so you want a muxer instead. [`list`](/usage/list) won't even show them.

Subtitles burned into the video image, because there's no track to read.

MP4, which it refuses by name rather than guessing at the container.

## Getting going

[Quick start](/usage/quick-start) is install, build a set, read a track. The rest of this section is
why it works the way it does, and you don't need any of it to run the thing.
