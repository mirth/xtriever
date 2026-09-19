"""The version-1 → version-2 converter on a synthetic version-1 dense file: every row's bytes,
the manifest header, the empty tombstone set, the original removed; a dry run writes nothing."""

import json
import struct

import helpers_024  # noqa: F401
import pytest

import convert_dense_v1_to_v2 as conv


def write_v1(dense_dir, ids, dim, fingerprint="fp"):
    header = {"format_version": 1, "dim": dim, "metric": "cosine", "fingerprint": fingerprint, "count": len(ids)}
    encoded = json.dumps(header, separators=(",", ":")).encode()
    norms = [float(i + 1) for i in range(len(ids))]
    vectors = [[float(i * dim + j) / 7.0 for j in range(dim)] for i in range(len(ids))]
    body = b"".join(struct.pack("<I", i) for i in ids)
    body += b"".join(struct.pack("<f", n) for n in norms)
    body += b"".join(struct.pack(f"<{dim}f", *v) for v in vectors)
    (dense_dir / "index.bin").write_bytes(b"XTDENSE1" + struct.pack("<Q", len(encoded)) + encoded + body)
    return norms, vectors


def test_rows_manifest_and_removal(tmp_path):
    dim, ids = 3, [2, 5, 9, 10]
    norms, vectors = write_v1(tmp_path, ids, dim)
    summary = conv.convert(tmp_path, dry_run=False)
    assert summary["rows"] == 4 and summary["row_bytes"] == 8 + 4 * dim
    assert not (tmp_path / "index.bin").exists()
    rows = (tmp_path / "vectors.0.bin").read_bytes()
    assert len(rows) == 4 * (8 + 4 * dim)
    for r, doc_id in enumerate(ids):
        at = r * (8 + 4 * dim)
        assert struct.unpack("<I", rows[at : at + 4])[0] == doc_id
        assert struct.unpack("<f", rows[at + 4 : at + 8])[0] == pytest.approx(norms[r])
        assert list(struct.unpack(f"<{dim}f", rows[at + 8 : at + 8 + 4 * dim])) == pytest.approx(vectors[r])
    manifest = (tmp_path / "manifest.bin").read_bytes()
    assert manifest[:8] == b"XTDENSE2"
    hdr_len = struct.unpack("<Q", manifest[8:16])[0]
    header = json.loads(manifest[16 : 16 + hdr_len])
    assert header == {
        "format_version": 2, "dim": dim, "metric": "cosine", "fingerprint": "fp",
        "generation": 0, "rows": 4, "live": 4, "ordered": True, "tombstones_len": 8,
    }
    assert manifest[16 + hdr_len :] == conv.EMPTY_TOMBSTONES
    assert not (tmp_path / "vectors.0.bin.tmp").exists() and not (tmp_path / "manifest.bin.tmp").exists()


def test_dry_run_writes_nothing_and_unsorted_ids_are_refused(tmp_path):
    write_v1(tmp_path, [1, 2, 3], 2)
    before = sorted(p.name for p in tmp_path.iterdir())
    assert conv.convert(tmp_path, dry_run=True)["rows"] == 3
    assert sorted(p.name for p in tmp_path.iterdir()) == before
    bad = tmp_path / "bad"
    bad.mkdir()
    write_v1(bad, [3, 2, 1], 2)
    with pytest.raises(SystemExit):
        conv.convert(bad, dry_run=False)
