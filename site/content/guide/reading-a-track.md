---
title: Reading a track
label: Reading a track
description: Running extract, reading the report line it prints, and what to do when a track comes back badly.
---

# Reading a track

This is the command that produces the subtitle file.

```console
$ subtrackt extract movie.mkv --reference movie.subtref --format srt -o movie.en.srt --report
822 cues from 822 images (1644 packets); glyphs 20597 matched / 17 unmatched
  / 2237 ambiguous (99.9% read); fit 10.7; cache 100%; defused 87; corrections 3 (context)
```

Two things came out of that: a subtitle file, and a line of counts. The counts are the half that
makes this tool different from any other, so most of this page is about reading them.

## First, what is in the file

If you have not met the file before, ask:

```console
$ subtrackt list movie.mkv
  0  hdmv_pgs_subtitle    eng   1920x1080  Full
  1  hdmv_pgs_subtitle    eng   1920x1080  Forced
```

Stream index, format, language, the size of the subtitle plane, and the track's name if the file
carries one. The index in the first column is what `--stream` takes; every other command defaults
to the first picture track it finds.

Only picture tracks are listed. A text subtitle track in the same file is skipped rather than
reported, because there is nothing here for it to gain.

`list` reads Blu-ray rips and raw `.sup` dumps, DVD `.idx`/`.sub` pairs — point it at the `.idx` —
and broadcast transport streams.

## Running the extraction

```console
$ subtrackt extract movie.mkv --reference movie.subtref --format srt -o movie.en.srt
```

`--format` is `srt` (SubRip) or `vtt` (WebVTT). Without `-o` the subtitle goes to standard output,
so it pipes. Everything the tool says *about itself* — progress, warnings, the report — goes to
standard error and never into the subtitle, so a redirect is always clean.

Lines the disc drew in a leaning typeface come out tagged as italic, in both formats. That decision
is made by measuring the ink rather than by looking anything up, so it works even with a reference
set that carries no italic cut.

By default the file also carries a short note saying what produced it and which reference set it
read with. That note is the difference between a bad extraction you can diagnose in six months and
one you cannot. WebVTT gets it automatically; SubRip does not, because SubRip has no comment syntax
and a strict parser is entitled to reject text before the first cue. `--provenance` forces it in
anyway.

## Reading the report

`--report` prints the tally. Every field is a count or a measurement — there is nothing in it that
is an opinion.

**`822 cues from 822 images`** — how many subtitles came out, and how many pictures went in. These
being equal is normal.

**`glyphs 20597 matched`** — shapes that found an entry in the reference set close enough to accept.

**`17 unmatched`** — shapes where the nearest entry was still too far away. **This is the number the
whole design exists to produce.** It is not an error estimate; it is a count of specific characters
the tool declined to guess at.

**`2237 ambiguous`** — shapes that did match, but where the runner-up was too close to call
comfortably. These are still written out, using the best candidate. A large ambiguous count is not
a failure, but a *growing* one across similar titles is worth noticing.

**`99.9% read`** — matched and ambiguous as a fraction of all shapes. This is **coverage**: how much
of the track found *an* answer.

**`fit 10.7`** — the average distance between the shapes that matched and the entries they matched.
This is the quality half, and it is the field people skip. Coverage says how many shapes found an
answer; `fit` says how well they fitted it. **A mean that drifts upward toward the acceptance
threshold is the signature of a track being read confidently and wrongly** — everything matching,
nothing matching well. If you track one number across a library, track this one.

**`cache 100%`** — how many shapes were answered from the session cache rather than by comparison.
This should be very high, because the same letter recurs constantly. If it drops sharply, something
upstream has stopped normalising shapes consistently, and that is a bug rather than a property of
the disc.

**`defused 87`** — shapes that failed to read, were retried as *two characters that were touching*,
and succeeded. Recovery work, and free, because it only runs where something already failed.

**`corrections 3 (context)`** — characters rewritten after matching, by rules that resolve
genuinely ambiguous pairs from their surroundings. Every individual correction is listed underneath
the summary, by design: "3 corrections" is a claim nobody can check, and `'I' -> 'l' in "jalapeño"`
is one anybody can.

## The gate

You do not have to read the report yourself for the tool to act on it.

`--on-unmatched` says what should happen when shapes cannot be read:

| Value | What it does |
| :--- | :--- |
| `threshold` | Fail the run if coverage falls below `--min-matched`. **The default**, at 0.90. |
| `fail-track` | Fail on a single unread shape. |
| `drop` | Leave out any subtitle containing an unread shape. |
| `placeholder` | Write the subtitle with a replacement character in place of the shape. |

The default means a badly-read track **fails loudly** rather than producing a file, which is what
makes this safe to run unattended: the calling script gets a non-zero exit and an error, not a
plausible subtitle.

One warning about that floor, because it is easy to misread: it catches a track that could not be
*read*. It makes no claim about a track that was read *well*. A track can clear it comfortably and
still be wrong, if the reference set is a near-miss. Coverage is a weak predictor of correctness at
any threshold, which is the same point as [the previous page](/guide/fitting-a-title).

## When a track reads badly

In rough order of how often it is the answer:

**Coverage is low — most shapes unmatched.** The reference set is wrong for this material, or you
forgot to pass one at all. Refit against a wider candidate directory. This failure is loud and easy;
it is the good case.

**Coverage is fine but the text is subtly wrong.** Almost always a near-miss typeface: the set is
close enough to match everything and not close enough to be right. Check the `fit` figure against
other titles you have read successfully. Widen the candidate directory and refit.

**The dialogue is fine and the flashbacks are garbage.** The set has no italic cut. Rebuild it with
`--italic` and, while you are there, `--bold`. This is a common and very fixable failure.

**Word spaces are missing on some lines.** Usually all-caps lines, often speaker labels in
subtitles for the deaf and hard of hearing. A known limitation rather than a misconfiguration; see
[what it cannot do](/guide/what-it-cannot-do).

**Nothing in your library fits.** Then the honest answer is that this title does not have a text
subtitle available from this route, and the caller should fall back — burn the original pictures
into the video, or queue the title for a person. **Getting that answer is a success, not a
failure.** It is the outcome the whole design is arranged to make possible.

## Two flags worth knowing

`--post-correct` is on by default and resolves pairs a drawn character genuinely cannot distinguish
— zero against capital O, one against lower-case l against capital I — using the characters around
them. It has been measured to improve lines and worsen none, which is why it is on. Turn it off with
`--no-post-correct` if you want the raw reading.

`--include-outline` treats the outline drawn around subtitle text as part of the character rather
than as background. The default does not, and the default is usually right, but material with
unusually heavy outlines can read better with it.

Everything else is in the README's command reference. What the pipeline does internally, stage by
stage, is in [Architecture](/research/architecture); what it costs to run is in
[What a run costs](/research/cost-baseline).
