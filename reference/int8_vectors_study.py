#!/usr/bin/env python3
"""What does eight-bit storage of the dense vectors cost, measured on the engine's own rows?

Feature 026 stores each vector as one signed byte per dimension and a scale (dense format 3,
ADR-0015). This reads a BEIR dataset's evaluation cache — the float embeddings the embedder
produced (`vectors.f32.bin`, written by `beir run --config dense-baseline-v1`) and the
eight-bit rows the engine stored beside them — embeds the dataset's test queries with the
reference recipe, and reports:

  * that the stored rows **are** the scheme: the codes and scales recomputed from the floats
    in Python match the bytes on disk exactly;
  * the float ranking against the engine's ranking (integer dot products over the stored codes,
    cosine over the quantised norms, as `crates/xtriever-dense/src/index/search.rs` scores):
    candidate agreement at depth 100 (SC-001's measure, promised ≥ 99 % in
    `specs/026-eight-bit-precision/contracts/dense-format-v3.md`), the top-10 and top-1 kept,
    and nDCG@10 and Recall@100 for both;
  * bytes per row, for the record.

Before the feature, this script simulated the scheme on format-2 float rows and also measured a
global scale and an exact rescoring pass; those numbers are recorded in ADR-0015 and the spec,
and that version is in the history at the commit that introduced format 3.

    apps/python-wiki-demo/.venv/bin/python reference/int8_vectors_study.py [dataset]

The dataset defaults to `scifact`; any BEIR set whose cache was built by this engine works.
"""

from __future__ import annotations

import json
import math
import struct
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
DATASET_NAME = sys.argv[1] if len(sys.argv) > 1 else "scifact"
CACHE = ROOT / f"target/xt-dense-cache/{DATASET_NAME}"
DATASET = ROOT / f"reference/datasets/beir/{DATASET_NAME}"
MODEL = ROOT / "reference/models/all-MiniLM-L6-v2"
MAX_SEQ_LEN = 256
K = 10
DEPTH = 100
F32_MIN_POSITIVE = np.float32(2.0 ** -126)


