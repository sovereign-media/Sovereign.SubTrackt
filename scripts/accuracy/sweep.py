#!/usr/bin/env python3
"""Run the pipeline over a sample of a media library and score each title against its sidecar SRT.

Writes one JSON result per title, so the sweep is resumable and a crash costs one title rather than
the run. See `docs/library-accuracy.md` for what the numbers mean and what they cannot claim.

Two decisions worth stating, because the measurement turns on both:

- **Every English sidecar in the folder is scored, not one.** Which release a rip's sidecar came
  from is unknown, and an SDH transcript scored against a dialogue-only track differs for reasons
  the matcher has no part in. The analysis takes the best-agreeing one and reports the spread.
- **The extracted SRT is kept.** It costs minutes of reads to produce and kilobytes to store, so
  discarding it would mean re-reading the whole library to answer any new question about scoring.

The inventory is a JSON list of objects with `folder`, `year`, `codecs`, `mkv` and `srts` (the SRT
filenames beside the file). It is deliberately not committed: it is an inventory of somebody's
media library.
"""
import argparse
import concurrent.futures as cf
import json
import os
import re
import subprocess
import sys
import time

# Sidecars that are not a full English transcript of the dialogue. A forced track covers only
# foreign-language lines and would score as though the pipeline had read almost nothing.
REJECT = re.compile(r"forced|commentary|sign|song|jump-scare|foreign|non-english", re.I)
ENGLISH = re.compile(r"\.(eng|en|english)([.-]|$)|\.english", re.I)

STOPWORDS = (" the ", " and ", " you ", " that ", " what ", " with ", " this ", " have ")


def parse_args():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--inventory", required=True, help="JSON list of titles to sweep")
    ap.add_argument("--out", default="results", help="directory for per-title result files")
    ap.add_argument("--work", default="work", help="directory to keep extracted SRTs in")
    ap.add_argument("--reference", required=True, help="the .subtref to match against")
    # Normalised, because Windows' `CreateProcess` rejects a relative path spelled with forward
    # slashes -- `target/release/subtrackt` is not found while `target\release\subtrackt` is. The
    # same fix `scripts/bench/run.py` needed, for the same reason.
    ap.add_argument("--subtrackt", default="target/release/subtrackt", type=os.path.normpath)
    ap.add_argument("--xtask", default="target/release/xtask", type=os.path.normpath)
    ap.add_argument("--workers", type=int, default=4, help="titles extracted in parallel")
    return ap.parse_args()


def looks_english(path):
    """Decide from the text, because plenty of sidecars carry no language tag at all.

    A filename is the weaker evidence: releases name subtitles `-meth.2.srt` as readily as
    `.eng.srt`, and rejecting the untagged ones silently drops titles the sweep could measure.
    """
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            head = fh.read(60_000).lower()
    except OSError:
        return False
    return sum(head.count(word) for word in STOPWORDS) >= 20


def english_sidecars(names, folder):
    tagged = [n for n in names if ENGLISH.search(n) and not REJECT.search(n)]
    if tagged:
        return sorted(tagged)[:3]
    untagged = [n for n in names if not REJECT.search(n)]
    return sorted(n for n in untagged if looks_english(os.path.join(folder, n)))[:3]


def pick_track(args, mkv):
    """The English bitmap track, preferring a full one over a forced or commentary track."""
    try:
        r = subprocess.run([args.subtrackt, "list", mkv], capture_output=True, text=True,
                           timeout=120, encoding="utf-8", errors="replace")
    except subprocess.TimeoutExpired:
        return None, "list timeout"
    if r.returncode != 0:
        return None, "list failed: " + (r.stderr or "").strip()[:200]

    rows = []
    for line in r.stdout.splitlines():
        m = re.match(r"\s*(\d+)\s+(\S+)\s+(\S+)\s+(\d+)x(\d+)\s*(.*)", line)
        if m:
            rows.append({"index": int(m.group(1)), "codec": m.group(2), "lang": m.group(3),
                         "w": int(m.group(4)), "h": int(m.group(5)), "title": m.group(6).strip()})
    if not rows:
        return None, "no bitmap streams"
    english = [r_ for r_ in rows if r_["lang"].lower() in ("eng", "en", "english")] or rows
    full = [r_ for r_ in english if not REJECT.search(r_["title"])] or english
    return full[0], None


