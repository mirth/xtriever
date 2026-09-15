# Report: Sparse Expansion Spike

**Feature**: `012-sparse-spike` | **Date**: 2026-09-15 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Pins

| model | revision | licence | parameters | activation | files |
|---|---|---|---|---|---|
| `opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill` | `babf71f3c48695e2e53a978208e8aba48335e3c0` | apache-2.0 | 66,985,530 | `log(1+log(1+relu))` | config, tokenizer, model.safetensors (~267 MB), idf.json (889,360 B) |
| `opensearch-project/opensearch-neural-sparse-encoding-doc-v2-distill` | `8921a26c78b8559d6604eb1f5c0b74c079bee38f` | apache-2.0 | 66,985,530 | `log(1+relu)` | same layout |

Manifests: `reference/models/manifest-sparse-doc-v{2,3}.json`; fetched and verified by
`scripts/fetch-model.sh --manifest …` (its file loop is generic; no script change).

## Red checkpoint (C1)

`reference/.venv-012/bin/pytest reference/tests_012 -q` → **1 passed** (the 003 scorer's
convention probe), **12 failed** (9 engine-run reproductions — no exports yet; 3 BM25/RRF/top-k
— functions missing), **3 errors** (the recipe checks — no encoder). The recipe oracle is the
v3 model card's own example: `What's the weather in ny now?` vs `Currently New York is rainy.`
→ similarity 11.1105 with six printed (query, document) weights.

## Verdict — GO, with `opensearch-neural-sparse-encoding-doc-v3-distill @ babf71f3c48695e2e53a978208e8aba48335e3c0`, dot-product scoring, weights quantised at ×100

By the rule fixed in the spec before the runs (FR-011): the v3 encoder's expansions, scored
as the model's dot product, beat the engine's BM25 nDCG@10 on **3 of 3** datasets (SciFact
0.708 vs 0.627, NFCorpus 0.345 vs 0.312, FiQA 0.357 vs 0.250), and reciprocal-rank fusion of
the engine's lexical and dense runs with the sparse run averages **0.484 vs the committed
hybrid's 0.468** (+1.6 points; SciFact 0.714 vs 0.690, NFCorpus 0.351 vs 0.345, FiQA 0.388 vs
0.369). The model card's numbers reproduce to the third decimal on all three sets, so the
recipe the engine must implement is exactly the one in research D1. The v2 encoder passes the
same rule with the same margins but emits vectors 1.6× as heavy (386 vs 239 non-zeros per
SciFact document, maxima 1,458 vs 335), so v3 is the pin. Scoring must be the **dot product**:
the "expansions as a plain BM25 field" shortcut wins on the two small corpora but collapses
on FiQA (0.199 — below BM25 itself), 8.7 mean points behind the dot product, far outside the
0.5-point allowance. Quantising document weights to integers at **×100** costs ≤ 0.03 nDCG
points on every set (×10 costs up to 0.95). For 013's spec verbatim: *inference-free encoder
`opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill` at revision
`babf71f3c48695e2e53a978208e8aba48335e3c0` (Apache-2.0, 66,985,530 parameters, DistilBERT,
`log(1+log(1+relu))` over max-pooled masked-LM logits, special tokens zeroed, window 512);
query = distinct token ids × `idf.json`, no model; document weights stored as `round(w × 100)`
term frequencies; score = Σ query_idf × stored_weight / 100; fused with the lexical and dense
candidate lists by RRF (k = 60).*

## The oracle agrees with the engine (SC-001)

All nine engine runs (lexical / dense / hybrid × 3 datasets), exported by the harness and
re-scored by the 003 reference: |Δ| ≤ 4.4e-16 on nDCG@10 and Recall@100, equal query counts.

## Metrics (nDCG@10 / Recall@100; every run scored by the 003 reference; models by key)

| variant | SciFact | NFCorpus | FiQA |
|---|---|---|---|
| engine BM25 (`lexical-baseline-v1`) | 0.6270 / 0.8876 | 0.3115 / 0.2478 | 0.2502 / 0.5518 |
| engine dense (`dense-baseline-v1`) | 0.6451 / 0.9250 | 0.3167 / 0.3115 | 0.3687 / 0.7061 |
| engine hybrid (`hybrid-baseline-v1`) | 0.6897 / 0.9417 | 0.3450 / 0.3207 | 0.3692 / 0.7071 |
| spike's own text BM25 (F-002) | 0.6867 / 0.9247 | 0.3228 / 0.2493 | 0.2473 / 0.5526 |
| **v3 `dot`** | **0.7080** / 0.9343 | **0.3454** / 0.2776 | **0.3571** / 0.6427 |
| v3 `dot-q100` | 0.7095 / 0.9377 | 0.3451 / 0.2779 | 0.3589 / 0.6418 |
| v3 `bm25x` (expansions as a BM25 field) | 0.6371 / 0.9153 | 0.3143 / 0.2574 | 0.1989 / 0.4899 |
| v3 `bm25x+text-b0.5` | 0.6990 / 0.9420 | 0.3396 / 0.2741 | 0.2899 / 0.5857 |
| v3 `rrf-lex+dot` | 0.6983 / 0.9370 | 0.3354 / 0.2798 | 0.3247 / 0.6405 |
| v3 `rrf-dense+dot` | 0.6997 / 0.9550 | 0.3366 / 0.3242 | **0.4033** / 0.7108 |
| **v3 `rrf-lex+dense+dot`** | **0.7139** / 0.9517 | **0.3508** / 0.3216 | 0.3881 / 0.7007 |
| v3 `rrf-lex+dense+bm25x` | 0.7199 / 0.9517 | 0.3558 / 0.3159 | 0.3697 / 0.6925 |
| v2 `dot` | 0.7148 / 0.9343 | 0.3437 / 0.2792 | 0.3553 / 0.6359 |
| v2 `rrf-lex+dense+dot` | 0.7183 / 0.9583 | 0.3539 / 0.3205 | 0.3833 / 0.7046 |

The full table — every variant × dataset × model, the `bm25x+text` boosts, the pairwise
fusions — is [`runs/summary.json`](./runs/summary.json). Model cards' claims: v3 0.708 / 0.345
/ 0.356, v2 0.715 / 0.343 / 0.357 — reproduced.

## Quantisation (nDCG@10 points lost vs the float dot product; negative = gained)

| model | SciFact ×10 / ×100 / ×1000 | NFCorpus | FiQA | smallest scale within 0.1 |
|---|---|---|---|---|
| v3 | +0.64 / −0.15 / 0.00 | +0.13 / +0.03 / −0.03 | +0.95 / −0.18 / −0.02 | **×100** on all three |
| v2 | +0.35 / +0.01 / 0.00 | +0.07 / −0.01 / 0.00 | +0.38 / −0.01 / −0.04 | ×100 (×10 on NFCorpus) |

## Costs (this host: Apple M1, MPS, PyTorch 2.14, 8 threads; batch 8; every shard timed, aggregated per shard — review round 1 #1)

| model | dataset | docs | docs/s | nnz/doc mean / p95 / max | nnz/query mean / p95 | truncated (> 512) |
|---|---|---|---|---|---|---|
| v3 | SciFact | 5,183 | 27.5 | 239 / 281 / 335 | 19.6 / 33 | 455 |
| v3 | NFCorpus | 3,633 | 27.4 | 217 / 264 / 333 | 4.9 / 11 | 330 |
| v3 | FiQA | 57,638 | 33.6 | 224 / 281 / 730 | 12.9 / 21 | 2,417 |
| v2 | SciFact | 5,183 | 27.3 | 386 / 625 / 1,458 | 19.6 / 33 | 455 |
| v2 | NFCorpus | 3,633 | 27.5 | 292 / 486 / 1,282 | 4.9 / 11 | 330 |
| v2 | FiQA | 57,638 | 30.7 | 245 / 432 / 1,567 | 12.9 / 21 | 2,417 |

Model on disk: 267 MB (f32 safetensors), 66,985,530 parameters, both. Query side at run time:
the tokenizer and `idf.json` (30,522 entries, 889 KB) — no model, confirmed by
`test_queries_never_call_the_model`. No query encoded to an empty vector on any set.

**Wikipedia projection** (v3; SciFact's 239 non-zeros per document as the analogue for
256-token passages): 427,947 × 239 ≈ **102 M postings**; at 1.5–2.5 B per tantivy posting with
a term frequency, **150–255 MB** on disk, memory-mapped like the rest of the index. Encoding
time: this host's GPU does 27–34 docs/s → 3.5–4.4 h for the corpus in Python; **a CPU encode
through candle would be far slower** — DistilBERT plus a 768 × 30,522 masked-LM head is
roughly 5× the FLOPs of the MiniLM embedder, whose 008 build took 11 h at 4 threads (F-004).

## Findings

### F-001 — The masked-LM logits are the memory: batch 32 held ~20 GB, batch 8 holds ~0.5 GB

Each batch materialises `batch × tokens × vocab` logits (32 × 512 × 30,522 × 4 B ≈ 2 GB)
before the max-pool, and MPS keeps them in unified memory; the first run reached 20 GB on a
32 GB machine. The default batch is now 8, the logits are freed and `torch.mps.empty_cache()`
is called per batch — and throughput went *up* (F-003). The engine's encoder (013) must
pool per batch the same way and keep batches small; the FLOPs are in the head, not the body.

### F-002 — The spike's one-field text BM25 beats the engine's BM25 by 6 points on SciFact

The spike's own BM25 (title and text joined into one field, Snowball English stemming, no
stop words, k1 1.2, b 0.75) scores 0.687 on SciFact against the engine's 0.627 (NFCorpus
0.323 vs 0.312; FiQA equal), with a top-10 Jaccard of only 0.35 against the engine's run.
The engine indexes `title` and `text` as separate fields with boosts 2.0 / 1.0 through
tantivy's `en_stem` (SimpleTokenizer + RemoveLong + LowerCaser + Stemmer). This is not a
SPLADE result; it is the lexical-quality lead from the conversation before this spike
(analyzer and field weighting), now with a number: a separate, cheap feature.

### F-003 — A resumed encode reported partial costs; now every shard carries its own

The first v3 FiQA record summed only the shards of the run that finished it (49 of 58: 45.7
docs/s, 1,738 truncated), while dividing the whole corpus by that run's wall time. Each shard
now stores its documents, wall time and truncation count, and the record aggregates every
shard; every encoding was redone under one method (batch 8) — the table above. The metrics
did not change by a digit (the encoder is deterministic on this device); the throughput did:
27–34 docs/s, not 45.7. Raised by review round 1 #1.

### F-004 — The build cost moves from "hours" to "a GPU or a day"

The MiniLM embedder's Wikipedia pass took 11 h on this host's CPU (008). The sparse encoder is
DistilBERT (6 layers, 768 wide) plus a masked-LM head whose vocabulary projection alone is
~6 GFLOP per 256-token passage — about 5× the MiniLM cost, so a candle CPU build of the
Wikipedia corpus would be on the order of 2 days. 013 should plan for encoding documents
outside the Rust build (this script on MPS: ~3–6 h) and ingesting the vectors through an
`add_embedded`-style path, exactly as 004's vector cache feeds the dense stage — the Rust
encoder is still needed for correctness (goldens) and for building small indexes, not for
the corpus.

### F-005 — Two mistakes caught by the oracles before they cost anything

(a) `np.savez` appended `.npz` to the `.part` name, so no shard was ever renamed; (b) every
shard was saved with the first shard's document ids. Both surfaced in the first SciFact
score: the spike's *text BM25* came out at 0.14 with a 0.05 Jaccard against the engine's run
— a number that cannot be right for BM25 on BM25 — before any SPLADE figure was believed.
The recipe check against the model card's own example had passed all along (it never touched
the shards). Fixed; SciFact re-encoded; the card's numbers reproduced.

### F-006 — The plain-BM25-field shortcut is a small-corpus illusion

`bm25x` (expansions as term frequencies, BM25-scored) beats the engine's BM25 on SciFact and
NFCorpus and even fuses best of all there (0.720 / 0.356) — then scores 0.199 on FiQA, below
BM25 itself, because BM25's length normalisation and IDF fight the learned weights on a
57k-document corpus with long documents. The dot product is the only scoring that holds on
all three; 013 needs the custom scorer, not the free field.

## Review round 1

GitHub Copilot, four comments: all taken.

| # | Comment | Action |
|---|---|---|
| 1 | Resume accounting mixed full-corpus and partial-run values (v3 FiQA: 45.7 docs/s and 1,738 truncations were partial) | Taken: per-shard metadata (documents, wall, truncated) saved in each shard and aggregated; throughput over timed documents; every encoding redone under one method and the committed costs regenerated (F-003) |
| 2 | `dot-q10` / `dot-q1000` reports recorded `--scale` (100) instead of their own scale | Taken: `effective_scale(variant)` — the scale from the variant's name, `None` for the float dot product |
| 3 | `--manifest` / `--dataset` / `--variant` were optional to argparse though required by the handlers (`Path(None)` crash) | Taken: required per subcommand; a missing flag is a usage error |
| 4 | The query-side weight check looped over whatever came back and could pass vacuously | Taken: the test asserts the exact expected id list (distinct, non-special, non-zero IDF, ascending) before the weights |

After the round: 16 / 16 checks; verdicts unchanged.

## Deliberately not done

- A symmetric (query-time) encoder — Q1 = A; the OpenSearch card lists its own symmetric
  `neural-sparse-encoding-v2-distill` at 0.528 BEIR average against the doc-only v3's 0.517:
  the ceiling a query-time model would add is ~1 point, for a model on the phone.
- The naver `splade-*` family (CC BY-NC-SA 4.0) — ineligible, not run.
- Re-ranking on top of the sparse variants; BM25 / RRF parameter tuning; the Wikipedia
  corpus (projected).
- Any Rust: `git diff main -- crates/ swift/ apps/ python/ .github/ deny.toml` is empty.
