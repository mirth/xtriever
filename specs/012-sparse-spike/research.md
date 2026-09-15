# Research: Sparse Expansion Spike

**Feature**: `012-sparse-spike` | **Date**: 2026-09-15 | **Plan**: [plan.md](./plan.md)

## D1 — The candidate models and their recipes (model cards, read 2026-09-15)

`opensearch-project/opensearch-neural-sparse-encoding-doc-v2-distill` and
`…-doc-v3-distill`: Apache-2.0, DistilBERT-based (67 M parameters), `AutoModelForMaskedLM`;
files `config.json`, `tokenizer.json`, `model.safetensors`, `idf.json`. Published BEIR
nDCG@10 (the cards): v3-distill average 0.517 — SciFact 0.708, NFCorpus 0.345, FiQA 0.356;
v2-distill average 0.504 — SciFact 0.715, NFCorpus 0.343, FiQA 0.357. Against the engine's
BM25 (0.627 / 0.312 / 0.250) and dense (0.645 / 0.317 / 0.369) those are the numbers the spike
must reproduce or explain.

**Document side** (the card's `get_sparse_vector`): masked-LM logits `output` (batch × tokens
× vocab) → `values = max over tokens of (output × attention_mask)` → v2: `log(1 + relu(values))`;
v3: `log(1 + log(1 + relu(values)))` (the card: "note we update the activation for v3 model")
→ `values[:, special_token_ids] = 0` where
`special_token_ids = [tokenizer.vocab[t] for t in tokenizer.special_tokens_map.values()]`.
Tokenised with `padding=True, truncation=True` (the model's max length, 512 from the config;
the spike records how many documents truncate).

