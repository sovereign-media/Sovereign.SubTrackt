---
title: Fitting a title
label: Fitting a title
description: What fit decides for you, and the part everyone gets wrong, which is what it does not and cannot certify.
---

# Fitting a title

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

With `-o` it copies the winner out, ready to hand to `extract`. Without it, it prints the ranking
and the `extract` command line you would run.

## What it is doing

It reads the first few hundred subtitles, cuts them into character shapes the way a real extraction
would, and then measures, for each candidate set, how far those shapes sit from the nearest entry
in that set, averaged over all of them. A shape nothing in the set can account for is charged the
worst possible distance, so a set that simply lacks characters cannot win by ignoring them.

Lowest average wins.

**It samples rather than reading the whole film**, because a typeface does not change halfway
through. A few hundred cues spread across the opening is a fair sample of the drawing, and it turns
a job that would take a minute or more into one that takes about three seconds. For material that
genuinely changes style partway through, which is rare but does happen, `--limit` raises the
sample.

Files in the directory that are not reference sets are skipped with a warning rather than killing
the run, and sets built for an incompatible internal grid are reported as unusable. If the
candidate list comes out smaller than the directory looked, it says so rather than quietly
narrowing.

## What `fit` decides

Which of the sets you gave it is **closest to this material**. That is a useful answer, and where
the right typeface is among the candidates it generally finds it. Picking correctly is worth a
great deal, because the gap between the right set and a plausible wrong one is not subtle.

## What `fit` does not decide

**Whether the winner is any good.**

The score is *relative*. It ranks the candidates against each other. It says nothing about whether
the best of them is close enough to trust, and the tool does not pretend otherwise: the caveat is
printed under the table every time it runs.

If your directory holds ten typefaces and the disc was drawn in an eleventh, `fit` will still
produce a winner, still rank it first and still print a score. The score will not look obviously
wrong, because scores only look wrong next to better ones.

## Why this cannot be fixed

This part surprises people, so here is the reasoning rather than the conclusion on its own.

The obvious move is to find a statistic that grades the winner, some number saying this fit is good
or this fit is bad, and refuse anything below a line. Several have been tried. All of them failed,
and they failed structurally rather than for want of a better idea.

**Most candidate statistics are functions of the matcher's own answer**, which is circular. A
reference set that is systematically wrong is, by construction, one that finds close matches. That
is *why* it produces wrong characters. The thing making the reading wrong is the thing making the
evidence look reassuring. Distances, agreement between top candidates, how much was read: each of
them looks better on some confidently wrong reads than on some correct ones.

**A statistic that avoids the circle by not looking at the answer breaks somewhere else.**
Identifying the typeface directly from the shapes, without matching anything, does sidestep it, and
then runs into a harder fact: a character decoded off a subtitle plane has drifted further from its
own typeface than typefaces sit apart from each other.

**Judging the output text instead has a third version of the first problem.** Score the words that
came out against what real language looks like and you can only score the words the model can
score. The characters it cannot score are not a random sample. They are concentrated exactly where
the reading went wrong.

So the conclusion is not that nobody has found it yet. It is that a tool cannot grade its own
reading without something to compare it against, and if you had something to compare it against you
would not need the tool. The long version, with all six attempts and why each one failed, is in
[Telling a good fit from a bad one](../docs/fit-confidence.md).

## So: read a few cues

Here is the practical instruction, and it takes about a minute.

Run the extraction, open the output and read the first page. Not for accuracy, for sanity. You are
looking for the signature of a wrong typeface, which is not gibberish. It is words that are almost
right: `Iater`, `rnodern`, `hoIiday`. A wrong set produces text that reads fine at a glance and
falls apart when you look at it.

If it reads like English, or whatever it should be, you are fine. If it does not, the answer is
usually one of:

- **The right typeface is not in your candidate directory.** Add more fonts and refit.
- **The set is right but incomplete.** With no italic or bold cut, the film's italic passages read
  badly while its dialogue reads perfectly. Rebuild that set with `--italic` and `--bold`.
- **Nothing you have fits.** That is a real answer, and being able to give it is the point of the
  tool.

Once a title is fitted, the set is a property of that title and you keep it. Refitting is cheap,
but you only need it once.

Next: [reading a track](/guide/reading-a-track).
