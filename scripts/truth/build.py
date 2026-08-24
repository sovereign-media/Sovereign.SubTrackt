#!/usr/bin/env python3
"""Turn a hand transcription into an SRT, and trim an extraction to the same span.

#185. `xtask cue-images` writes one image per cue and a manifest of their time spans;
`scripts/truth/sheets.py` stacks them into contact sheets; a person reads the sheets and writes the
text into a transcription file. This joins the text back to the timings.

The transcription format is deliberately the least a person can type. A line of `#N` opens cue `N`,
counting from zero exactly as the manifest does, and every line after it up to the next `#` is that
cue's text:

    #0
    And on that point,
    members of the jury, I rest my case.
    #1
    (Camera clicks)

Cue indices are checked against the manifest and a gap is an error rather than a silent omission --
a ground truth missing a cue would score every later cue against the wrong one.

    $ scripts/truth/build.py --manifest truth-cache/cues.tsv --text truth/*.txt --out truth.srt
    $ scripts/truth/build.py --trim extracted.srt --to 300 --out extracted-300.srt
"""

import argparse
import glob
import sys


def timestamp(ms):
    """Milliseconds as `HH:MM:SS,mmm`."""
    ms = int(ms)
    hours, ms = divmod(ms, 3_600_000)
    minutes, ms = divmod(ms, 60_000)
    seconds, ms = divmod(ms, 1000)
    return f"{hours:02}:{minutes:02}:{seconds:02},{ms:03}"


def read_manifest(path):
    """`cue -> (start_ms, end_ms)`, from what `xtask cue-images` wrote."""
    spans = {}
    with open(path, encoding="utf-8") as handle:
        next(handle)
        for line in handle:
            cue, start, end, _name = line.rstrip("\n").split("\t")
            spans[int(cue)] = (int(start), int(end))
    return spans


def read_text(paths):
    """`cue -> text`, from the transcription files."""
    cues = {}
    current = None
    for path in paths:
        with open(path, encoding="utf-8") as handle:
            for line in handle:
                line = line.rstrip("\n")
                if line.startswith("#") and line[1:].strip().isdigit():
                    current = int(line[1:].strip())
                    if current in cues:
                        sys.exit(f"cue {current} appears twice")
                    cues[current] = []
                    continue
                if current is None:
                    sys.exit(f"{path}: text before the first #cue marker")
                cues[current].append(line)
    return {cue: "\n".join(lines).strip("\n") for cue, lines in cues.items()}


def read_srt(path):
    """`[(start_ms, end_ms, text)]`, tolerating a BOM."""
    blocks = open(path, encoding="utf-8-sig").read().replace("\r\n", "\n").strip().split("\n\n")
    out = []
    for block in blocks:
        lines = block.split("\n")
        if len(lines) < 3:
            continue
        start, end = lines[1].split(" --> ")
        out.append((start, end, "\n".join(lines[2:])))
    return out


def write_srt(path, cues):
    """`[(start, end, text)]` with timestamps already formatted."""
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        for index, (start, end, text) in enumerate(cues, start=1):
            handle.write(f"{index}\n{start} --> {end}\n{text}\n\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--manifest", help="cues.tsv from `xtask cue-images`")
    ap.add_argument("--text", nargs="*", default=[], help="transcription files, or globs")
    ap.add_argument("--trim", help="an existing SRT to cut down instead")
    ap.add_argument("--to", type=int, help="how many cues to keep when trimming")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    if args.trim:
        if args.to is None:
            sys.exit("--trim needs --to")
        write_srt(args.out, read_srt(args.trim)[: args.to])
        print(f"{args.to} cues -> {args.out}")
        return

    if not (args.manifest and args.text):
        sys.exit("need --manifest and --text, or --trim and --to")

    paths = []
    for pattern in args.text:
        paths.extend(sorted(glob.glob(pattern)) or [pattern])
    spans = read_manifest(args.manifest)
    text = read_text(paths)

    # A gap is an error rather than an omission: ground truth missing a cue would score every later
    # cue against the wrong one, which is the failure this whole exercise exists to rule out.
    wanted = range(min(text), max(text) + 1)
    missing = [cue for cue in wanted if cue not in text]
    if missing:
        sys.exit(f"transcription has no text for cues {missing[:10]}")
    unknown = [cue for cue in text if cue not in spans]
    if unknown:
        sys.exit(f"transcription names cues the manifest does not have: {unknown[:10]}")

    cues = [
        (timestamp(spans[cue][0]), timestamp(spans[cue][1]), text[cue])
        for cue in sorted(text)
    ]
    write_srt(args.out, cues)
    print(f"{len(cues)} cues ({min(text)}..{max(text)}) -> {args.out}")


if __name__ == "__main__":
    main()