**Query side** ("inference-free"): the query is tokenised, each present token id gets weight
`idf[token]` from `idf.json` (the repo's file, a token → weight map), no model call. The score
is the inner product of the two vectors over the vocabulary. This is what makes the family
phone-friendly: at query time the engine needs the tokenizer and a 30k-entry table.

**Ineligible**: `naver/splade-*` (CC BY-NC-SA 4.0) — recorded, not run. **Ceiling reference**:
not run (Q1 = A); the report quotes published symmetric-SPLADE numbers labelled as such.

**Pinning**: each model gets `reference/models/manifest-sparse-doc-v{2,3}.json` in the 004
manifest shape (repository, revision hash, files with bytes and sha256) and is fetched by
`scripts/fetch-model.sh --manifest …` — the script iterates the manifest's `files` list
generically (`scripts/fetch-model.sh:53–70`), so a four-file manifest needs no script change.
The revision hash is the repo's `main` commit at the time of pinning (`HfApi().model_info`),
written into the manifest, never resolved at run time.

## D2 — The oracle: the 003 reference scorer, imported

`reference/gen_003_fixtures.py` holds `reference(qrels, run)` (BEIR's wrapper semantics:
identical-id drop, pytrec_eval `ndcg_cut.10` / `recall.100`, mean over scored queries, BEIR
rounding), `run_scores`, `load_qrels_tsv`, `probe()`. The spike imports that module (it is a
script with functions; `sys.path` insertion) rather than copying the code, and runs `probe()`
first. The engine's runs are exported with the harness's `--export-run` (JSONL
`{"query_id", "doc_ids"}`, `crates/xtriever-eval/src/run.rs:163`) for `lexical-baseline-v1`,
`dense-baseline-v1` and `hybrid-baseline-v1` (the last two with the 004 vector cache and the
rerank index directories that already exist under `target/`); re-scoring each must reproduce
its committed baseline to 1e-6 (SC-001) — the same check `--verify-run` makes.

## D3 — Environment

`reference/.venv-004` already holds `torch 2.14.0`, `transformers 5.17.0`, `tokenizers 0.23.2`,
`safetensors 0.8.0`, `numpy 2.5.3`, Python 3.12 (arm64), and MPS is available. The spike gets
its own `reference/.venv-012` from `reference/requirements-012.in` pinning the same versions
plus `pytrec_eval==0.5` (003), `scipy` (CSR matrices for the dot product and BM25 over the
expansion field) and `huggingface-hub` (only for `model_info` when pinning). Compute: MPS for
encoding if it is faster (measured on SciFact's first shard and stated), CPU otherwise;
throughput reported for the path used with the thread count.

## D4 — Storage of encodings and runs (under `target/`, never committed)

`target/xt-sparse-cache/<model-key>/<dataset>/docs-NNNNN.npz` — one shard per 1,000
documents in corpus order: `ids` (object array of doc ids), `indptr`, `indices` (int32 vocab
ids), `data` (f32 weights); `queries.npz` likewise for the judged queries (token ids × idf).
`<model-key>` = `<repo-basename>@<revision-8>`; a shard is written only once complete
(`.part` → rename), so a killed run resumes at the shard. `target/xt-sparse-runs/<dataset>/
<name>.jsonl` — runs in the harness's export shape; `<name>.json` — the 003 scorer's report
for it. The engine's exported runs live beside them as `engine-lexical.jsonl`,
`engine-dense.jsonl`, `engine-hybrid.jsonl`.

## D5 — The scoring variants

- **dot**: `scores = D · q` with `D` the CSR matrix (documents × vocab, f32) and `q` the query
  vector; top-100 by score, ties by ascending doc id (stable sort on `(-score, id)`), scores
  of exactly zero excluded (a query with no overlap yields an empty list — counted).
- **dot-q10 / q100 / q1000**: `D` with `data = round(data × S) / S` — the loss of holding the
  weight as an integer term frequency at scale `S` (FR-008). The query side keeps its float
  IDF weights (the engine would too).
- **bm25x**: BM25 over the expansion field alone: `tf = round(d_t × S)` (S from the
  quantisation result, 100 unless the table says otherwise), document length = Σ tf, IDF from
  the expansion field's document frequencies (the standard Lucene/tantivy form
  `ln(1 + (N − df + 0.5) / (df + 0.5))`), k1 = 1.2, b = 0.75; the query = the tokens the query
  side produces (each once; IDF-weighted query terms are *not* used — this is what a plain
  BM25 field would see). Implemented in NumPy/SciPy on the same CSR.
- **bm25x+text**: the expansion field's BM25 score added to a text-field BM25 score with the
  expansion boost `β ∈ {0.5, 1.0, 2.0}` — the text BM25 is the spike's own (SimpleTokenizer-like
  split on non-alphanumerics, lower-case, English Snowball stemmer via `snowballstemmer`, no
  stop words — the engine's `standard_en` shape), checked against the engine's exported
  lexical run (rank overlap reported; the two tokenizers are not identical and the report says
  how far apart they are). This variant is the one the engine would get for free (option (a)
  in the design conversation).
- **rrf(·)**: reciprocal rank fusion, k = 60, over ranked id lists at depth 100 → top-100;
  pairs `(engine-lexical, dot)`, `(engine-dense, dot)`, triple `(engine-lexical, engine-dense,
  dot)`, and the same three with `bm25x` in place of `dot` — so the engine's three-way fusion
  is predicted from the engine's own two runs.

## D6 — Costs

Throughput: wall time per shard and per corpus, documents/s, the device (MPS or CPU) and
`torch.get_num_threads()`; non-zeros per document (mean, p95, max) and per query; truncated
documents (token count > the model's max before truncation, counted on the tokenizer). The
Wikipedia projection: mean non-zeros × 427,947 = postings; bytes ≈ postings × (a tantivy
posting with a term frequency: ~1.5–2.5 B compressed, measured on FiQA by building nothing —
the estimate uses the literature's range and states it; 013 measures). The encoder's cost on
the phone is not in scope (documents are encoded at build time, on the host).

## D7 — What is deliberately not done

- No re-ranking on top of the sparse variants (006's gain is independent of the candidate
  source and would double the runs).
- No Rust: the engine's runs are consumed, never produced here.
- No symmetric model, no non-commercial model (Q1 = A; licence).
- No Wikipedia encoding (projected, not run).
- No tuning of BM25 parameters or of RRF k: the engine's values only, so the numbers predict
  the engine.
