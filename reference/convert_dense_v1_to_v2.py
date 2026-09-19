#!/usr/bin/env python3
"""Convert a dense index directory from format version 1 to version 2 (Feature 024, ADR-0013)
without re-embedding: the vectors are the pinned embedder's output and do not change, only
their arrangement on disk does.

Version 1 (`dense/index.bin`, columnar):
    magic "XTDENSE1" · hdr_len u64 LE · JSON header {…,"count":n} · ids n×u32 · norms n×f32 ·
    vectors n×dim×f32
Version 2 (`dense/manifest.bin` + `dense/vectors.0.bin`, rows):
    manifest: magic "XTDENSE2" · hdr_len u64 LE · JSON header {…,"generation":0,"rows":n,
    "live":n,"ordered":true,"tombstones_len":8} · an empty roaring bitmap (8 bytes)
    rows: n × (id u32 · norm f32 · vector dim×f32), in the version-1 order (ascending ids)

A record of how the shipped Wikipedia artefact was regenerated, not a supported tool: the
engine refuses version 1 at open, and the proof that the conversion changed nothing is the
artefact's host goldens (`wikidemo measure`, 800/800 score bits).

    python3 reference/convert_dense_v1_to_v2.py <index dir>/dense [--dry-run]
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path

V1_MAGIC = b"XTDENSE1"
V2_MAGIC = b"XTDENSE2"
#: `RoaringBitmap::new().serialize_into(..)` (roaring 0.10, portable format): the
#: no-run-container cookie 12346 as u32 LE, then a container count of 0.
EMPTY_TOMBSTONES = bytes.fromhex("3a30000000000000")
CHUNK_ROWS = 4096


def read_v1_header(path: Path) -> tuple[dict, int]:
    with path.open("rb") as fh:
        magic = fh.read(8)
        if magic != V1_MAGIC:
            raise SystemExit(f"{path}: magic {magic!r} is not version 1")
        (hdr_len,) = struct.unpack("<Q", fh.read(8))
        header = json.loads(fh.read(hdr_len))
    if header.get("format_version") != 1:
        raise SystemExit(f"{path}: header version {header.get('format_version')} is not 1")
    return header, 16 + hdr_len


def convert(dense_dir: Path, dry_run: bool) -> dict:
    v1 = dense_dir / "index.bin"
    header, body_at = read_v1_header(v1)
    n, dim = int(header["count"]), int(header["dim"])
    ids_at = body_at
    norms_at = ids_at + 4 * n
    vectors_at = norms_at + 4 * n
    expected_len = vectors_at + 4 * n * dim
    actual_len = v1.stat().st_size
    if actual_len != expected_len:
        raise SystemExit(f"{v1}: {actual_len} bytes, expected {expected_len} for count {n} × dim {dim}")
    manifest_header = {
        "format_version": 2,
        "dim": dim,
        "metric": header["metric"],
        "fingerprint": header["fingerprint"],
        "generation": 0,
        "rows": n,
        "live": n,
        "ordered": True,
        "tombstones_len": len(EMPTY_TOMBSTONES),
    }
    encoded = json.dumps(manifest_header, separators=(",", ":")).encode("utf-8")
    manifest = V2_MAGIC + struct.pack("<Q", len(encoded)) + encoded + EMPTY_TOMBSTONES
    row_bytes = 8 + 4 * dim
    summary = {
        "rows": n,
        "dim": dim,
        "row_bytes": row_bytes,
        "v1_bytes": actual_len,
        "v2_rows_bytes": n * row_bytes,
        "manifest_bytes": len(manifest),
        "written": not dry_run,
    }
    if dry_run:
        return summary
    rows_path = dense_dir / "vectors.0.bin"
    tmp_rows = dense_dir / "vectors.0.bin.tmp"
    with v1.open("rb") as src, tmp_rows.open("wb") as dst:
        # The id and norm columns are 4 bytes a row — read once; the vector column is then
        # streamed in file order, so the source is read sequentially from front to back.
        src.seek(ids_at)
        ids = struct.unpack(f"<{n}I", src.read(4 * n))
        norms = src.read(4 * n)
        if any(b <= a for a, b in zip(ids, ids[1:])):
            raise SystemExit(f"{v1}: ids are not strictly ascending")
        for start in range(0, n, CHUNK_ROWS):
            count = min(CHUNK_ROWS, n - start)
            vectors = src.read(4 * dim * count)
            for i in range(count):
                r = start + i
                dst.write(struct.pack("<I", ids[r]))
                dst.write(norms[4 * r : 4 * r + 4])
                dst.write(vectors[4 * dim * i : 4 * dim * (i + 1)])
        dst.flush()
    tmp_rows.replace(rows_path)
    tmp_manifest = dense_dir / "manifest.bin.tmp"
    tmp_manifest.write_bytes(manifest)
    tmp_manifest.replace(dense_dir / "manifest.bin")
    v1.unlink()
    if rows_path.stat().st_size != summary["v2_rows_bytes"]:
        raise SystemExit(f"{rows_path}: {rows_path.stat().st_size} bytes, expected {summary['v2_rows_bytes']}")
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("dense_dir", type=Path, help="the index's dense/ directory holding index.bin")
    parser.add_argument("--dry-run", action="store_true", help="print the counts, write nothing")
    args = parser.parse_args()
    summary = convert(args.dense_dir, args.dry_run)
    print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    sys.exit(main())
