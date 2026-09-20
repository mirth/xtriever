#!/usr/bin/env python3
"""Does eight-bit quantisation of the *cross-encoder's weights* cost re-ranking quality?

The third of the eight-bit studies. `int8_vectors_study.py` quantised the stored vectors;
`int8_model_study.py` quantised the embedder. This one quantises the re-ranker — the stage that
dominates every latency measurement this project has taken.

It reproduces what the engine does: take each query's dense candidates, score the pairs with the
cross-encoder, re-rank, and measure nDCG@10 against the dataset's judgements. Once with the
pinned float weights and once per quantisation scheme.

    apps/python-wiki-demo/.venv/bin/python reference/int8_reranker_study.py [dataset] [depth]
"""

from __future__ import annotations

import json
import math
import struct
import sys
import time
from collections import defaultdict
from pathlib import Path

import numpy as np
import torch
from tokenizers import Tokenizer
from transformers import AutoModelForSequenceClassification

ROOT = Path(__file__).resolve().parent.parent
DATASET_NAME = sys.argv[1] if len(sys.argv) > 1 else "scifact"
DEPTH = int(sys.argv[2]) if len(sys.argv) > 2 else 20
DATASET = ROOT / f"reference/datasets/beir/{DATASET_NAME}"
CACHE = ROOT / f"target/xt-dense-cache/{DATASET_NAME}"
EMBEDDER = ROOT / "reference/models/all-MiniLM-L6-v2"
RERANKER = ROOT / "reference/models/ms-marco-MiniLM-L-6-v2"
MAX_PAIR_LEN = 512
K = 10


def read_rows(dense_dir: Path) -> np.ndarray:
    manifest = (dense_dir / "manifest.bin").read_bytes()
    header = json.loads(manifest[16 : 16 + struct.unpack("<Q", manifest[8:16])[0]])
    dim, rows = header["dim"], header["rows"]
    raw = np.fromfile(dense_dir / f"vectors.{header['generation']}.bin", dtype=np.uint8)
    table = raw.reshape(rows, 8 + 4 * dim)
    return table[:, 8:].copy().view(np.float32).reshape(rows, dim)


def embed_queries(texts: list[str]) -> np.ndarray:
    from transformers import AutoModel

    tok = Tokenizer.from_file(str(EMBEDDER / "tokenizer.json"))
    tok.enable_truncation(max_length=256)
    tok.enable_padding(length=256, pad_id=0, pad_token="[PAD]", direction="right")
    torch.set_grad_enabled(False)
    model = AutoModel.from_pretrained(str(EMBEDDER), dtype=torch.float32).eval()
    out = []
    for start in range(0, len(texts), 64):
        batch = [tok.encode(t) for t in texts[start : start + 64]]
        ids = torch.tensor([e.ids for e in batch], dtype=torch.long)
        mask = torch.tensor([e.attention_mask for e in batch], dtype=torch.long)
        hidden = model(input_ids=ids, attention_mask=mask, token_type_ids=torch.zeros_like(ids)).last_hidden_state
        m = mask.unsqueeze(-1).to(hidden.dtype)
        pooled = (hidden * m).sum(dim=1) / m.sum(dim=1)
        out.append(torch.nn.functional.normalize(pooled, p=2.0, dim=1).numpy())
    return np.concatenate(out).astype(np.float32)


def quantise_weights(model: torch.nn.Module, per_channel: bool) -> int:
    layers = 0
    for module in model.modules():
        if not isinstance(module, torch.nn.Linear):
            continue
        weight = module.weight.data
        scale = (weight.abs().amax(dim=1, keepdim=True) if per_channel
                 else torch.full((weight.shape[0], 1), float(weight.abs().max()))) / 127.0
        module.weight.data = torch.round(weight / scale.clamp(min=1e-12)).clamp(-127, 127) * scale
        layers += 1
    return layers


def cross_scores(model, tok, pairs: list[tuple[str, str]]) -> np.ndarray:
    out = []
    for start in range(0, len(pairs), 32):
        chunk = pairs[start : start + 32]
        encoded = [tok.encode(a, b) for a, b in chunk]
        width = max(len(e.ids) for e in encoded)
        ids = torch.tensor([e.ids + [0] * (width - len(e.ids)) for e in encoded], dtype=torch.long)
        mask = torch.tensor([e.attention_mask + [0] * (width - len(e.attention_mask)) for e in encoded], dtype=torch.long)
        types = torch.tensor([e.type_ids + [0] * (width - len(e.type_ids)) for e in encoded], dtype=torch.long)
        logits = model(input_ids=ids, attention_mask=mask, token_type_ids=types).logits
        out.append(logits[:, 0].numpy())
    return np.concatenate(out)


