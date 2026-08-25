---
title: Why not OCR
label: Why not OCR
description: The thesis. An OCR engine always answers; a glyph matcher is allowed to say it does not know, and that is worth more than accuracy.
---

# Why not OCR

Turning pictures of words into words is a solved problem with a name: **optical character
recognition**. There are good OCR engines, they are free, and pointing one at a subtitle image
works.

So why does this exist? Not because OCR is bad. Because OCR answers a question nobody asked, and
will not answer the one that matters.

## An OCR engine always answers

Show a general OCR engine a shape it has never seen. An unusual typeface, a character from a
language it was not trained on, a letter the disc drew badly, two letters that happen to touch. It
will still return a letter. It has to, because that is what it is for. It finds the most plausible
reading and hands it back with a confidence number attached.

The confidence number feels like it solves this. It does not.

A confidence number is the engine's opinion of its own work, and the cases where it is most wrong
are the cases where its opinion is worth least. A shape that is genuinely unfamiliar can score high
because it happens to resemble something common. A whole track read in the wrong typeface can come
back looking calm. You cannot set a threshold on it, because the number does not mean the same
thing twice.

What you get is a subtitle file full of real-looking words, some of which nobody ever wrote, and
**nothing in the output tells you which**.

## Why that is the whole problem

If a person is going to read the result before it ships, none of this matters much. They will spot
`Iater` for `later` and fix it.

This tool is built for the case where nobody is going to: thousands of titles, processed without a
human in the loop, results going straight into a library. There, a plausible wrong line is worse
than no line at all, because everything downstream treats it as a right one. It gets indexed,
searched, translated and displayed as though somebody had written it.

An automated pipeline does not need a tool that is usually right. It needs a tool that can tell it
when it was not.

## What this does instead

SubTrackt is not an OCR engine. It is a **glyph matcher**, and the difference is that it has a
fixed, finite list of shapes it knows.

Each shape cut out of a subtitle picture is measured and compared against every entry in that list.
The closest entry wins, but only if it is close *enough*. If the nearest known shape is still too
far away, the answer is **no match**.

That is the design, and it trades three things for one.

It gives up:

- Any character not in the list. There is no guessing at what a strange shape probably was.
- Any typeface too far from the one the list was built from.
- The comfort of always producing output.

It buys **a failure you can see**. An unread character is a specific character, at a specific time,
in a specific line. It is counted. It appears in the report. Ask for it and the run will stop
outright when there are too many.

## A fact you can act on

A confidence score is a number to argue about. An unmatched glyph is an event, and because it is an
event the thing calling SubTrackt can *do* something:

- Ship the text, because the track read cleanly.
- Try a different reference set, because this one is not the right typeface.
- Give up on text for this title and fall back to burning the original pictures into the video,
  which is ugly but correct.
- Put it in a queue for a person, because it is one of the few that need one.

All four are decisions, and all four need the tool to have been honest about failing. None of them
is available to a caller holding a file of confident sentences.

## The side effect: it is fast

There is a second consequence, and it was not the goal.

Comparing a shape against a few hundred known shapes is arithmetic, a handful of machine
instructions per comparison. Nothing loads a model, starts a session, looks for an accelerator or
reserves memory. And the same letter in the same film is the same shape every time, so after the
first few cues almost every character is answered from a cache without any comparison at all.

Reading a feature film's entire subtitle track takes seconds, from one small program with nothing
installed alongside it. That is not why the design was chosen, but it is why it is cheap to run
over a large library.

## The honest version

Two things have to be said plainly, because the argument above is easy to overstate.

**With a reference set built from the material's own typeface, this reads at least as well as a
general OCR engine, and more evenly across titles.** With a generic set that does not match the
material, it reads *worse*, and clearly worse. Refusing to guess is only a virtue when the shapes
you do know are the right ones.

**Nothing here can tell you which of those you are in without you looking.** The report says how
much was read, not how much was read *correctly*, and those are different questions. The page on
[fitting a title](/guide/fitting-a-title) is about that gap, and [what it cannot
do](/guide/what-it-cannot-do) is the full list.

So the claim is a narrow one: when this tool fails, it says so, and says where. Everything else was
traded for that.

All of the above was measured against five other tools reading the same bytes, and the next page,
[How it compares](/guide/how-it-compares), is what that found.
