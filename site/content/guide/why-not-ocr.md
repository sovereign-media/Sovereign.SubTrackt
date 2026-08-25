---
title: Why not OCR
label: Why not OCR
description: The thesis. An OCR engine always answers; a glyph matcher can say it doesn't know, and that turns out to be worth more.
---

# Why not OCR

Reading pictures of words is a solved problem with a name: optical character recognition. Good
engines exist, they're free, and pointing one at a subtitle image works.

So why this?

## OCR always answers

Show an OCR engine a shape it's never seen — an odd typeface, a language it wasn't trained on, a
letter the disc drew badly, two letters touching — and it returns a letter anyway. It has to. It
finds the most plausible reading and attaches a confidence number.

The confidence number looks like the answer to this, and isn't. It's the engine's opinion of its
own work, and it's least reliable exactly where it's most wrong. An unfamiliar shape scores high
when it happens to resemble something common. A whole track read in the wrong typeface comes back
looking calm. You can't set a threshold on it, because it doesn't mean the same thing twice.

What you get is a file full of real-looking words, some of which nobody wrote, and nothing marking
which.

## Why that matters here

If someone reads the output before it ships, it barely does. They'll spot `Iater` and fix it.

This is built for the case where nobody reads it: thousands of titles, no human in the loop,
straight into a library. There a plausible wrong line is worse than no line, because everything
downstream treats it as real. It gets indexed, searched, translated and displayed as though someone
wrote it.

An unattended pipeline doesn't need a tool that's usually right. It needs one that says when it
wasn't.

## What this does instead

It's a glyph matcher rather than an OCR engine, which means it has a fixed list of shapes it knows.

Every shape cut out of a subtitle gets measured and compared against that list. Closest entry wins,
but only if it's close *enough*. If the nearest known shape is still too far away, the answer is no
match.

You give up three things:

- characters that aren't in the list, since there's no guessing at unfamiliar shapes
- typefaces too far from the one the list was built from
- always producing output

and get one back: a failure you can see. An unread character is a specific character at a specific
time in a specific line. It's counted, it's in the report, and you can make the run stop when there
are too many.

## Why that trade is worth making

A confidence score is a number to argue about. An unmatched glyph is an event, so whatever called
SubTrackt can do something about it:

- ship the text, the track read clean
- try a different reference set, this one's the wrong typeface
- fall back to burning the original pictures into the video, which is ugly but correct
- queue it for a person, since it's one of the few that need one

All four need the tool to have been honest about failing, and none of them is available to
something holding a file of confident sentences.

## The catch

Built from the material's own typeface, a set reads at least as well as a general OCR engine and
more evenly across titles. Built from a generic one that doesn't match, it reads clearly worse.
Refusing to guess only helps when the shapes you know are the right ones.

[How it compares](/guide/how-it-compares) has both numbers.
