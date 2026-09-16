//! Xtriever evaluation harness — BEIR SciFact / NFCorpus / FiQA, nDCG@10 and Recall@100.
//!
//! The library is `std`-only and depends on `xtriever-core` alone; it scores any `LexicalIndex`
//! through a named [`run::EvalConfig`]. The `beir` example binary wires it to `xtriever-lexical`
//! (a dev-dependency). Contract: `specs/003-eval-harness/contracts/eval-harness.md`.
//!
//! # Metric conventions (spec FR-003)
//!
//! These are `pytrec_eval`'s rules as BEIR's `EvaluateRetrieval.evaluate` applies them, measured
//! against `pytrec_eval 0.5` and pinned by the goldens in `reference/fixtures/003/`:
//!
//! | situation | rule |
//! |---|---|
//! | nDCG gain | linear, `grade / log2(rank + 1)`; ideal from all grades > 0, cut at 10 |
//! | Recall@100 | relevant (grade > 0) among the first 100 distinct ids ÷ all relevant |
//! | a repeated document id | counts once, at its first rank |
//! | a result whose id equals the query id | dropped before scoring, counted as `dropped_identical` |
//! | judged query with no relevant document | scored 0.0 and **counted** in the mean |
//! | judged query with an empty result list | scored 0.0 and counted |
//! | query in the run but not judged | **ignored**, counted as `unjudged_queries` |
//! | judged query absent from the run | **excluded** from the mean, counted as `not_retrieved_queries` |
//! | ties | none — the retriever's order is the ranking; nothing here re-sorts |
//! | the mean | per-query values summed in ascending query-id order (deterministic) |
//!
//! # Configurations
//!
//! `lexical-baseline-v1` (003) indexes `title` × 2.0 beside `text`; `lexical-baseline-v2`
//! (Feature 013) indexes one joined `contents` field (`title + " " + text`, an empty side
//! omitted) under the same analyzer, query kind and depth. The two-field layout was measured
//! 5.9 / 1.1 nDCG@10 points behind the joined one on SciFact / NFCorpus (FiQA has no titles):
//! a boosted short field lets one title term outweigh several body matches, which is why
//! BEIR's reference BM25 indexes one `contents` field too. `hybrid-baseline-v2` and
//! `hybrid-rerank-v2` are their v1s over the v2 lexical list; `hybrid-rerank-v3` (Feature 015)
//! re-ranks the v2 fused list under the interpolating rule — `RerankConfig::mode`, passed to the
//! pipeline as a per-search override — and reproduces Feature 014's `lin-0.5-d20` cells
//! (0.7207 / 0.3622 / 0.3910 nDCG@10); every v1 stays runnable and
//! reproduces its baseline. The CI smoke compares against the v2 lexical baseline.
//!
//! Datasets are never committed: `scripts/fetch-beir.sh` downloads them into a git-ignored cache
//! and both the script and [`dataset::Dataset::load`] verify every file's size and SHA-256
//! against `reference/datasets/beir-manifest.json` before it is read.

pub mod dataset;
pub mod error;
pub mod metrics;
pub mod report;
pub mod run;

pub use error::{Error, Result};
