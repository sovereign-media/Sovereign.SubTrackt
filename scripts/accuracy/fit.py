#!/usr/bin/env python3
"""Ask every title which typeface fits it, without consulting a subtitle file.

Usage: fit.py <inventory.json> [sets-dir] [out.json]

`subtrackt fit` scores a directory of reference sets against a sample of the title's own glyphs.
Nothing in it reads a release SRT, so the result is a statement about matchability alone -- what the
matcher can see for itself, before any question of whether the characters were the right ones.
"""
import concurrent.futures as cf
import json
import os
import re
import subprocess
import sys
import time

SUBTRACKT = os.environ.get("SUBTRACKT", "target/release/subtrackt")
SETS = sys.argv[2] if len(sys.argv) > 2 else "sets"
OUT = sys.argv[3] if len(sys.argv) > 3 else "fit.json"
INVENTORY = sys.argv[1] if len(sys.argv) > 1 else "sample.json"

HEAD = re.compile(r"(\d+) cues, (\d+) glyphs, (\d+) distinct shapes")
ROW = re.compile(r"^\s{2}(\S+)\s+([\d.]+)\s+([\d.]+)%\s*$")


def one(title):
    rec = {"folder": title["folder"], "year": title["year"], "codecs": title["codecs"]}
    started = time.time()
    try:
        r = subprocess.run(
            [SUBTRACKT, "fit", title["mkv"], "--references", SETS, "--show", "30", "--plain"],
            capture_output=True, text=True, timeout=1800, encoding="utf-8", errors="replace")
    except subprocess.TimeoutExpired:
        rec["error"] = "timeout"
        return rec
    rec["seconds"] = round(time.time() - started, 2)
    if r.returncode != 0:
        rec["error"] = (r.stderr or "").strip()[-200:]
        return rec

    # `fit` prints its table on stderr, alongside the rest of the CLI status output.
    text = (r.stdout or "") + (r.stderr or "")
    m = HEAD.search(text)
    if m:
        rec["cues"], rec["glyphs"], rec["shapes"] = (int(m.group(i)) for i in (1, 2, 3))
    ranked = []
    for line in text.splitlines():
        row = ROW.match(line)
        if row and row.group(1) != "reference":
            ranked.append({"set": row.group(1), "score": float(row.group(2)),
                           "read": float(row.group(3))})
    rec["ranked"] = ranked
    return rec


def main():
    titles = json.load(open(INVENTORY, encoding="utf-8"))
    out = []
    with cf.ThreadPoolExecutor(max_workers=4) as pool:
        for i, rec in enumerate(pool.map(one, titles)):
            out.append(rec)
            if i % 10 == 0:
                print("%d/%d" % (i, len(titles)), file=sys.stderr, flush=True)
    json.dump(out, open(OUT, "w", encoding="utf-8"), indent=1)
    print("wrote %s" % OUT, file=sys.stderr)


main()