REPORT = re.compile(
    r"glyphs (\d+) matched / (\d+) unmatched / (\d+) ambiguous \(([\d.]+)% read\); fit ([\d.]+)"
)


def run_title(args, title):
    key = re.sub(r"[^A-Za-z0-9]+", "_", title["folder"])[:120]
    dest = os.path.join(args.out, key + ".json")
    if os.path.exists(dest):
        return key, "skip"

    rec = {"folder": title["folder"], "year": title["year"],
           "codecs": title["codecs"], "mkv": title["mkv"]}
    try:
        rec["bytes"] = os.path.getsize(title["mkv"])
    except OSError:
        rec["bytes"] = None

    def finish(status):
        json.dump(rec, open(dest, "w", encoding="utf-8"))
        return key, status

    track, err = pick_track(args, title["mkv"])
    if track is None:
        rec["error"] = err
        return finish("no-track")
    rec["track"] = track

    srt = os.path.join(args.work, key + ".srt")
    # `placeholder` rather than the shipped default: a track that reads badly must still produce
    # text to be scored, or the sweep would silently drop exactly the titles worth measuring.
    cmd = [args.subtrackt, "extract", title["mkv"], "--reference", args.reference,
           "--format", "srt", "-o", srt, "--post-correct", "--stream", str(track["index"]),
           "--plain", "--on-unmatched", "placeholder", "--report"]
    started = time.time()
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=3600,
                           encoding="utf-8", errors="replace")
    except subprocess.TimeoutExpired:
        rec["error"] = "extract timeout"
        rec["extract_seconds"] = round(time.time() - started, 3)
        return finish("timeout")
    rec["extract_seconds"] = round(time.time() - started, 3)

    m = REPORT.search(r.stderr or "")
    if m:
        rec["glyphs"] = {"matched": int(m.group(1)), "unmatched": int(m.group(2)),
                         "ambiguous": int(m.group(3)), "read_pct": float(m.group(4)),
                         "fit": float(m.group(5))}
    if r.returncode != 0:
        rec["error"] = "extract failed: " + (r.stderr or "").strip()[-300:]
        return finish("extract-failed")

    folder = os.path.dirname(title["mkv"])
    rec["scores"] = {}
    for name in english_sidecars(title["srts"], folder):
        try:
            scored = subprocess.run(
                [args.xtask, "srt-score", srt, os.path.join(folder, name), "--json", "--align"],
                capture_output=True, text=True, timeout=900,
                encoding="utf-8", errors="replace")
        except subprocess.TimeoutExpired:
            continue
        if scored.returncode == 0:
            try:
                rec["scores"][name] = json.loads(scored.stdout)
            except json.JSONDecodeError:
                pass
    if not rec["scores"]:
        rec["error"] = "no sidecar scored"
    rec["srt"] = srt
    return finish("ok")


def main():
    args = parse_args()
    os.makedirs(args.out, exist_ok=True)
    os.makedirs(args.work, exist_ok=True)

    titles = [t for t in json.load(open(args.inventory, encoding="utf-8")) if t.get("srts")]
    print("%d titles to sweep" % len(titles), file=sys.stderr, flush=True)

    started = time.time()
    done = 0
    with cf.ThreadPoolExecutor(max_workers=args.workers) as pool:
        for key, status in pool.map(lambda t: run_title(args, t), titles):
            done += 1
            if done % 10 == 0:
                print("%d/%d  %.1f min  last=%s %s"
                      % (done, len(titles), (time.time() - started) / 60, status, key[:50]),
                      file=sys.stderr, flush=True)
    print("done in %.1f min" % ((time.time() - started) / 60), file=sys.stderr, flush=True)


main()
