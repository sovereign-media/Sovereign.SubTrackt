#!/usr/bin/env python3
"""Which languages does the library actually carry, and on how many discs?

#189 asks what `charset()` has to grow to, and that question has no answer until something says
which orthographies are on the shelf. Every figure this project has published is English; the
reason is not that the library is English, and this is what says so with a number.

It reads the container headers only -- `subtrackt list` stops at the stream table -- so a pass over
the whole library is minutes rather than hours, and no bitmap is ever decoded.

    $ scripts/language/survey.py --out library-languages.json
    $ scripts/language/survey.py --out out.json --limit 50      # a sample, for a smoke run

Two things it deliberately does *not* do:

- **It does not guess at an untagged stream.** Stream 0 of Gone Girl is English and carries no tag,
  because the muxer left the default untagged (#180 predicted exactly that). Counting untagged as
  English would be inventing data to avoid an unknown, so untagged is its own row and is reported
  as such.
- **It does not separate a commentary track from a feature track.** Gone Girl carries nine, all
  tagged with the language they are spoken in. They are subtitle streams in that language and the
  charset question does not care which of them a caller extracts.
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

# `idx  codec  lang  WxH  title`, where lang is `--` when the container carries no tag and the
# title is optional. Anchored on the resolution because that is the one field whose shape cannot
# collide with a title.
ROW = re.compile(r"^\s*(\d+)\s+(\S+)\s+(\S+)\s+(\d+)x(\d+)\s*(.*?)\s*$")

DEFAULT_INVENTORY = Path("image-based-subs-report.csv")
DEFAULT_ROOT = "//nas/MEDIA"
INVENTORY_PREFIX = "/media/"


def binary() -> str:
    """The release binary, which is what the bench uses too."""
    for candidate in ("target/release/subtrackt.exe", "target/release/subtrackt"):
        if Path(candidate).exists():
            # normpath, because CreateProcess will not resolve a relative path spelled with
            # forward slashes and every other script here spells it the same way.
            return os.path.normpath(candidate)
    sys.exit("build the release binary first: cargo build --release -p subtrackt-cli")


def titles(inventory: Path, root: str) -> list[tuple[str, str]]:
    """(folder, media path) for every file the inventory says carries a bitmap track."""
    out: list[tuple[str, str]] = []
    with inventory.open(encoding="utf-8", newline="") as handle:
        for row in csv.DictReader(handle):
            path = row.get("path", "")
            if not path.startswith(INVENTORY_PREFIX):
                continue
            out.append((row["folder"], root.rstrip("/") + "/" + path[len(INVENTORY_PREFIX) :]))
    return out


def streams(binary_path: str, media: str) -> list[dict[str, str]] | None:
    """The container's subtitle stream table, or None if it could not be read."""
    try:
        done = subprocess.run(
            [binary_path, "list", media],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=120,
        )
    except subprocess.TimeoutExpired:
        return None
    if done.returncode != 0:
        return None
    found = []
    for line in done.stdout.splitlines():
        match = ROW.match(line)
        if match:
            found.append(
                {
                    "index": match.group(1),
                    "codec": match.group(2),
                    "language": match.group(3),
                    "title": match.group(6),
                }
            )
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, default=DEFAULT_INVENTORY)
    parser.add_argument("--root", default=DEFAULT_ROOT, help="what /media/ is mounted at")
    parser.add_argument("--out", type=Path, required=True, help="where the JSON result goes")
    parser.add_argument("--limit", type=int, default=0, help="stop after N files (a smoke run)")
    args = parser.parse_args()

    exe = binary()
    work = titles(args.inventory, args.root)
    if args.limit:
        work = work[: args.limit]

    # A language is counted once per file *and* once per stream. The two differ by the commentary
    # tracks and they answer different questions: how much of the library could be read in this
    # language, versus how much material there is. Per *file* rather than per title because the
    # inventory holds television episodes too, and two shows both have a "Season 4".
    by_file: dict[str, set[str]] = defaultdict(set)
    per_stream: dict[str, int] = defaultdict(int)
    unreadable: list[str] = []
    read = 0

    for at, (folder, media) in enumerate(work, start=1):
        found = streams(exe, media)
        if found is None:
            unreadable.append(f"{folder}: {media}")
        else:
            read += 1
            for stream in found:
                by_file[stream["language"]].add(media)
                per_stream[stream["language"]] += 1
        if at % 100 == 0 or at == len(work):
            print(f"  {at}/{len(work)} files, {len(unreadable)} unreadable", file=sys.stderr)

    rows = sorted(
        (
            {"language": lang, "files": len(paths), "streams": per_stream[lang]}
            for lang, paths in by_file.items()
        ),
        key=lambda row: (-row["files"], row["language"]),
    )
    result = {
        "files_attempted": len(work),
        "files_read": read,
        "files_unreadable": unreadable,
        "languages": rows,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")

    print(f"\n{read} of {len(work)} titles read, {len(rows)} distinct language tags\n")
    print(f"{'tag':>6}  {'titles':>7}  {'streams':>8}")
    for row in rows:
        print(f"{row['language']:>6}  {row['titles']:>7}  {row['streams']:>8}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