def ndcg_at_k(ranking, relevant, k=K) -> float:
    dcg = sum(relevant.get(d, 0) / math.log2(i + 2) for i, d in enumerate(ranking[:k]))
    idcg = sum(g / math.log2(i + 2) for i, g in enumerate(sorted(relevant.values(), reverse=True)[:k]))
    return dcg / idcg if idcg else 0.0


def main() -> int:
    corpus = [json.loads(line) for line in (DATASET / "corpus.jsonl").read_text().splitlines()]
    corpus_ids = [d["_id"] for d in corpus]
    passages = [f"{d.get('title', '')} {d.get('text', '')}".strip() for d in corpus]
    qrels = defaultdict(dict)
    for line in (DATASET / "qrels/test.tsv").read_text().splitlines()[1:]:
        q, d, s = line.split("\t")
        if int(s) > 0:
            qrels[q][d] = int(s)
    queries = {json.loads(l)["_id"]: json.loads(l)["text"] for l in (DATASET / "queries.jsonl").read_text().splitlines()}
    query_ids = [q for q in queries if q in qrels]

    vectors = read_rows(CACHE)
    candidates = np.argsort(-(embed_queries([queries[q] for q in query_ids]) @ vectors.T), axis=1)[:, :DEPTH]
    dense_ndcg = np.mean([ndcg_at_k([corpus_ids[i] for i in candidates[r][:K]], qrels[q]) for r, q in enumerate(query_ids)])
    print(f"{DATASET_NAME}: {len(query_ids)} queries · re-ranking the dense top {DEPTH}")
    print(f"{'dense only (no re-rank)':<28}nDCG@10 {dense_ndcg:.4f}")

    tok = Tokenizer.from_file(str(RERANKER / "tokenizer.json"))
    tok.enable_truncation(max_length=MAX_PAIR_LEN, strategy="longest_first")
    tok.no_padding()
    torch.set_grad_enabled(False)

    pairs = [(queries[q], passages[i]) for r, q in enumerate(query_ids) for i in candidates[r]]
    baseline_order = None
    for label, scheme in (("float32 weights", None), ("int8 per output channel", True), ("int8 per tensor", False)):
        model = AutoModelForSequenceClassification.from_pretrained(str(RERANKER), dtype=torch.float32).eval()
        layers = quantise_weights(model, scheme) if scheme is not None else 0
        started = time.time()
        scores = cross_scores(model, tok, pairs).reshape(len(query_ids), DEPTH)
        order = np.argsort(-scores, axis=1)
        ndcg = np.mean([
            ndcg_at_k([corpus_ids[candidates[r][i]] for i in order[r][:K]], qrels[q])
            for r, q in enumerate(query_ids)
        ])
        if baseline_order is None:
            baseline_order, baseline_ndcg, baseline_scores = order, ndcg, scores
            print(f"{label:<28}nDCG@10 {ndcg:.4f}   (+{ndcg - dense_ndcg:.4f} over dense alone, {time.time() - started:.0f} s)")
        else:
            kept = np.mean([len(set(order[r][:K]) & set(baseline_order[r][:K])) / K for r in range(len(query_ids))])
            drift = float(np.abs(scores - baseline_scores).mean())
            print(
                f"{label:<28}nDCG@10 {ndcg:.4f}   Δ {ndcg - baseline_ndcg:+.4f}  top-10 kept {kept:.3f}  "
                f"mean |Δ score| {drift:.4f}  ({layers} linear layers, {time.time() - started:.0f} s)"
            )
    weights = sum(p.numel() for p in AutoModelForSequenceClassification.from_pretrained(str(RERANKER), dtype=torch.float32).parameters())
    print(f"\nre-ranker weights: {weights * 4 / 1e6:.0f} MB in float32 · about {weights / 1e6:.0f} MB at eight bits")
    return 0


if __name__ == "__main__":
    sys.exit(main())
