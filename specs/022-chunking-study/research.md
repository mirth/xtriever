# Research: The Chunking Study

Read on 2026-09-17: `reference/rerank_study.py` and `reference/sparse_remeasure.py` (the
014/016 study pattern), `reference/gen_003_fixtures.py` (the scorer), `reference/gen_008_fixtures.py`
(the contract chunker's reference implementation), `crates/xtriever-eval/src/run.rs` (the
baselines' document shaping and depth), `crates/xtriever-ffi/src/index.rs` (`SearchOptions.depth`
→ the candidate depth per search), the 015 and 013 baseline files, `reference/requirements-012.in`,
`scripts/setup-reference-venv.sh`; the token-length survey of the three corpora (spec input).

## D1 — Environment: `reference/.venv-022`

`reference/requirements-022.in` = the 012 pins (torch 2.14.0, transformers 5.17.0, tokenizers
0.23.2, safetensors 0.8.0, numpy 2.5.3, pytrec_eval 0.5, huggingface-hub, pytest, pip-tools)
+ `chonky==0.1.7` (the version 021 pinned, satisfied by the 012 stack).
`requirements-022.txt` is the 012 lock verbatim plus `chonky==0.1.7`: the 012 lock is
pinned but **not hashed** (0 hashes — contrary to what this document first said, and to
`scripts/setup-reference-venv.sh`'s `--require-hashes`, which cannot have produced
`.venv-012`); a fresh `pip-compile --generate-hashes` downloaded more than 1 GB of torch
wheels in 30 minutes without finishing and `--no-index` cannot see torch, so the lock is
derived, not re-resolved, and says so in its header. The venv: `uv venv --python 3.12
reference/.venv-022 && VIRTUAL_ENV=reference/.venv-022 uv pip install -r
reference/requirements-022.txt`, then the `xtriever` wheel from `target/wheels/` (a local
build, not a resolvable requirement). The study imports `gen_003_fixtures` (scoring) and
`gen_008_fixtures` (the contract chunker) by `sys.path`, as `rerank_study.py` does.

## D2 — Reading BEIR as the harness does

`corpus.jsonl` (`_id`, `title`, `text`), `queries.jsonl` (`_id`, `text`), `qrels/test.tsv`
via `gen_003_fixtures.load_qrels_tsv`. Only the queries present in the test qrels are searched
(the baselines score 300 / 323 / 648 queries; pytrec_eval scores queries in both the run and
the qrels, and the mean is over those — `reference()`); searching the others would change
nothing. `reference()` also drops a hit whose id equals the query id, as the 003 probe fixed.

## D3 — Document shaping: the baselines' join

`contents = title + " " + text` when both are non-empty, else the non-empty one
(`run.rs::join_title_text`); for a passage, `title + " " + passage`. One field `contents`
under `standard_en`, boost 1.0, also the dense field: `IndexConfig(fields=[FieldDef("contents",
TEXT("standard_en"))], dense_fields=["contents"])` with the engine's defaults — the 013 v2
layout the baselines were measured with. Chunked passages carry `ChunkInfo(parent=doc id,
ordinal)` so the document id is the engine's provenance, never parsed from a string; `whole`
passages carry no chunk info and their external id is the document id.

## D4 — Depth and k

`SearchOptions(depth=…)` is the candidate depth per search (`index.rs:393`), so one index per
(variant, dataset) serves both configurations: the anchor `whole@100` (`k=100, depth=100` —
the harness's) and the study's `@300` (`k=300, depth=300`). Re-rank depth 0 and 20, the
recorded mode (interpolate α 0.5). Cells are named `<variant>-d<rerank depth>@<k>.<dataset>`.

## D5 — The four splitters

- `whole`: the document as is.
- `contract`: `gen_008_fixtures.chunk(text, budget, cost)` with `budget = 256 −
  token_count(title)` and `cost(unit) = token_count(unit) − 2` from the embedder's
  `tokenizer.json` (no truncation / padding — the 021 `Window`); a title alone at ≥ 256
  positions → the document indexed whole and counted (`title_fills_window`).
- `chonky`: `ParagraphSplitter(model_id=<pinned dir>, device="cpu")` on `text`; non-empty
  stripped slices; the partition asserted (021's `Splitter`). The split is cached per dataset
  (`target/xt-chunking-study/chonky.<dataset>.jsonl`) so `chonky-bounded` reuses it.
- `chonky-bounded`: the chonky chunks, then (a) fragments — a chunk whose own
  `token_count(chunk) < 16` — merged into the preceding chunk of the document (the first into
  the following) with a newline; (b) any chunk whose passage exceeds the window re-chunked by
  the contract chunker with the same budget (paragraphs → sentences → words → fragments, so
  the bound is guaranteed); (c) a remainder under 16 positions left by (b) merged into its
  predecessor only when the result stays within the window, otherwise kept and counted.
  So: zero passages over the window; under-16 passages only where merging would breach the
  window (counted; expected near zero) or where the whole document is that short. The spec's
  SC-003 is read this way (it said "unless it is a whole document"; the window exception is
  the one case the two fixes cannot both satisfy).

## D6 — MaxP aggregation

Hits come ordered by the engine (fused order at depth 0; the re-ranked order at depth 20);
walking them in order and keeping each document at its first occurrence *is* MaxP with ties
by first occurrence. The document list is truncated to 100. A query with fewer than 100
distinct documents keeps what it has; the share of such queries per cell is counted.

## D7 — Run and score files

The repository's run shape (`rerank_study.read_run` / `write_run`: JSONL `{"query_id",
"doc_ids"}` — the 014/016 convention; the spec's "TREC run file" is this shape) and the 003
scorer's report shape (`per_query`, `scored_queries`, `mean_ndcg_10`, `mean_recall_100`),
compared with `rerank_study.compare_metrics` (tolerance 1e-6 on every query and the means)
against `specs/013-lexical-quality/baselines/hybrid-baseline-v2.*.json` (fused) and
`specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.*.json` (re-ranked).

## D8 — Resumability and records

Indexes under `target/xt-chunking-study/<variant>.<dataset>/` with a `build.json` (passages,
documents, over-window count, under-16 count, title-fills-window count, split seconds, embed
seconds); a present run file is not recomputed; a present index is reused. The build
records are copied into the report's table.

## D9 — The decision

`decide` reads the score files; constants `MEAN_GAIN = 0.005`, `MAX_DROP = 0.005`,
`RECALL_DROP = 0.005`, `MIN_POSITIONS = 16` as literals (tested); per variant: the datasets it
ran on, the re-ranked mean nDCG@10 vs `whole@300`'s on the same datasets, the per-dataset
deltas, the mean Recall@100 delta, `recommended: bool`, `scope: "three-way" | "two-way"`;
the best chunker by the rule, ties to `contract`; `owner-decision.json` in the 016 shape.
The `whole@100` vs `whole@300` difference is reported beside, so the depth's own effect is
visible.

## D10 — Cost and order

Per the estimate given to the owner (~1 h 40 SciFact, ~1 h 30 NFCorpus, ~2 h 10 FiQA `whole`,
~2 h 35 FiQA winner): the order is SciFact `whole` (anchor) → NFCorpus `whole` (anchor) →
the six chunked SciFact/NFCorpus indexes → the cells → provisional decision → FiQA `whole`
(anchor) → FiQA winner → final decision. Each block runs unattended in the background with
a progress monitor.
