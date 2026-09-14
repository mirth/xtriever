#!/usr/bin/env python3
"""Convert the pinned Simple English Wikipedia parquet to JSONL for the Rust build (Feature 008).

One object per line, keys ``id``, ``url``, ``title``, ``text`` in that order, parquet row order,
``ensure_ascii=False``, ``\\n`` line ends. The output is deterministic for a given parquet and
this script, and is pinned by hash in ``reference/datasets/wiki-manifest.json`` (research D2), so
the Rust side never needs a parquet reader.

    reference/.venv-008/bin/python reference/wiki_to_jsonl.py reference/datasets/wiki/train-00000-of-00001.parquet reference/datasets/wiki/simple.jsonl
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
from pathlib import Path

import pyarrow.parquet as pq

COLUMNS = ("id", "url", "title", "text")


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    src, dst = Path(argv[1]), Path(argv[2])
    tmp = dst.with_suffix(dst.suffix + ".tmp")
    table = pq.read_table(src, columns=list(COLUMNS))
    columns = [table.column(c).to_pylist() for c in COLUMNS]
    lines = 0
    digest = hashlib.sha256()
    with open(tmp, "w", encoding="utf-8", newline="\n") as out:
        for row in zip(*columns):
            line = json.dumps(dict(zip(COLUMNS, row)), ensure_ascii=False) + "\n"
            out.write(line)
            digest.update(line.encode("utf-8"))
            lines += 1
    if dst.exists():
        existing = hashlib.sha256(dst.read_bytes()).hexdigest()
        if existing != digest.hexdigest():
            tmp.unlink()
            print(f"wiki_to_jsonl: FAIL — {dst} exists with sha256={existing}, "
                  f"a fresh conversion gives {digest.hexdigest()}; refusing to overwrite", file=sys.stderr)
            return 1
        tmp.unlink()
    else:
        os.replace(tmp, dst)
    print(f"wiki_to_jsonl: {lines} lines, {dst.stat().st_size} bytes, sha256={digest.hexdigest()}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
