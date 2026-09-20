#!/usr/bin/env python3
"""Does eight-bit quantisation of the stored dense vectors cost retrieval quality?

A cheap study before any feature work (the pattern Feature 022 used for chunking). It reads
the dense stage's own committed vectors for SciFact — `target/xt-dense-cache/scifact/`, dense
format 2 — embeds the dataset's test queries with the reference recipe
(`reference/gen_004_fixtures.py`: mean-mask pooling, L2 normalisation, the pinned weights), and
compares three rankings against the exact float ranking and against the dataset's own
relevance judgements:

  * **exact**           — what the engine does today: float32 dot products;
  * **int8**            — vectors quantised to eight-bit integers, scored as integers;
  * **int8 + rescore**  — the eight-bit ranking's top N rescored with the exact float vectors.

What matters is not the score error but whether the *ranking* moves, and whether a rescoring
pass puts back whatever the quantisation disturbed.

    apps/python-wiki-demo/.venv/bin/python reference/int8_vectors_study.py [dataset]

The dataset defaults to `scifact`; any BEIR set whose dense cache exists works.
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
RESCORE_DEPTHS = (10, 20, 50, 100, 200)


def read_rows(dense_dir: Path) -> tuple[np.ndarray, np.ndarray]:
    """The committed rows: `id u32 · norm f32 · vector dim×f32` (dense format 2, ADR-0013)."""
    manifest = (dense_dir / "manifest.bin").read_bytes()
    assert manifest[:8] == b"XTDENSE2", manifest[:8]
    header_len = struct.unpack("<Q", manifest[8:16])[0]
    header = json.loads(manifest[16 : 16 + header_len])
    dim, rows = header["dim"], header["rows"]
    raw = np.fromfile(dense_dir / f"vectors.{header['generation']}.bin", dtype=np.uint8)
    row_bytes = 8 + 4 * dim
    assert raw.size == rows * row_bytes, (raw.size, rows * row_bytes)
    table = raw.reshape(rows, row_bytes)
    ids = table[:, :4].copy().view(np.uint32).reshape(rows)
    vectors = table[:, 8:].copy().view(np.float32).reshape(rows, dim)
    return ids, vectors


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


def quantise(vectors: np.ndarray, per_vector: bool) -> tuple[np.ndarray, np.ndarray]:
    """Symmetric eight-bit codes and the scale that turns them back into floats."""
    if per_vector:
        scale = np.abs(vectors).max(axis=1, keepdims=True) / 127.0
    else:
        scale = np.full((vectors.shape[0], 1), np.abs(vectors).max() / 127.0, dtype=np.float32)
    codes = np.rint(vectors / scale).clip(-127, 127).astype(np.int8)
    return codes, scale.astype(np.float32)


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
    if not (CACHE / "manifest.bin").exists():
        print(f"no dense vectors at {CACHE} — run the SciFact dense baseline first", file=sys.stderr)
        return 1

    ids, vectors = read_rows(CACHE)
    corpus_ids = [json.loads(line)["_id"] for line in (DATASET / "corpus.jsonl").read_text().splitlines()]
    assert len(corpus_ids) == vectors.shape[0], (len(corpus_ids), vectors.shape[0])
    # Internal ids are the corpus positions (Feature 004 data model), and the rows are ordered.
    assert (ids == np.arange(ids.size, dtype=np.uint32)).all(), "rows are not in corpus order"

    qrels: dict[str, dict[str, int]] = defaultdict(dict)
    for line in (DATASET / "qrels/test.tsv").read_text().splitlines()[1:]:
        query_id, doc_id, score = line.split("\t")
        if int(score) > 0:
            qrels[query_id][doc_id] = int(score)
    queries = {json.loads(line)["_id"]: json.loads(line)["text"] for line in (DATASET / "queries.jsonl").read_text().splitlines()}
    query_ids = [q for q in queries if q in qrels]
    print(f"{DATASET_NAME}: {vectors.shape[0]:,} documents · {len(query_ids)} judged queries · dim {vectors.shape[1]}")

    Q = embed_queries([queries[q] for q in query_ids])

    exact_scores = Q @ vectors.T
    exact_order = np.argsort(-exact_scores, axis=1)

    print(f"\n{'scheme':<26}{'nDCG@10':>10}{'Recall@100':>12}{'top-10 kept':>13}{'bytes/row':>11}")
    row_bytes_f32 = 8 + 4 * vectors.shape[1]
    exact_ndcg = np.mean([ndcg_at_k([corpus_ids[i] for i in exact_order[r][:K]], qrels[q]) for r, q in enumerate(query_ids)])
    exact_recall = np.mean([recall_at([corpus_ids[i] for i in exact_order[r][:100]], qrels[q], 100) for r, q in enumerate(query_ids)])
    print(f"{'exact float32':<26}{exact_ndcg:>10.4f}{exact_recall:>12.4f}{1.0:>13.3f}{row_bytes_f32:>11}")

    results = {}
    for label, per_vector in (("int8, one scale per vector", True), ("int8, one global scale", False)):
        codes, scale = quantise(vectors, per_vector)
        q_codes, q_scale = quantise(Q, True)
        approx = (q_codes.astype(np.int32) @ codes.astype(np.int32).T).astype(np.float32) * q_scale * scale.T
        order = np.argsort(-approx, axis=1)

        ndcg = np.mean([ndcg_at_k([corpus_ids[i] for i in order[r][:K]], qrels[q]) for r, q in enumerate(query_ids)])
        recall = np.mean([recall_at([corpus_ids[i] for i in order[r][:100]], qrels[q], 100) for r, q in enumerate(query_ids)])
        kept = np.mean([len(set(order[r][:K]) & set(exact_order[r][:K])) / K for r in range(len(query_ids))])
        extra = 4 if per_vector else 0  # the per-vector scale beside the codes
        print(f"{label:<26}{ndcg:>10.4f}{recall:>12.4f}{kept:>13.3f}{8 + vectors.shape[1] + extra:>11}")
        results[label] = (order, approx)

    print("\nwith an exact rescoring pass over the eight-bit shortlist (per-vector scale):")
    order, _ = results["int8, one scale per vector"]
    print(f"{'shortlist':<26}{'nDCG@10':>10}{'top-10 kept':>13}{'Δ nDCG vs exact':>18}")
    for depth in RESCORE_DEPTHS:
        rescored_ndcg, kept = [], []
        for r, q in enumerate(query_ids):
            shortlist = order[r][:depth]
            best = shortlist[np.argsort(-exact_scores[r, shortlist])][:K]
            rescored_ndcg.append(ndcg_at_k([corpus_ids[i] for i in best], qrels[q]))
            kept.append(len(set(best) & set(exact_order[r][:K])) / K)
        print(f"{f'top {depth} rescored':<26}{np.mean(rescored_ndcg):>10.4f}{np.mean(kept):>13.3f}{np.mean(rescored_ndcg) - exact_ndcg:>+18.4f}")

    print(f"\nmemory for the shipped Wikipedia artefact (427,947 rows, dim 384):")
    for label, per_row in (("float32 today", row_bytes_f32), ("int8 + per-vector scale", 8 + 384 + 4), ("int8 + global scale", 8 + 384)):
        print(f"  {label:<26}{427_947 * per_row / 1e6:>8.1f} MB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
