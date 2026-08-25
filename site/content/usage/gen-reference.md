---
title: subtrackt gen-reference
label: gen-reference
description: Turning font files into the reference sets the matcher compares against. The one-off you do before anything can be read.
---

# `subtrackt gen-reference`

Renders a font into a **reference set**, the file that answers "what does each character look
like?". Nothing is embedded in the binary, so this is how a set comes to exist. Until you have run
it once, nothing else in the tool can produce output.

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

The second is the normal way to start. `./sets` becomes your **library of candidates**, every
typeface you could plausibly be asked to read. You are not choosing anything yet and you do not need
to know what any particular disc used. Each set is a few kilobytes, so collecting broadly is nearly
free, and a wider library gives [`fit`](/usage/fit) a better chance of finding the right answer.

`--name`, `--italic` and `--bold` describe *one* typeface, so they cannot be combined with a
directory of fonts. Combining them is an error rather than a silent choice.

## Which font files it reads

`.ttf` and `.otf`.

`.ttc` collections are **skipped**. A collection holds several faces under one filename, and
picking the first one silently would file a set under a name that does not describe it, which shows
up months later as a bad extraction nobody can explain.

In a directory run, a font that cannot be read is named and skipped rather than killing the pass.
One unreadable file among forty is not a reason to produce nothing. If *every* font fails, the
command fails.

## Italic and bold are worth supplying

A film's italic passages, meaning flashbacks, radio voices, foreign dialogue and song lyrics, are
not the upright letters leaning over. They are **different drawings**. An italic `a` is usually a
different shape, and a set built from only the upright cut does not really know them.

```console
$ subtrackt gen-reference Arial.ttf arial.subtref --italic Ariali.ttf --bold Arialbd.ttf
```

One set, several cuts, every character contributing an entry from each. On a real Blu-ray this is
the difference between reading the italic act well and reading it badly, and the italic act is
often a small fraction of the runtime and a large fraction of the errors.

Without the italic cut the pipeline measures how far a line leans and samples along that slant,
two stages before the matcher, then switches that off once a set carries italic entries. The
four-way comparison is in [The slant](../docs/italic-slant.md).

## Why this is a subcommand and not a script

Because both sides of the comparison have to be produced the same way.

A reference set is not a picture of a font. It is a font put through the **identical**
normalisation `extract` applies to a character cut out of a decoded subtitle image: the same
binarisation, the same grid, the same proportions recorded alongside it. A set built through any
other transform produces distances that are numerically fine and mean nothing.

That is why it ships inside the binary rather than as a helper you write yourself.

## Naming sets so `fit` is readable

The name recorded inside the set is what appears in `fit`'s ranking table, not the filename. In a
directory run each set is named after its font's stem, which is usually what you want. For a set you
build by hand, especially one carrying extra cuts, say so:

```console
$ subtrackt gen-reference Arial.ttf ./sets/arial-ri.subtref \
      --italic Ariali.ttf --name arial-ri
```

A row reading `arial-ri` rather than `arial` tells you at a glance that the winner was the set with
the italic cut in it, and you will want that distinction when a title reads oddly.

## Why nothing is shipped

Because a shipped set would have to come from *some* font, most discs were not authored in it, and
a near-miss set fails invisibly rather than loudly.
[Reference sets](/guide/reference-sets#why-nothing-ships-embedded) makes the argument;
[Which set should ship](../docs/reference-set.md) has the measurement.

Next: [`fit`](/usage/fit), which decides which of your candidates reads a given title best.
