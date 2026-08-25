---
title: subtrackt extract
label: extract
description: The command that produces the subtitle file — every flag, how to read the report, and what the accuracy gate does and does not promise.
---

# `subtrackt extract`

This is the command that produces the subtitle file.

```console
$ subtrackt extract movie.mkv --reference movie.subtref --format srt -o movie.en.srt --report
822 cues from 822 images (1644 packets); glyphs 20597 matched / 17 unmatched
  / 2237 ambiguous (99.9% read); fit 10.7; cache 100%; defused 87; corrections 3 (context)
```

Two things came out of that: a subtitle file on disk, and a line of counts on stderr. The counts are
the half that makes this tool different from an OCR run, so a good part of this page is about
reading them.

## Usage

```console
$ subtrackt extract [OPTIONS] <INPUT>
```

| Argument | Meaning |
| :--- | :--- |
| `<INPUT>` | The file to read. Same formats [`list`](/usage/list) reads. |

### The flags you will actually use

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `--reference <FILE>` | **none embedded** | The `.subtref` to match against. Without it every glyph comes back unmatched. |
| `-o`, `--output <FILE>` | stdout | Where to write the subtitle. |
| `-f`, `--format <FMT>` | `srt` | `srt` (SubRip) or `vtt` (WebVTT). |
| `-s`, `--stream <N>` | first picture track | Which subtitle stream to read. |
| `--report` | off | Print the extraction summary to stderr. |
| `--on-unmatched <POLICY>` | `threshold` | What to do about glyphs the matcher cannot identify. [See below](#the-accuracy-gate). |
| `--min-matched <RATIO>` | `0.90` | With `threshold`, the fraction that must match. A value outside `0.0..=1.0` is rejected at parse time rather than clamped later. |

### Shape and text handling

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `--include-outline` | off | Treat the outline drawn around subtitle text as part of the character rather than as background. |
| `--defuse` / `--no-defuse` | on | Retry a component the matcher cannot read as two characters that were touching. |
| `--borrow-track-scale` / `--no-borrow-track-scale` | borrow | Where a line whose glyphs are all one height takes its scale from. |
| `--post-correct` / `--no-post-correct` | on | Resolve ambiguous reads from the characters around them. |
| `--lone-words` / `--no-lone-words` | on | Also read a one-character word of `l` as `I`. Needs post-correction. |
| `--track-vocabulary` / `--no-track-vocabulary` | off | Also resolve a word-edge glyph from words the same track read clearly. Needs post-correction. |
| `--vocab-prefix` / `--no-vocab-prefix` | on | Let a candidate match the *start* of a clear word, so `look` is supported by a clear `looking`. |
| `--vocab-min-occurrences <N>` | `1` | How often a word must be read clearly before it counts as evidence. |
| `--vocab-min-len <N>` | `2` | Shortest word the vocabulary arm may correct, in characters. |
| `--assume-english` | off | Assert the track is English, for the rules that need to know. The container is asked first. |

### Output metadata

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `--provenance` / `--no-provenance` | a note where the format allows one | Record what produced this file, and what it read, as a comment near the top. |

Plus the [global options](/usage/global-options).

## Why every switch comes in a pair

`--defuse` and `--no-defuse` both exist even though only one of them changes anything today. So do
the other five pairs.

That is deliberate. Each of those defaults is a **measurement result** rather than a fixed property
of the tool — it is on because scoring it against real discs said it should be — and results move. A
wrapper script that has pinned the behaviour it wants should not have to notice when a measurement
changes the default out from under it. The rightmost flag on the command line wins, so a script can
append one to a line that already carries the other.

## Where the output goes

Without `-o` the subtitle goes to standard output, so it pipes:

```console
$ subtrackt extract movie.mkv --reference movie.subtref | grep -n "red zone"
```

Everything the tool says *about itself* — progress, warnings, the report, errors — goes to standard
error and never into the subtitle. A redirect is always clean, and not one escape byte reaches
stdout under any flag combination, because a coloured `.srt` is a corrupt `.srt`.

## Italic lines are tagged

A line the ink shows was set in a leaning face is written `<i>`, in both formats. That decision is a
measurement of the line's own slant rather than a reference lookup, so it works even with a set
carrying no italic cut, and it agrees with a release subtitle's own tags on the overwhelming
majority of cues without changing a character of the text.
[The slant](/research/italic-slant) has the figures and the two ways it is still wrong.

## Provenance

By default an extracted file says what made it, where the format has syntax to say it in:

```
NOTE
Extracted by subtrackt 1.0.0 on 2026-08-24
reference set: arial-ri
glyphs: 66370 matched, 68 unmatched, 7046 ambiguous (99.9% read)
mean match distance: 11.7
```

WebVTT gets this automatically. **SubRip does not**, because SubRip has no comment syntax at all — a
note there is text before the first cue, which most parsers skip and a strict one is entitled to
reject. `--provenance` forces it in anyway; `--no-provenance` refuses both.

Every line of it is a count or a measurement. **There is no character error rate in there, and there
cannot be**: that needs a reference transcript and an extraction has none, so a file claiming its own
accuracy would be exactly the confident wrong answer this design exists to avoid. What it records
instead is the reference set, which is the half of a bad read that is otherwise untraceable months
later.

## Reading the report

`--report` prints the tally. Every field is a count or a measurement; there is nothing in it that is
an opinion.

| Field | What it means |
| :--- | :--- |
| `822 cues from 822 images` | Subtitles out, pictures in. Equal is normal. |
| `20597 matched` | Shapes that found an entry in the reference set close enough to accept. |
| `17 unmatched` | Shapes where the nearest entry was still too far away. |
| `2237 ambiguous` | Shapes that matched, but with a runner-up too close to call comfortably. Written out using the best candidate. |
| `99.9% read` | **Coverage**: matched plus ambiguous, as a fraction of all shapes. |
| `fit 10.7` | Mean distance between the shapes that matched and the entries they matched. |
| `cache 100%` | Share of shapes answered from the session cache rather than by comparison. |
| `defused 87` | Shapes that failed, were retried as two characters that were touching, and succeeded. |
| `corrections 3 (context)` | Characters rewritten after matching, by the post-correction arms. |

Three of those deserve more than a row.

**`unmatched` is the number the whole design exists to produce.** It is not an error estimate. It is
a count of specific characters the tool declined to guess at, and a caller can act on it.

**`fit` is the field people skip, and it is the one to watch.** Coverage says how many shapes found
*an* answer; `fit` says how well they fitted it. A mean drifting upward toward the acceptance
threshold is the signature of a track being read confidently and wrongly — everything matching,
nothing matching well. If you track one number across a library, track this one.

**`cache` should be very high**, because the same letter recurs constantly. If it drops sharply,
something upstream has stopped normalising shapes consistently, and that is a bug rather than a
property of the disc.

With post-correction on, **every individual correction is listed underneath the summary**. That is by
design: three corrections is a claim nobody can check, and a named substitution in a named word is
one anybody can. A stage allowed to rewrite text has to leave a trace.

## The accuracy gate

You do not have to read the report yourself for the tool to act on it. `--on-unmatched` says what
happens when shapes cannot be read:

| Value | Behaviour |
| :--- | :--- |
| `threshold` | Fail the run if coverage falls below `--min-matched`. **The default**, at `0.90`. |
| `fail-track` | Fail on a single unread shape. |
| `drop` | Leave out any subtitle containing an unread shape. |
| `placeholder` | Write the subtitle with a replacement character in place of the shape. |

The default means a badly-read track **fails loudly** rather than producing a file, which is what
makes this safe to run unattended: the calling script gets a non-zero exit and an error on stderr,
not a plausible subtitle.

`fail-track` was the default until it was measured. Rejecting on a *single* unread glyph refused
essentially every track in the library, which is a gate that never opens. It remains the right
choice for a caller who genuinely cannot tolerate a guess and has a fallback ready.

One warning about the floor, because it is easy to misread: **it catches a track that could not be
read. It makes no claim about a track that was read well.** A track can clear 0.90 comfortably and
still be wrong, if the reference set is a near-miss. Coverage is a weak predictor of correctness at
any threshold, which is the same point [`fit`](/usage/fit) makes about its own score.
[Architecture](/research/architecture#the-accuracy-gate) has where the number came from and why
raising it would not buy much.

## Post-correction

`--post-correct` is on by default. It resolves pairs that a *drawn* character genuinely cannot
distinguish — zero against capital O, one against lower-case l against capital I — using the
characters around them, and it has been measured to improve lines and worsen none.
`--no-post-correct` gives you the raw reading.

It has arms, and they are separately switchable:

- **Context**, always on with post-correction. Digit-versus-letter and the `l`/`I` family, decided
  from what surrounds the character.
- **Lone words** (`--lone-words`, on). A one-character word read as `l` becomes `I`. Assumes
  English; it crosses an apostrophe only where the container says the track is English, or where you
  have said so with `--assume-english`.
- **Track vocabulary** (`--track-vocabulary`, off). Resolves a word-edge glyph from words the *same
  track* read clearly elsewhere, so an ambiguous reading is settled by a clear one from another cue.
  `--vocab-min-occurrences`, `--vocab-min-len` and `--vocab-prefix` tune what counts as evidence.

The sweep behind those defaults, and what each arm is worth, is in
[Post-correction](/research/post-correction).

## When a track reads badly

In rough order of how often each is the answer:

- **Coverage is low, most shapes unmatched.** The reference set is wrong for this material, or you
  forgot to pass one. Refit against a wider candidate directory. This failure is loud and easy — it
  is the good case.
- **Coverage is fine and the text is subtly wrong.** Almost always a near-miss typeface. Check the
  `fit` figure against titles you have read successfully, widen the candidate directory, refit.
- **The dialogue is fine and the flashbacks are garbage.** The set has no italic cut. Rebuild it
  with [`gen-reference`](/usage/gen-reference) and its `--italic` flag.
- **Word spaces missing on some all-caps lines.** A known limitation rather than a misconfiguration;
  see [What it cannot do](/guide/what-it-cannot-do).
- **Nothing in your library fits.** Then the honest answer is that this title has no text subtitle
  available by this route, and the caller should fall back. Getting that answer is a success.

[Reading a track](/guide/reading-a-track) walks the same list at more length.

Next: [`glyphs`](/usage/glyphs), for when you want the shapes rather than the text.
