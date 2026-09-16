# Research: Lexical Quality — One Field for BM25

**Feature**: `013-lexical-quality` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## D1 — Attribution: the field layout is the whole gap (measured 2026-09-15/16)

In the 012 spike's BM25 (NumPy, k1 1.2 / b 0.75, split on non-alphanumerics, lower-case,
Snowball English; scored by the 003 reference), nDCG@10:

| layout | SciFact | NFCorpus | FiQA |
|---|---|---|---|
| **engine** (`lexical-baseline-v1`: `title` × 2.0 + `text`, tantivy `en_stem`) | 0.6270 | 0.3115 | 0.2502 |
| spike, engine shape (`title` × 2.0 + `text`) | 0.6207 | 0.3115 | 0.2473 |
| spike, `title` + `text` (boost 1) | 0.6646 | 0.3247 | 0.2473 |
| spike, `text` only | 0.6752 | 0.3176 | 0.2473 |
| **spike, one joined field** (`title + " " + text`) | **0.6867** | **0.3228** | 0.2473 |
| joined + `title` × 0.5 | 0.6854 | 0.3255 | — |
| joined + `title` × 1.0 | 0.6632 | 0.3197 | — |
| joined, k1 0.9 / b 0.4 | 0.6823 | 0.3224 | 0.2340 |
| joined + Lucene stop words | 0.6826 | 0.3150 | 0.2405 |
| joined + stop words + k1 0.9 / b 0.4 | 0.6778 | 0.3139 | 0.2293 |

The engine-shape replica reproduces the engine within 0.6 / 0.0 / 0.3 points, so the
tokenizer and stemmer are not the gap; the joined field is (+6.0 / +1.1 / 0.0 against the
replica). Why: a boosted separate `title` field lets one title term (short field → strong
length normalisation, boost 2.0) outweigh several body matches; BEIR's reference BM25
(Anserini) indexes one `contents` field for the same reason. **Decision**: one field, boost
1.0, nothing else. **Rejected with numbers**: BM25 parameter change, stop words, an extra
title field.

## D2 — Where the change lives: an evaluation configuration, not the engine

`xtriever-eval`'s `EvalConfig` (`src/run.rs:47–56`) lists `FieldSpec { name, from: Source,
analyzer, boost }` with `Source::{Title, Text}` (`:20–25`); `document_fields` (`:449–461`)
builds a document's fields from the corpus columns, skipping empty values when
`omit_empty_fields`. **Decision**: add `Source::TitleAndText` — `title + " " + text`, the
title omitted (and no separator) when empty, the text likewise — and
`EvalConfig::lexical_baseline_v2()`: one field `contents` from it, `standard_en`, boost 1.0,
`omit_empty_fields: true`, `MatchAll`, k 100 (everything but the fields equal to v1, tested).
`HybridConfig::hybrid_baseline_v2()` and `RerankConfig::hybrid_rerank_v2()` wrap it; the
`beir` example's name dispatch (`examples/beir.rs:110–113`) gains the three names. The engine
(`xtriever-lexical`, the pipeline) sees an ordinary one-field schema — nothing changes there.

## D3 — The hybrid build's dense fields follow the lexical schema

`examples/beir.rs:487–491` hard-codes the pipeline's `dense_fields` as `["title", "text"]`,
which v2's schema does not have (the descriptor validates dense fields against the schema).
**Decision**: derive `dense_fields` from the configuration's field names. The dense passage
the pipeline stores is unchanged in content — v1's `title` + `text` joined by one space equals
v2's `contents` — and the vectors come from the 004 cache by value in both, so the dense
list is identical; only the lexical list differs, which is the point.

## D4 — Baselines and their verification

`specs/013-lexical-quality/baselines/{lexical-baseline-v2,hybrid-baseline-v2,hybrid-rerank-v2}.{scifact,nfcorpus,fiqa}.json`,
each verified by `reference/gen_003_fixtures.py --verify-run` (the run exported with
`--export-run`) to 1e-6, `beir compare` against the v1 baseline for the deltas. The hybrid
runs use `--index-dir target/xt-rerank-index-v2/<d>` (the example rebuilds the directory on
every run, `beir.rs:474–481`; a separate path keeps the v1 index that 012's export used).
Cost: the re-rank runs on the three sets took ~50 minutes in 010's gate (RAYON 4).

## D5 — CI

The `eval-smoke` job runs `beir smoke --dataset scifact --baseline
specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json` with the default
configuration (v1). **Decision**: the smoke step passes `--config lexical-baseline-v2` and
the v2 baseline path; v1 stays the default of the example (FR-004: v1 runnable, unchanged).
Still one dataset, no model (standing rule).

## D6 — Documentation

`python/README.md` (the schema example uses `title` 2.0 + `text`: add the recommendation and
its number), `crates/xtriever-cli/src/wiki/chunking.rs`'s schema comment (the Wikipedia index
keeps its two fields until rebuilt — say so), `crates/xtriever-eval/src/lib.rs` (the v2
configurations), 012's report F-002 (resolved by this feature).

## D7 — Not done

The Wikipedia index rebuild and device record (014 re-encodes the corpus anyway); the 007
fixture goldens (boundary parity, their own schema); any analyzer or parameter change (D1).
