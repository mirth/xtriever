#!/usr/bin/env python3
"""Does eight-bit quantisation of the *embedder's weights* cost retrieval quality?

The companion to `int8_vectors_study.py`, which quantised the stored vectors and left the model
alone. This one leaves the storage alone and quantises the model: every linear layer's weight
matrix to eight-bit integers, dequantised for the matrix multiply — weight-only quantisation,
which is what an inference engine on a phone can actually do without an accelerator.

It embeds the SciFact corpus and its queries twice, once with the pinned float weights and once
with the quantised ones, and compares the rankings the two produce against the dataset's own
relevance judgements.

    apps/python-wiki-demo/.venv/bin/python reference/int8_model_study.py [dataset]
"""

from __future__ import annotations

import json
import math
import sys
import time
from collections import defaultdict
from pathlib import Path

import numpy as np
import torch
from tokenizers import Tokenizer
from transformers import AutoModel

ROOT = Path(__file__).resolve().parent.parent
DATASET_NAME = sys.argv[1] if len(sys.argv) > 1 else "scifact"
DATASET = ROOT / f"reference/datasets/beir/{DATASET_NAME}"
MODEL = ROOT / "reference/models/all-MiniLM-L6-v2"
MAX_SEQ_LEN = 256
BATCH = 64
K = 10


def load_model() -> torch.nn.Module:
    torch.set_grad_enabled(False)
    return AutoModel.from_pretrained(str(MODEL), dtype=torch.float32).eval()


def quantise_weights(model: torch.nn.Module, per_channel: bool) -> dict[str, float]:
    """Eight-bit symmetric quantisation of every linear layer's weights, dequantised in place.

    Per output channel is the usual choice: one scale per row of the weight matrix. Per tensor
    is the cruder alternative, kept for comparison. Biases, embeddings and layer norms stay in
    float — they are small and quantising them is where accuracy usually goes.
    """
    errors, quantised, params = [], 0, 0
    for module in model.modules():
        if not isinstance(module, torch.nn.Linear):
            continue
        weight = module.weight.data
        if per_channel:
            scale = weight.abs().amax(dim=1, keepdim=True) / 127.0
        else:
            scale = torch.full((weight.shape[0], 1), float(weight.abs().max()) / 127.0)
        scale = scale.clamp(min=1e-12)
        codes = torch.round(weight / scale).clamp(-127, 127)
        restored = codes * scale
        errors.append(float((restored - weight).abs().max()))
        module.weight.data = restored
        quantised += 1
        params += weight.numel()
    return {"layers": quantised, "params": params, "max_weight_error": max(errors)}


def embed(model: torch.nn.Module, texts: list[str]) -> np.ndarray:
    tok = Tokenizer.from_file(str(MODEL / "tokenizer.json"))
    tok.enable_truncation(max_length=MAX_SEQ_LEN)
    tok.enable_padding(length=MAX_SEQ_LEN, pad_id=0, pad_token="[PAD]", direction="right")
    out = []
    for start in range(0, len(texts), BATCH):
        batch = [tok.encode(t) for t in texts[start : start + BATCH]]
        ids = torch.tensor([e.ids for e in batch], dtype=torch.long)
        mask = torch.tensor([e.attention_mask for e in batch], dtype=torch.long)
        hidden = model(input_ids=ids, attention_mask=mask, token_type_ids=torch.zeros_like(ids)).last_hidden_state
        m = mask.unsqueeze(-1).to(hidden.dtype)
        pooled = (hidden * m).sum(dim=1) / m.sum(dim=1)
        out.append(torch.nn.functional.normalize(pooled, p=2.0, dim=1).numpy())
    return np.concatenate(out).astype(np.float32)


def ndcg_at_k(ranking, relevant, k=K) -> float:
    dcg = sum(relevant.get(d, 0) / math.log2(i + 2) for i, d in enumerate(ranking[:k]))
    idcg = sum(g / math.log2(i + 2) for i, g in enumerate(sorted(relevant.values(), reverse=True)[:k]))
    return dcg / idcg if idcg else 0.0


def recall_at(ranking, relevant, k) -> float:
    return sum(1 for d in ranking[:k] if d in relevant) / len(relevant) if relevant else 0.0


def score(doc_vectors, query_vectors, corpus_ids, query_ids, qrels):
    scores = query_vectors @ doc_vectors.T
    order = np.argsort(-scores, axis=1)
    ndcg = np.mean([ndcg_at_k([corpus_ids[i] for i in order[r][:K]], qrels[q]) for r, q in enumerate(query_ids)])
    recall = np.mean([recall_at([corpus_ids[i] for i in order[r][:100]], qrels[q], 100) for r, q in enumerate(query_ids)])
    return ndcg, recall, order


def main() -> int:
    corpus = [json.loads(line) for line in (DATASET / "corpus.jsonl").read_text().splitlines()]
    corpus_ids = [d["_id"] for d in corpus]
    # The passage the engine embeds is title and text joined, as Feature 004's harness builds it.
    passages = [f"{d.get('title', '')} {d.get('text', '')}".strip() for d in corpus]
    qrels = defaultdict(dict)
    for line in (DATASET / "qrels/test.tsv").read_text().splitlines()[1:]:
        q, d, s = line.split("\t")
        if int(s) > 0:
            qrels[q][d] = int(s)
    queries = {json.loads(l)["_id"]: json.loads(l)["text"] for l in (DATASET / "queries.jsonl").read_text().splitlines()}
    query_ids = [q for q in queries if q in qrels]
    query_texts = [queries[q] for q in query_ids]
    print(f"{DATASET_NAME}: {len(passages):,} passages · {len(query_ids)} judged queries")

    started = time.time()
    model = load_model()
    base_docs, base_queries = embed(model, passages), embed(model, query_texts)
    base_ndcg, base_recall, base_order = score(base_docs, base_queries, corpus_ids, query_ids, qrels)
    print(f"float32 weights          nDCG@10 {base_ndcg:.4f}  Recall@100 {base_recall:.4f}   ({time.time() - started:.0f} s)")

    for label, per_channel in (("int8 per output channel", True), ("int8 per tensor", False)):
        started = time.time()
        model = load_model()
        stats = quantise_weights(model, per_channel)
        docs, qs = embed(model, passages), embed(model, query_texts)
        ndcg, recall, order = score(docs, qs, corpus_ids, query_ids, qrels)
        drift = float(np.mean(np.sum(docs * base_docs, axis=1)))
        kept = np.mean([len(set(order[r][:K]) & set(base_order[r][:K])) / K for r in range(len(query_ids))])
        print(
            f"{label:<25}nDCG@10 {ndcg:.4f}  Recall@100 {recall:.4f}   "
            f"Δ nDCG {ndcg - base_ndcg:+.4f}  top-10 kept {kept:.3f}  "
            f"cosine to float embedding {drift:.5f}  ({time.time() - started:.0f} s)"
        )
        print(
            f"{'':25}{stats['layers']} linear layers, {stats['params'] / 1e6:.1f} M weights quantised, "
            f"largest weight error {stats['max_weight_error']:.2e}"
        )

    weights_mb = sum(p.numel() for p in load_model().parameters()) * 4 / 1e6
    print(f"\nembedder weights: {weights_mb:.0f} MB in float32 · about {weights_mb / 4:.0f} MB at eight bits")
    return 0


if __name__ == "__main__":
    sys.exit(main())
