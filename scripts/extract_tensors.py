#!/usr/bin/env python3
"""Copy named tensors out of a safetensors file into a new, smaller safetensors file — the same
bytes and dtype, nothing converted (Feature 026: the eight-bit re-ranker artefact carries the
published classifier but not the pooler it feeds on; the pooler is borrowed from the pinned
float file, exactly as the tokenizer is).

The output is deterministic — sorted header keys, no whitespace, tensors in name order — so a
manifest can pin its size and SHA-256 and `scripts/fetch-model.sh` can verify what it wrote.

    python3 scripts/extract_tensors.py SOURCE.safetensors OUT.safetensors NAME [NAME...]

Standard library only.
"""

from __future__ import annotations

import json
import struct
import sys
from pathlib import Path


def main(argv: list[str]) -> int:
    if len(argv) < 4:
        print(__doc__, file=sys.stderr)
        return 2
    source, out, names = Path(argv[1]), Path(argv[2]), sorted(argv[3:])
    with source.open("rb") as f:
        header_len = struct.unpack("<Q", f.read(8))[0]
        header = json.loads(f.read(header_len))
        base = 8 + header_len
        chunks: list[bytes] = []
        new_header: dict = {}
        at = 0
        for name in names:
            if name not in header:
                print(f"extract_tensors: {source} has no tensor {name}", file=sys.stderr)
                return 1
            meta = header[name]
            start, end = meta["data_offsets"]
            f.seek(base + start)
            data = f.read(end - start)
            chunks.append(data)
            new_header[name] = {"dtype": meta["dtype"], "shape": meta["shape"], "data_offsets": [at, at + len(data)]}
            at += len(data)
    encoded = json.dumps(new_header, sort_keys=True, separators=(",", ":")).encode("utf-8")
    with out.open("wb") as o:
        o.write(struct.pack("<Q", len(encoded)))
        o.write(encoded)
        for chunk in chunks:
            o.write(chunk)
    print(f"extract_tensors: wrote {out} ({8 + len(encoded) + at} bytes, {len(names)} tensors)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
