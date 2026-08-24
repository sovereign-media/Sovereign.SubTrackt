---
title: Reference sets
label: Reference sets
description: What a reference set is, how you build one from fonts you already have, and why the binary deliberately ships without any.
---

# Reference sets

The matcher works by comparison, so it needs something to compare *against*. That something is a
**reference set**, and building one is the first thing you do.

## What one is

A reference set is a small file — a few kilobytes — that answers one question: *what does each
character look like?*

Not "look like" as a picture. Look like as a **measurement**. For every character, the set stores
the same description the pipeline will later compute from a shape cut out of a subtitle image:
a fixed-size summary of where the ink falls, plus a few proportions that summary throws away — how
tall the character stands relative to its line, how far it drops below the baseline, how wide its
ink is for its height.

That last group matters more than it sounds. A capital `I` and a lower-case `l` can be the *same*
arrangement of ink at the same height. What separates them is that one is drawn heavier than the
other, and the set records that.

The important part is that both sides of the comparison are produced **the same way**. A reference
set is not a picture of a font; it is a font put through the identical process a subtitle image
goes through. That is why building one is a command in this tool rather than a script you write:
a set built through any other route produces distances that mean nothing.

## How you make one

From a font file you already have.

```console
$ subtrackt gen-reference Arial.ttf arial.subtref
```

That reads the font, draws every character, measures each one, and writes the set.

Point it at a **directory** instead and you get one set per font in it:

```console
$ subtrackt gen-reference /usr/share/fonts ./sets
```

That is the normal way to start. `./sets` becomes your **library of candidates** — every typeface
you could plausibly be asked to read. You are not choosing anything yet, and you do not need to
know what any particular disc used. You are just collecting. Each set costs a few kilobytes, so
collecting broadly is nearly free.

This is a one-off. You do it once, and add to it when you meet material nothing in the library
fits.

## Italic and bold are separate shapes

A film's italic passages are not the same letters leaning over. They are **different drawings** —
an italic `a` is often a different shape entirely, not a slanted upright one. A set built only from
the upright cut of a typeface does not really know them.

So if you have the other cuts of the font, put them in:

```console
$ subtrackt gen-reference Arial.ttf arial.subtref --italic Ariali.ttf --bold Arialbd.ttf
```

One set, several cuts, every character contributing an entry from each. This is worth more than it
looks, because a film's italic act — flashbacks, radio voices, foreign dialogue, song lyrics — is
often a small fraction of the runtime and a large fraction of the errors.

Where you do not have the italic cut, the pipeline compensates in a different way, by measuring how
far a line leans and taking that into account. It is a real recovery, and it switches itself off as
soon as a set actually carries italic entries. The full comparison is in
[The slant](/research/italic-slant).

## Why nothing is embedded in the binary

This is the design decision people push back on hardest, so it is worth stating carefully.

SubTrackt ships with **no reference set at all**. Out of the box, before you build one, every
character comes back unmatched and you get nothing. That is deliberate.

The alternative — shipping a set built from some reasonable typeface — sounds obviously better. It
is not, and the reason is the same argument as [the previous page](/guide/why-not-ocr).

A shipped set would have to be built from *some* font. Whatever font that is, most discs were not
authored in it. When a reference set is close to the material but not the same, the matcher does
not fail loudly — it finds a nearest entry for nearly everything, because nearly everything has
*something* vaguely similar in the set. The result is a file that looks complete and is quietly
wrong, in ways no counter in the output can detect.

That is precisely the failure mode this tool exists to avoid, and it would have been shipped as the
default.

Compare the two failures:

- **No set at all**: everything is unmatched, the run refuses, and you know immediately.
- **A near-miss set**: most things match, the report looks healthy, and the text is wrong.

The first is an inconvenience. The second is the thing that cannot be recovered from, because
nothing downstream will ever notice.

So the set is your input, not the tool's opinion. The cost is that this does not work the moment
you download it, and that cost is real — it is the first entry in
[what it cannot do](/guide/what-it-cannot-do).

The measurement behind the decision, including what a deliberately wrong set does to a real disc,
is in [Which set should ship](/research/reference-set). What studios actually author against, over a
survey of real titles, is in [Typeface survey](/research/library-survey).

## What you have when you are done

A directory of candidate sets, built once, from fonts on a machine you control.

The next question is which one of them this particular film was drawn in — and that is
[fitting a title](/guide/fitting-a-title).
