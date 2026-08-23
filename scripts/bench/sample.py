#!/usr/bin/env python3
"""Measure the size and the content of a sample of tracks, where a label cannot say.

The second half of #133. `scan.py` reads what a remuxer *declared*; this reads what a track
actually holds, which is the only way to answer two questions the labels cannot:

- **How far does the label undercount?** 35% of readable titles carry no track title at all, and
  the dominant spelling of SDH in this library is the typo `SHD`. A track is SDH in *content* when
  it carries speaker labels -- `ERIK:` -- and bracketed sound cues, and that is visible in the text
  rather than in the header.
- **Where are the extremes?** Every disc on the standing bench sits at 20k-66k glyphs, and #135 was
  a saturation bug at 65,535 that none of them could reach. Choosing a long track needs the
  distribution, and only a decode pass produces it.

Two modes, because they cost very different amounts:

    --mode size     `glyphs --summary`, ~30 s per title, gives cues/glyphs/shapes
    --mode content  full `extract`, minutes per title, gives the text to classify

Usage:
    scripts/bench/sample.py --tracks tracks.jsonl --mode size --count 100 --out sizes.jsonl
"""

import argparse
import json
import os
import random
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

NAS = "//nas/MEDIA/movies"

SDH_LABEL = re.compile(r"\bS\s*[DH]\s*[DH]\b|\bCC\b|caption|hard.of.hearing", re.I)
NON_ENGLISH = re.compile(
    r"french|german|spanish|italian|dutch|danish|swedish|norwegian|finnish|polish|czech|thai|"
    r"korean|japanese|portuguese|hungarian|russian|chinese|mandarin|latin|castilian|brazilian|"
    r"arabic|turkish|greek|hebrew|romanian|croatian|bulgarian|estonian|latvian|lithuanian|"
    r"slovak|slovenian|serbian|ukrainian|indonesian|vietnamese|malay|hindi|icelandic|catalan",
    re.I,
)

# What SDH looks like in the text rather than in the header. A speaker label is an upper-case run
# ending in a colon at the start of a line; a sound cue is bracketed. Both are conventions rather
# than standards, so this is reported as evidence and not as ground truth.
SPEAKER = re.compile(r"^\s*[-\s]*[A-Z][A-Z0-9 .'#-]{1,30}:", re.M)
SOUND_CUE = re.compile(r"^\s*[-\s]*[\[(][A-Za-z][^\])]{2,40}[\])]\s*$", re.M)

SUMMARY = re.compile(r"cues=(\d+)\s+glyphs=(\d+)\s+shapes=(\d+)")


def english_track(track):
    """Whether a track is plausibly the English one, by language tag then by title."""
    title = track.get("title") or ""
    if NON_ENGLISH.search(title):
        return False
    return track.get("lang") in ("eng", None)


def pick_track(record):
    """The track to measure: the English SDH one where labelled, else the first English one."""
    tracks = [t for t in record.get("tracks", []) if english_track(t)]
    if not tracks:
        tracks = record.get("tracks", [])
    if not tracks:
        return None
    labelled = [t for t in tracks if SDH_LABEL.search(t.get("title") or "")]
    return (labelled or tracks)[0]


def media_path(folder):
    """The largest `.mkv` in a title's folder, which is the feature rather than an extra."""
    directory = os.path.join(NAS, folder)
    try:
        files = [f for f in os.listdir(directory) if f.lower().endswith(".mkv")]
    except OSError:
        return None
    if not files:
        return None
    return max((os.path.join(directory, f) for f in files), key=lambda p: os.path.getsize(p))


def measure_size(binary, record, track):
    """Cue, glyph and shape counts, without matching anything."""
    path = media_path(record["folder"])
    if not path:
        return {"error": "no media"}
    out = subprocess.run(
        [binary, "glyphs", path, "--summary", "--stream", str(track["index"])],
        capture_output=True,
        text=True,
        timeout=900,
        errors="replace",
    )
    m = SUMMARY.search(out.stdout + out.stderr)
    if not m:
        return {"error": (out.stderr or "").strip()[:120] or "no summary"}
    return {"cues": int(m.group(1)), "glyphs": int(m.group(2)), "shapes": int(m.group(3))}


def measure_content(binary, reference, record, track):
    """Extract the track and look for what SDH puts in the text."""
    path = media_path(record["folder"])
    if not path:
        return {"error": "no media"}
    out = subprocess.run(
        [binary, "extract", path, "--stream", str(track["index"]), "--reference", reference,
         "--format", "srt"],
        capture_output=True,
        text=True,
        timeout=1800,
        errors="replace",
    )
    if out.returncode != 0:
        return {"error": (out.stderr or "").strip()[:120] or "failed"}
    text = "\n".join(
        line for line in out.stdout.splitlines()
        if line.strip() and "-->" not in line and not line.strip().isdigit()
    )
    return {
        "chars": len(text),
        "speaker_labels": len(SPEAKER.findall(text)),
        "sound_cues": len(SOUND_CUE.findall(text)),
        "colons": text.count(":"),
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--tracks", required=True, help="scan.py output")
    ap.add_argument("--out", required=True)
    ap.add_argument("--mode", choices=["size", "content"], required=True)
    ap.add_argument("--count", type=int, default=100)
    ap.add_argument("--workers", type=int, default=6)
    ap.add_argument("--binary", default="target/release/subtrackt.exe")
    ap.add_argument("--reference", help="a .subtref, required for --mode content")
    ap.add_argument("--only", choices=["labelled", "unlabelled", "any"], default="any",
                    help="restrict to titles whose English track is or is not SDH-labelled")
    ap.add_argument("--seed", type=int, default=133, help="so a re-run picks the same sample")
    args = ap.parse_args()

    if args.mode == "content" and not args.reference:
        ap.error("--mode content needs --reference")

    records = []
    for line in open(args.tracks, encoding="utf-8"):
        record = json.loads(line)
        track = pick_track(record) if "tracks" in record else None
        if not track:
            continue
        labelled = bool(SDH_LABEL.search(track.get("title") or ""))
        if args.only == "labelled" and not labelled:
            continue
        if args.only == "unlabelled" and labelled:
            continue
        records.append((record, track, labelled))

    random.Random(args.seed).shuffle(records)
    records = records[: args.count]

    def one(item):
        record, track, labelled = item
        base = {
            "folder": record["folder"],
            "stream": track["index"],
            "title": track.get("title"),
            "sdh_labelled": labelled,
        }
        try:
            if args.mode == "size":
                return {**base, **measure_size(args.binary, record, track)}
            return {**base, **measure_content(args.binary, args.reference, record, track)}
        except subprocess.TimeoutExpired:
            return {**base, "error": "timeout"}

    done = 0
    with open(args.out, "w", encoding="utf-8") as out:
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            for result in pool.map(one, records):
                out.write(json.dumps(result) + "\n")
                out.flush()
                done += 1
                print(f"  {done}/{len(records)}  {result.get('folder','')[:50]}",
                      file=sys.stderr, flush=True)
    print(f"{done} -> {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
