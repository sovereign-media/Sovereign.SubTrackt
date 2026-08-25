---
title: subtrackt gen-reference
label: gen-reference
description: Turning font files into the reference sets the matcher compares against — the one-off you do before anything can be read.
---

# `subtrackt gen-reference`

Renders a font into a **reference set**: the file that answers "what does each character look
like?". Nothing is embedded in the binary, so this is how a set comes to exist, and until you have
run it once nothing else in the tool can produce output.

```console
$ subtrackt gen-reference /usr/share/fonts ./sets
$ subtrackt gen-reference Arial.ttf arial.subtref --italic Ariali.ttf --bold Arialbd.ttf
```

## Usage

```console
$ subtrackt gen-reference [OPTIONS] <FONT> <OUTPUT>
```

| Argument | Meaning |
| :--- | :--- |
| `<FONT>` | A font file, or a directory of them. |
| `<OUTPUT>` | A `.subtref` file for one font; a directory for a directory of fonts. |

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `--name <NAME>` | the font's filename stem | Name recorded inside the set. This is what `fit` prints when it ranks candidates, so set it to something you will recognise in that table. |
| `--italic <FONT>` | — | Italic cut of the same typeface, contributing its own entry for every character. |
| `--bold <FONT>` | — | Bold cut, likewise. |

Plus the [global options](/usage/global-options).

## The two modes

**One font, one set.** Give it a file and a `.subtref` path:

```console
$ subtrackt gen-reference Arial.ttf arial.subtref
```

**A directory of fonts, one set each.** Give it a directory and a directory:

```console
$ subtrackt gen-reference /usr/share/fonts ./sets
```

The second is the normal way to start, and `./sets` becomes your **library of candidates** — every
typeface you could plausibly be asked to read. You are not choosing anything yet and you do not need
to know what any particular disc used. Each set is a few kilobytes, so collecting broadly is nearly
free, and a wider library is what gives [`fit`](/usage/fit) a chance of finding the right answer.

`--name`, `--italic` and `--bold` describe *one* typeface, so they cannot be combined with a
directory of fonts. Combining them is an error rather than a silent choice.

## Which font files it reads

`.ttf` and `.otf`.

`.ttc` collections are **skipped**. A collection holds several faces under one filename, and picking
the first one silently would file a set under a name that does not describe it — which is exactly
the kind of quiet wrong answer that later shows up as a bad extraction nobody can explain.

In a directory run, a font that cannot be read is named and skipped rather than killing the pass —
one unreadable file among forty is not a reason to produce nothing. If *every* font fails, the
command fails.

## Italic and bold are worth supplying

A film's italic passages — flashbacks, radio voices, foreign dialogue, song lyrics — are not the
upright letters leaning over. They are **different drawings**. An italic `a` is frequently a
different shape entirely, and a set built only from the upright cut does not really know them.

```console
$ subtrackt gen-reference Arial.ttf arial.subtref --italic Ariali.ttf --bold Arialbd.ttf
```

One set, several cuts, every character contributing an entry from each. On a real Blu-ray this is
the difference between reading the italic act well and reading it badly, and the italic act is
often a small fraction of the runtime and a large fraction of the errors.

Where you do not have the italic cut, the pipeline compensates a different way — it measures how far
a line leans and samples along that slant, two stages before the matcher — and that recovery
switches itself off the moment a set actually carries italic entries. The four-way comparison is in
[The slant](../docs/italic-slant.md).

## Why this is a subcommand and not a script

Because both sides of the comparison have to be produced the same way.

A reference set is not a picture of a font. It is a font put through the **identical** normalisation
that `extract` applies to a character cut out of a decoded subtitle image: the same binarisation,
the same grid, the same proportions recorded alongside it. A set built through any other transform
produces distances that are numerically fine and mean nothing.

That constraint is the whole reason this ships inside the binary rather than as a helper you write
yourself.

## Naming sets so `fit` is readable

The name recorded inside the set is what appears in `fit`'s ranking table, not the filename. In a
directory run each set is named after its font's stem, which is usually what you want. For a set you
build by hand, especially one carrying extra cuts, say so:

```console
$ subtrackt gen-reference Arial.ttf ./sets/arial-ri.subtref \
      --italic Ariali.ttf --name arial-ri
```

A table row reading `arial-ri` rather than `arial` tells you at a glance that the winner was the set
with the italic cut in it, which is a distinction you will want when a title reads oddly.

## Why nothing is shipped

This is the design decision people push back on hardest, and it is not laziness: a shipped set would
have to be built from *some* font, most discs were not authored in it, and a near-miss set does not
fail loudly — it finds a plausible nearest entry for nearly everything and produces a file that
looks complete and is quietly wrong.

[Reference sets](/guide/reference-sets) makes the argument in full, and
[Which set should ship](../docs/reference-set.md) has the measurement behind it.

Next: [`fit`](/usage/fit), which decides which of your candidates reads a given title best.
