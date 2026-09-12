# Contract: `xtriever-eval` public surface and the `beir` command

**Feature**: `003-eval-harness` | **Date**: 2026-09-12 | **Plan**: [../plan.md](../plan.md)

The spec promises no API stability yet (Assumptions); the stable contracts are the **report
format**, the **fixture set** and the **command lines** CI and PR authors use.

## Library (`std`-only; depends on `xtriever-core`, `serde`, `serde_json`, `sha2`)

```rust
pub mod dataset {
    pub struct Manifest;            // load(path), verify_file(path, entry) -> Result<()>
    pub struct Dataset { corpus, queries, qrels }   // Dataset::load(manifest, name, cache_dir)
}
pub mod metrics {
    pub fn ndcg_at(ranked: &[&str], relevant: &BTreeMap<String, u32>, k: usize) -> f64;
    pub fn recall_at(ranked: &[&str], relevant: &BTreeMap<String, u32>, k: usize) -> f64;
}
pub mod run {
    pub struct EvalConfig;          // lexical_baseline_v1()
    pub fn build(dataset: &Dataset, cfg: &EvalConfig) -> (Schema, Vec<Document>, IdMap);
    pub fn execute(index: &dyn LexicalIndex, ids: &IdMap, dataset: &Dataset, cfg: &EvalConfig) -> Result<Run>;
}
pub mod report {
    pub fn score(run: &Run, qrels: &Qrels) -> EvalReport;
    pub fn delta(before: &EvalReport, after: &EvalReport) -> Delta;
    pub fn smoke(baseline: &EvalReport, current: &EvalReport) -> Result<Delta, SmokeFailure>;
}
```

Error type: a `thiserror` enum local to the crate (`Manifest`, `HashMismatch { path, expected,
actual }`, `Parse { path, line }`, `Io`, plus `xtriever_core::Error` via `From`). No `unwrap`,
no `panic`.

## Command (`cargo run -p xtriever-eval --example beir -- <subcommand>`)

| subcommand | does | exit |
|---|---|---|
| `verify [--cache DIR] <dataset…>` | re-verify cached files against the manifest and print counts | 0 / 1 |
| `run --dataset D --config lexical-baseline-v1 [--out report.json] [--export-run run.jsonl]` | index, retrieve, score; write the report | 0 / 1 |
| `delta before.json after.json` | print the FR-021 table + FR-022 trigger line | 0 |
| `smoke --dataset scifact --baseline baseline.json` | `run` then FR-024; prints the delta | 0 pass / 2 fail |

The example depends on `xtriever-lexical` as a dev-dependency; the library does not.

## Report file (stable)

JSON, pretty-printed, keys in this order: `config`, `dataset`, `lexical_commit`,
`harness_commit`, `dataset_hashes`, `counts { documents, queries, judged_queries, judgement_pairs }`,
`scored_queries`, `no_relevant_queries`, `dropped_identical`, `unjudged_queries`,
`not_retrieved_queries`, `mean_ndcg_10`, `mean_recall_100`, `beir_rounded { ndcg_10, recall_100 }`, `per_query`
(sorted by id), `observations` (optional).

## Baseline location

`specs/003-eval-harness/baselines/lexical-baseline-v1.{scifact,nfcorpus,fiqa}.json` — the files
the smoke and every future delta read.

## Scripts

- `scripts/fetch-beir.sh [dataset…]` — curl + shasum + unzip into `reference/datasets/beir/`;
  idempotent; exits non-zero on any hash mismatch.
- `reference/gen_003_fixtures.py` — goldens; `--verify-run run.jsonl --qrels path` scores a
  Rust-exported run with `pytrec_eval` and BEIR's wrapper semantics.
