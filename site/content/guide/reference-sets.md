---
title: Reference sets
label: Reference sets
description: What a reference set is, how you build one from fonts you already have, and why the binary deliberately ships without any.
---

# Reference sets

The matcher works by comparison, so it needs something to compare against. That's a reference set,
and building one is the first thing you do.

## What one is

A few kilobytes answering one question: what does each character look like?

Not as a picture. As a measurement — for every character, the same description the pipeline will
later compute from a shape cut out of a subtitle image. A fixed-size summary of where the ink
falls, plus a few proportions that summary throws away: how tall the character stands against its
line, how far it drops below the baseline, how wide its ink is for its height.

Those proportions matter more than they sound. A capital `I` and a lower-case `l` can be the same
ink at the same height, and what separates them is that one's drawn heavier.

Both sides of the comparison have to come out of the same process, which is why building a set is a
command in this tool rather than a script you write. A set built any other way produces distances
that are numerically fine and mean nothing.

## Building them

Point [`gen-reference`](/usage/gen-reference) at a directory of fonts and get one set per font:

```console
$ subtrackt gen-reference /usr/share/fonts ./sets
```

`./sets` is now your library of candidates. You aren't choosing anything yet and you don't need to
know what any disc used — you're collecting, and at a few kilobytes each you can afford to collect
broadly. This is a one-off; come back to it when you meet material nothing in the library fits.

## Italic and bold are separate shapes

A film's italic passages aren't the upright letters leaning over. They're different drawings — an
italic `a` is usually a different shape — so a set built from only the upright cut doesn't really
know them.

If you have the other cuts, put them in:

```console
$ subtrackt gen-reference Arial.ttf arial.subtref --italic Ariali.ttf --bold Arialbd.ttf
```

Worth more than it sounds. Flashbacks, radio voices, foreign dialogue and song lyrics are often a
small slice of the runtime and a large slice of the errors.

**And a whole film can be set in one of the other cuts.** *Excision* (2012) is drawn in Arial Bold
throughout. Read with a set built from Arial regular and italic it scores 24.8% with 2,666 characters
it couldn't read — worse than every other film in [the comparison](/guide/how-it-compares) put
together, and the single worst result this tool has published. The disc isn't unusual and the
matcher isn't at fault: it was handed the wrong drawing of every letter. Add `--bold` and the same
film reads 11.4% with 52.

Without the italic cut the pipeline measures how far each line leans and compensates, then switches
that off as soon as a set carries italic entries. [The slant](../docs/italic-slant.md) compares the
four combinations.

## Why nothing ships embedded

This is the decision people push back on hardest. Out of the box, before you build a set, every
character comes back unmatched and you get nothing.

Shipping a set built from some reasonable typeface sounds obviously better, and it's the same trap
[OCR falls into](/guide/why-not-ocr). Whatever font it came from, most discs weren't
authored in it, and a set that's close but not right doesn't fail loudly. It finds a nearest entry
for nearly everything, because nearly everything has something vaguely similar in the set. You get
a file that looks complete and is quietly wrong, and no counter in the output can see it.

Compare the two failures:

- **No set**: everything unmatched, the run refuses, you know immediately.
- **A near-miss set**: most things match, the report looks healthy, the text is wrong.

The first is an inconvenience. There's no recovering from the second, because nothing downstream
will ever notice.

So the set is your input rather than the tool's opinion. The price is that it doesn't work the
moment you download it, which is real enough to lead
[what it cannot do](/guide/what-it-cannot-do).

[Which set should ship](../docs/reference-set.md) has the measurement, including what a
deliberately wrong set does to a real disc. [Typeface survey](../docs/library-survey.md) has what
studios actually author against.

---

You now have a directory of candidates. Working out which one this film was drawn in is
[fitting a title](/guide/fitting-a-title).
