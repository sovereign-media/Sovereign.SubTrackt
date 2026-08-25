---
title: Reading a track
label: Reading a track
description: Running extract, the three report fields worth watching, and what to do when a track comes back badly.
---

# Reading a track

```console
$ subtrackt extract movie.mkv --reference movie.subtref --format srt -o movie.en.srt --report
822 cues from 822 images (1644 packets); glyphs 20597 matched / 17 unmatched
  / 2237 ambiguous (99.9% read); fit 10.7; cache 100%; defused 87; corrections 3 (context)
```

A subtitle file, and a line of counts. Every flag is on [`extract`](/usage/extract); this page is
the counts and what to do when they're bad.

Drop `-o` and the subtitle goes to stdout, so it pipes. Progress, warnings, the report and errors
all go to stderr and never into the subtitle, so a redirect is clean without asking. Italic lines
come out tagged in both formats, decided by measuring the ink rather than looking anything up, so
it works even with a set carrying no italic cut.

## The three fields worth watching

[`extract`](/usage/extract#reading-the-report) explains all nine fields. These three are the ones
to watch.

**`unmatched`** is why the tool exists. Not an error estimate — a count of specific characters it
declined to guess at, which is something a caller can act on.

**`fit`** is the mean distance between the shapes that matched and the entries they matched. It's
the field people skip and the one to track across a library. Coverage tells you how many shapes
found *an* answer; `fit` tells you how well they fitted it. A mean drifting up toward the
acceptance threshold means everything is matching and nothing is matching well, which is what a
track being read confidently and wrongly looks like.

**`cache`** should be near 100%, because the same letter recurs constantly. A sharp drop means
something upstream stopped normalising shapes consistently, which is a bug rather than anything
about the disc.

## The gate

By default a badly-read track fails instead of producing a file, so the calling script gets a
non-zero exit rather than a plausible subtitle. That's `--on-unmatched`, and
[`extract`](/usage/extract#the-accuracy-gate) has the four settings.

It catches a track that couldn't be *read*, and says nothing about one that was read *well*. A
near-miss set clears the default comfortably and still gets the text wrong, for the same reason
[nothing can grade a fit](/guide/fitting-a-title).

Nothing in the tool measures *well*, and nothing can: you decide that by reading a few cues. Every
accuracy figure quoted for SubTrackt, or for the tools it's
[compared against](/guide/how-it-compares), is that same judgement — made once by whoever
typed the transcript the scoring ran against.

## When a track reads badly

Roughly in order of how often each is the answer.

**Coverage is low, most shapes unmatched.** Wrong reference set for this material, or you forgot to
pass one. Refit against a wider candidate directory. Loud, easy, and the good case.

**Coverage is fine and the text is subtly wrong.** Almost always a near-miss typeface — close
enough to match everything, not close enough to be right. Compare the `fit` figure against titles
you've read successfully, widen the directory, refit.

**Dialogue is fine and the flashbacks are garbage.** No italic cut in the set. Rebuild it with
`--italic` and, while you're there, `--bold`. Common and very fixable.

**Word spaces missing on some all-caps lines.** A [known limitation](/guide/what-it-cannot-do)
rather than anything you've misconfigured.

**Nothing in your library fits.** Then this title has no text subtitle available by this route and
the caller should fall back: burn the original pictures into the video, or queue it for a person.
That's a success, not a failure. It's the answer the whole thing is built to be able to give.

---

What the pipeline does internally is [Architecture](../docs/architecture.md); what it costs to run
is [What a run costs](../docs/cost-baseline.md).
