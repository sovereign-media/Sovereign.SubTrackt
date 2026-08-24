#!/usr/bin/env python3
"""Turn a directory of cue PGMs into contact sheets a person can read.

#185. Every accuracy figure in this repository is scored against a release sidecar -- another
transcript of the same dialogue, frequently read off the same bitmaps by some other tool. So a
systematic error this pipeline introduces could be matched by the same systematic error in the
comparison and score as agreement. Closing that needs ground truth somebody checked against the
images.

`xtask cue-images` writes the images; this stacks them, labels each with its cue index, and writes
one PNG per batch, so a reader transcribes a screenful at a time instead of opening a thousand
files.

    $ cargo run -p xtask -- cue-images bench-cache/wanda.sup truth-cache --count 300
    $ scripts/truth/sheets.py truth-cache --out truth-sheets --per-sheet 10

The transcription goes into an SRT beside the sheets, and `scripts/truth/check.py` scores an
extraction against it.
"""

import argparse
import os
import sys

try:
    from PIL import Image, ImageDraw
except ImportError:  # pragma: no cover - the message is the whole handling
    sys.exit("this needs Pillow: python -m pip install pillow")

# Room for the cue number down the left of every strip.
GUTTER = 120
# Space between cues, so two stacked cues never read as one two-line cue.
GAP = 14
# What a cue image is padded to, so short cues do not make a ragged sheet.
MIN_HEIGHT = 40


def load(path):
    """One PGM, as a Pillow image."""
    return Image.open(path).convert("L")


def sheet(entries, cache, scale):
    """Stack `entries` into one labelled image."""
    images = [(cue, load(os.path.join(cache, name))) for cue, name in entries]
    if scale != 1:
        images = [
            (cue, im.resize((int(im.width * scale), int(im.height * scale)), Image.LANCZOS))
            for cue, im in images
        ]
    width = GUTTER + max(im.width for _, im in images)
    height = sum(max(im.height, MIN_HEIGHT) + GAP for _, im in images) + GAP

    out = Image.new("L", (width, height), 0)
    draw = ImageDraw.Draw(out)
    y = GAP
    for cue, im in images:
        out.paste(im, (GUTTER, y))
        draw.text((8, y + 4), f"{cue}", fill=255)
        # A rule under each cue, because a subtitle's own line break and the boundary between two
        # cues look identical once they are stacked, and confusing them is how a transcription
        # silently merges two cues into one.
        rule = y + max(im.height, MIN_HEIGHT) + GAP // 2
        draw.line([(0, rule), (width, rule)], fill=90)
        y = rule + GAP // 2
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("cache", help="directory written by `xtask cue-images`")
    ap.add_argument("--out", required=True, help="where the sheets go")
    ap.add_argument("--per-sheet", type=int, default=10)
    ap.add_argument("--scale", type=float, default=1.0, help="resize each cue image")
    ap.add_argument("--from", dest="first", type=int, default=0)
    ap.add_argument("--count", type=int, default=None)
    args = ap.parse_args()

    manifest = os.path.join(args.cache, "cues.tsv")
    rows = []
    with open(manifest, encoding="utf-8") as handle:
        next(handle)
        for line in handle:
            cue, _start, _end, name = line.rstrip("\n").split("\t")
            rows.append((int(cue), name))
    rows = [row for row in rows if row[0] >= args.first]
    if args.count is not None:
        rows = rows[: args.count]

    os.makedirs(args.out, exist_ok=True)
    written = 0
    for start in range(0, len(rows), args.per_sheet):
        batch = rows[start : start + args.per_sheet]
        path = os.path.join(args.out, f"sheet-{batch[0][0]:05}.png")
        sheet(batch, args.cache, args.scale).save(path)
        written += 1
    print(f"{written} sheets covering {len(rows)} cues -> {args.out}")


if __name__ == "__main__":
    main()
