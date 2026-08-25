---
title: subtrackt fit
label: fit
description: Ranking your reference sets against one title, reading the table it prints, and the thing it deliberately does not tell you.
---

# `subtrackt fit`

You have a directory of candidate reference sets. This film was drawn in one typeface and you do
not know which. `fit` is the command that asks.

```console
$ subtrackt fit movie.mkv --references ./sets -o movie.subtref
400 cues, 10195 glyphs, 134 distinct shapes

  reference set               score       read
  arial-ri                     12.5      96.5%
  arial                        13.6      95.6%
  tahoma                       20.8      93.0%

  score is mean distance per glyph, charging unread glyphs the 51-cell ceiling.
  Lower fits better. Nothing here checks whether the winner is good enough --
  no measured statistic can. Read a few cues before trusting a track to it.
```

## Usage

```console
$ subtrackt fit --references <DIR> [OPTIONS] <INPUT>
```

| Argument | Meaning |
| :--- | :--- |
| `<INPUT>` | The file to fit against. Same formats [`list`](/usage/list) reads. |

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `-r`, `--references <DIR>` | **required** | Directory of candidates. Every `.subtref` in it is scored. |
| `-o`, `--output <FILE>` | — | Copy the winning set here, ready to hand to `extract --reference`. |
| `-s`, `--stream <N>` | first picture track | Which subtitle stream to read. |
| `-l`, `--limit <N>` | `400` | Cues to sample. |
| `--show <N>` | `5` | How many candidates to list. The winner is always listed. |
| `--include-outline` | off | Include the glyph outline in the foreground mask as well as the fill. |

Plus the [global options](/usage/global-options).

With `-o` it copies the winner out. Without it, it prints the ranking and the `extract` command line
you would run next, so you can read the table first and commit second.

## What it is doing

It reads the first few hundred subtitles, cuts them into character shapes exactly the way a real
extraction would, and then measures, for each candidate set in the directory, how far those shapes
sit from the nearest entry in that set, averaged over all of them.

A shape nothing in the set can account for is charged the worst possible distance, which stops a
set that is simply missing characters from winning by ignoring them. Lowest average wins.

The `read` column is the same coverage figure `extract --report` prints, the fraction of shapes
that found *an* answer. It is context rather than the ranking key.

## It samples the opening

A typeface does not change halfway through a film, and cues are spread evenly through it, so the
opening few hundred is a fair sample of the drawing. That turns a job which would take a minute or
more into one that takes a few seconds even against a multi-gigabyte file.

For material that genuinely changes style partway through, which is rare but does happen, raise the
sample:

```console
$ subtrackt fit movie.mkv --references ./sets --limit 2000
```

## What a shrunken candidate list means

Files in the directory that are not reference sets are **skipped with a warning** rather than
killing the run, and sets built for an incompatible internal grid are reported as unusable.

If the number of candidates comes out smaller than the directory looked, the run says so on stderr
rather than quietly narrowing the field. Read it, because a fit that ranked three sets out of a
directory of forty is not the answer you thought you asked for.

## What `fit` decides, and what it does not

**It decides which of the sets you gave it is closest to this material.** Where the right typeface
is among your candidates it generally finds it, and picking correctly is worth a great deal. The
gap between the right set and a plausible wrong one is not subtle.

**It does not decide whether the winner is any good.** The score is *relative*. It ranks the
candidates against each other and says nothing about whether the best of them is close enough to
trust. If your directory holds ten typefaces and the disc was drawn in an eleventh, `fit` still
produces a winner, still ranks it first, and still prints a score that does not look obviously
wrong, because scores only look wrong next to better ones.

That is not a missing feature. Six approaches to grading a fit without ground truth have been tried
and all six failed for structural rather than incidental reasons. The argument is on
[Fitting a title](/guide/fitting-a-title) and in full in
[Telling a good fit from a bad one](../docs/fit-confidence.md).

The practical consequence is one instruction, and it is printed under the table every time the
command runs: **read a few cues before trusting a track to a set.**

## A fitted set is a property of the title

Once a title fits, keep the `.subtref` beside the film. Refitting is cheap but you only need it
once, and having the exact set a file was read with is what makes a bad extraction diagnosable six
months later instead of just regrettable.

## When the answer looks wrong

If the extraction that follows reads badly, it is usually one of three things:

- **The right typeface is not in your candidate directory.** Add more fonts, rerun
  [`gen-reference`](/usage/gen-reference), refit.
- **The set is right but incomplete.** With no italic or bold cut, the dialogue reads perfectly and
  the italic passages read badly. Rebuild that one set with `--italic` and `--bold`.
- **Nothing you have fits.** That is a real answer, and being able to give it is the point of the
  tool.

Next: [`extract`](/usage/extract), which produces the subtitle file.