def read_v3_rows(dense_dir: Path) -> tuple[dict, np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """The committed rows: `id u32 · norm f32 · scale f32 · codes dim×i8` (format 3)."""
    manifest = (dense_dir / "manifest.bin").read_bytes()
    assert manifest[:8] == b"XTDENSE3", manifest[:8]
    header_len = struct.unpack("<Q", manifest[8:16])[0]
    header = json.loads(manifest[16 : 16 + header_len])
    assert header["scheme"] == "i8-symmetric-per-vector", header["scheme"]
    dim, rows = header["dim"], header["rows"]
    raw = np.fromfile(dense_dir / f"vectors.{header['generation']}.bin", dtype=np.uint8)
    row_bytes = 12 + dim
    assert raw.size == rows * row_bytes, (raw.size, rows * row_bytes)
    table = raw.reshape(rows, row_bytes)
    ids = table[:, :4].copy().view(np.uint32).reshape(rows)
    norms = table[:, 4:8].copy().view(np.float32).reshape(rows)
    scales = table[:, 8:12].copy().view(np.float32).reshape(rows)
    codes = table[:, 12:].copy().view(np.int8)
    return header, ids, norms, scales, codes


def read_floats(dense_dir: Path, rows: int, dim: int) -> np.ndarray:
    """The embedder's own output, `rows × dim` f32 LE, written beside the index by the eval."""
    floats = np.fromfile(dense_dir / "vectors.f32.bin", dtype=np.float32)
    assert floats.size == rows * dim, (floats.size, rows * dim)
    return floats.reshape(rows, dim)


def quantise(vectors: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """The scheme in the crate's own arithmetic: `f32` scale (`max|x| / 127`, floored at the
    smallest normal), `f32` quotients rounded half away from zero, and the stored norm
    `sqrt(Σ code²) × scale`. Returns codes (int8), scales (f32, per row) and norms (f64)."""
    vectors = vectors.astype(np.float32)
    peak = np.abs(vectors).max(axis=1, keepdims=True)
    scale = np.maximum(peak / np.float32(127.0), F32_MIN_POSITIVE).astype(np.float32)
    scale = np.where(peak > 0, scale, np.float32(1.0)).astype(np.float32)
    q = (vectors / scale).astype(np.float32)
    codes = (np.floor(np.abs(q) + np.float32(0.5)) * np.sign(q)).clip(-127, 127).astype(np.int8)
    norms = np.sqrt((codes.astype(np.int64) ** 2).sum(axis=1)).astype(np.float64) * scale[:, 0].astype(np.float64)
    return codes, scale[:, 0], norms


def embed_queries(texts: list[str]) -> np.ndarray:
    """The reference recipe, the same one the 004 fixtures are minted with."""
    import torch
    from tokenizers import Tokenizer
    from transformers import AutoModel

    tok = Tokenizer.from_file(str(MODEL / "tokenizer.json"))
    tok.enable_truncation(max_length=MAX_SEQ_LEN)
    tok.enable_padding(length=MAX_SEQ_LEN, pad_id=0, pad_token="[PAD]", direction="right")
    torch.set_grad_enabled(False)
    model = AutoModel.from_pretrained(str(MODEL), dtype=torch.float32).eval()

    out = []
    for start in range(0, len(texts), 32):
        batch = [tok.encode(t) for t in texts[start : start + 32]]
        input_ids = torch.tensor([e.ids for e in batch], dtype=torch.long)
        mask = torch.tensor([e.attention_mask for e in batch], dtype=torch.long)
        hidden = model(
            input_ids=input_ids, attention_mask=mask, token_type_ids=torch.zeros_like(input_ids)
        ).last_hidden_state
        m = mask.unsqueeze(-1).to(hidden.dtype)
        pooled = (hidden * m).sum(dim=1) / m.sum(dim=1)
        out.append(torch.nn.functional.normalize(pooled, p=2.0, dim=1).numpy())
    return np.concatenate(out).astype(np.float32)


def ndcg_at_k(ranking: list[str], relevant: dict[str, int], k: int = K) -> float:
    gains = [relevant.get(doc, 0) for doc in ranking[:k]]
    dcg = sum(g / math.log2(i + 2) for i, g in enumerate(gains))
    ideal = sorted(relevant.values(), reverse=True)[:k]
    idcg = sum(g / math.log2(i + 2) for i, g in enumerate(ideal))
    return dcg / idcg if idcg else 0.0


def recall_at(ranking: list[str], relevant: dict[str, int], k: int) -> float:
    if not relevant:
        return 0.0
    found = sum(1 for doc in ranking[:k] if doc in relevant)
    return found / len(relevant)


def main() -> int:
    if not (CACHE / "vectors.f32.bin").exists():
        print(f"no evaluation cache with floats at {CACHE} — run `beir run --dataset {DATASET_NAME} "
              f"--config dense-baseline-v1` with this build first", file=sys.stderr)
        return 1

    header, ids, norms, scales, codes = read_v3_rows(CACHE)
    dim, rows = header["dim"], header["rows"]
    floats = read_floats(CACHE, rows, dim)
    corpus_ids = [json.loads(line)["_id"] for line in (DATASET / "corpus.jsonl").read_text().splitlines()]
    assert len(corpus_ids) == rows, (len(corpus_ids), rows)
    # Internal ids are the corpus positions (Feature 004 data model), and the rows are ordered.
    assert (ids == np.arange(ids.size, dtype=np.uint32)).all(), "rows are not in corpus order"

    # 1. The stored rows are the scheme, bit for bit.
    py_codes, py_scales, py_norms = quantise(floats)
    assert (py_codes == codes).all(), f"{(py_codes != codes).sum()} codes differ from the scheme"
    assert (py_scales == scales).all(), f"{(py_scales != scales).sum()} scales differ from the scheme"
    assert (py_norms.astype(np.float32) == norms).all(), "stored norms differ from the scheme"
    print(f"{DATASET_NAME}: {rows:,} documents · dim {dim} · stored rows match the scheme exactly "
          f"({rows * (12 + dim) / 1e6:.1f} MB against {rows * (8 + 4 * dim) / 1e6:.1f} MB as floats)")

    qrels: dict[str, dict[str, int]] = defaultdict(dict)
    for line in (DATASET / "qrels/test.tsv").read_text().splitlines()[1:]:
        query_id, doc_id, score = line.split("\t")
        if int(score) > 0:
            qrels[query_id][doc_id] = int(score)
    queries = {json.loads(line)["_id"]: json.loads(line)["text"] for line in (DATASET / "queries.jsonl").read_text().splitlines()}
    query_ids = [q for q in queries if q in qrels]
    Q = embed_queries([queries[q] for q in query_ids])
    print(f"{len(query_ids)} judged queries embedded with the reference recipe")

    # 2. The float ranking: cosine over the embedder's own output.
    exact = (Q @ floats.T) / (np.linalg.norm(Q, axis=1)[:, None] * np.linalg.norm(floats, axis=1)[None, :])
    exact_order = np.argsort(-exact, axis=1, kind="stable")

    # 3. The engine's ranking: what search.rs computes from the stored rows.
    q_codes, q_scales, q_norms = quantise(Q)
    dots = (q_codes.astype(np.int32) @ codes.astype(np.int32).T).astype(np.float64)
    engine = dots * q_scales[:, None].astype(np.float64) * scales[None, :].astype(np.float64)
    engine = (engine / (q_norms[:, None] * norms[None, :].astype(np.float64))).astype(np.float32)
    engine_order = np.argsort(-engine, axis=1, kind="stable")

    n = len(query_ids)
    agreement = np.mean([len(set(exact_order[r][:DEPTH]) & set(engine_order[r][:DEPTH])) / DEPTH for r in range(n)])
    top10 = np.mean([len(set(exact_order[r][:K]) & set(engine_order[r][:K])) / K for r in range(n)])
    top1 = np.mean([exact_order[r][0] == engine_order[r][0] for r in range(n)])
    print(f"\ncandidate agreement at depth {DEPTH}: {agreement:.4f}  (SC-001 promises ≥ 0.99)")
    print(f"top-10 kept: {top10:.4f}   top hit unchanged: {top1:.4f}")

    print(f"\n{'ranking':<22}{'nDCG@10':>10}{'Recall@100':>12}{'bytes/row':>11}")
    for label, order, per_row in (("float32 (embedder)", exact_order, 8 + 4 * dim), ("eight-bit (engine)", engine_order, 12 + dim)):
        ndcg = np.mean([ndcg_at_k([corpus_ids[i] for i in order[r][:K]], qrels[q]) for r, q in enumerate(query_ids)])
        recall = np.mean([recall_at([corpus_ids[i] for i in order[r][:100]], qrels[q], 100) for r, q in enumerate(query_ids)])
        print(f"{label:<22}{ndcg:>10.4f}{recall:>12.4f}{per_row:>11}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
