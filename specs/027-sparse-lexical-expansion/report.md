# Report: Optional Sparse Lexical Expansion (Feature 027)

**Feature**: 027 · **Branches**: `027-sparse-lexical-expansion` (PR A, #33),
`027-sparse-lexical-expansion-b` (PR B, #34), `027-sparse-lexical-expansion-c` (PR C) ·
**Status**: done — sparse expansion as an opt-in per-index option, from the encoder through
every surface; the full pipeline within the gate on all three datasets, FiQA +0.0172 nDCG@10

## Verdict

An index can now be built **sparse**. Each passage is expanded by the pinned learned-sparse
encoder into weighted vocabulary terms, and BM25 scores those terms beside the text. The
query side needs no model, because the index carries it, so a device searches a sparse index
with nothing but the index. The option is off by default, and nothing changes without it.

Through the full pipeline the option does what the spike said it would, to the printed digit:
FiQA gains +0.0172 nDCG@10, SciFact and NFCorpus stay within the 0.005 gate. **Without the
re-ranker** SciFact and NFCorpus lose 0.0070 and 0.0056. The owner accepted that as the option's
documented cost (ADR-0017, option A). So the option is for corpora whose questions are worded
unlike their answers (FiQA-shaped: no titles), searched with the re-ranker, and every surface
that exposes it says so.

## Success criteria

| criterion | result | evidence |
|---|---|---|
| SC-001 FiQA full-pipeline nDCG@10 ≥ +0.010 | **PASS**: +0.0172 | `runs/hybrid-sparse-rerank-v1.fiqa.json` against `runs/hybrid-rerank-v3.fiqa.json` |
| SC-002 no full-pipeline nDCG@10 or Recall@100 below −0.005 | **PASS**: nearest NFCorpus nDCG@10 −0.0048; Recall@100 ≥ 0 everywhere | the table below |
| SC-003 option-off baselines reproduced exactly | **PASS for `hybrid-rerank-v3`**, identical to Feature 026's record in every per-query score. `hybrid-baseline-v2`'s only committed record (Feature 013) predates 026's eight-bit embedder, so its re-run is a comparator, not a reproduction | `runs/hybrid-rerank-v3.*.json` |
| SC-004 encoder weights within tolerance of the reference | **PASS**: largest difference 1.6e-5 against 1e-4 on 12 fixture documents; query terms exact; bit-identical across thread counts and load paths | `crates/xtriever-dense/tests/sparse_oracle.rs`, `sparse_load_paths.rs` |
| SC-005 a sparse index needs no artefact beyond itself; lexical stage ≤ 4× | **PASS**: the query side is inside the index; 2.49× / 3.86× / 3.42× | `runs/sizes.json`, `runs/measure-027.log` |

## The measurement (T026–T028, 2026-09-23)

**Full pipeline**, `hybrid-sparse-rerank-v1` against `hybrid-rerank-v3`:

| | nDCG@10 | Δ | Recall@100 | Δ | spike |
|---|---|---|---|---|---|
| SciFact | 0.72166 | −0.0003 | 0.95500 | 0.0000 | 0.72166 |
| NFCorpus | 0.35767 | −0.0048 | 0.32166 | +0.0007 | 0.35767 |
| FiQA | 0.40680 | +0.0172 | 0.71325 | +0.0073 | 0.40703 (Python weights) |
| mean | | +0.0040 | | +0.0027 | |

**Fused only**, `hybrid-sparse-v1` against `hybrid-baseline-v2` (ADR-0017):

| | nDCG@10 | Δ | Recall@100 Δ |
|---|---|---|---|
| SciFact | 0.70846 | −0.0070 | 0.0000 |
| NFCorpus | 0.34803 | −0.0056 | +0.0007 |
| FiQA | 0.40088 | +0.0308 | +0.0073 |

The harness was checked against the spike's own tools (T026): the same Rust-encoded SciFact
weights through `splade_field` and `rerank_runs` give a byte-identical re-ranked run. No query's
expansion was skipped on any dataset.

**Cost** (`runs/sizes.json`): the lexical stage grows 2.49× (SciFact), 3.86× (NFCorpus) and
3.42× (FiQA). Two builds of the same documents differ by up to 1.3% (FiQA), because of
tantivy's segment layout. Encoding on the build host (CPU, 10 threads) ran at 3.1, 2.9 and 6.0
passages per second: FiQA's 57,638 passages took 2 h 40 min. 455, 330 and 2,417 passages ran
past the encoder's 512-token window. A 200-article Simple English Wikipedia slice built through
the command line encoded 873 passages at 5.2 per second. No speed claim is made (research D12).

## PR A: the encoder, the query side, the oracle (merged #33)

`xtriever-dense::sparse`: the pinned `opensearch-neural-sparse-encoding-doc-v3-distill` as a
DistilBERT forward pass over `candle_nn` layers with **exact GELU** (candle 0.9.2's own DistilBERT
uses the tanh approximation, which the model's reference does not), one document at a time,
`log1p(log1p(relu(max logits)))`, special tokens zeroed. The query side: distinct non-special
token ids with a positive query-table entry. `field_text`: `s<id>` repeated
`round(weight × scale)` times, refusing any expansion the encoder could not have produced. The
oracle is `reference/gen_027_fixtures.py` (PyTorch). Three review rounds; the owner accepted the
size.

## PR B: the index, the format, the harness (merged #34)

`HybridIndex::create_sparse`: the reserved `_sparse` field, the query side copied into
`<index>/sparse/` and verified by hash at open, descriptor format version 3 for sparse indexes
only (ADR-0016, accepted). `search` spells the user's text fields out and adds one `_sparse`
term per kept query token. A query the query side cannot tokenise degrades to the text fields
(`StageReport::sparse_skipped`). A document with no text has no expansion. `add` needs the
encoder, `add_embedded` is refused, and `add_encoded` takes cached expansions. The harness gained
`hybrid-sparse-v1`, `hybrid-sparse-rerank-v1` and a resumable, per-recipe sparse cache. Seven
review rounds and one Copilot round. The measurement moved to PR C by the owner's decision.

## PR C: the surfaces and the results

The FFI's `IndexConfig.sparse` (`SparseOptionConfig`: the encoder directory, an optional scale
and boost defaulting to the engine's) builds a sparse index with the encoder attached.
`IndexInfo.sparse` and the index's own format version report it, and `StageReport.sparse_skipped`
reaches the bindings. Python exports the two new records and documents the option. Swift and
Kotlin alias the generated types, so their packages carry the change unaltered.
`xtriever wiki build --sparse-encoder` builds a sparse artefact whose corpus identity names the
expansion, and whose build record gains the encoder, the truncation count and the encoding
rate. The packagers never stage the encoder, which a test guards.

## Findings

- **candle's DistilBERT is not the model's DistilBERT.** Its GELU is the tanh approximation.
  Writing the forward pass over candle's layers with exact GELU is what brings the weights
  within 1.6e-5 of the reference.
- **An empty passage is not an empty expansion.** Given only `[CLS]` and `[SEP]`, the encoder
  weights about two dozen vocabulary entries (the fixture's `d07`). The pipeline writes an empty
  field for such a passage.
- **The option's benefit is a re-ranking benefit on two of three datasets.** Recall@100 never
  falls, but on SciFact and NFCorpus the fused order gets slightly worse, and the cross-encoder
  recovers it (ADR-0017).

## Follow-ups (not in this feature)

- The embedder's and the re-ranker's loaders verify a file's hash and then read it again to
  parse; the sparse encoder reads once (`model::read_pinned`). Closing that gap touches
  `xtriever-rerank`.
- The lexical stage reads `Match(None, …)` as every indexed text field, `_sparse` included. The
  pipeline spells the user's fields out instead. A schema flag keeping a field out of default
  matching would be the cleaner fix; it changes a core type and needs its own ADR (ADR-0016,
  Consequences).

## Deliberately not done

- **Not the default.** The mean gain (+0.004) is below Feature 016's +0.005 floor, and the
  option costs 0.006–0.007 on two datasets without re-ranking.
- **The shipped Wikipedia artefact is not rebuilt with it.** Encoding the full edition would take
  about a day on the build host, and the corpus is not FiQA-shaped.
- **No speed claim, no latency budget.** Throughput and sizes are recorded, not promised.
- **No quantised sparse encoder.** Model weights come from the owner as pinned artefacts.
