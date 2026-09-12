//! Reports, deltas and the smoke verdict (spec FR-017–FR-024; research D7).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dataset::{Counts, Dataset};
use crate::error::{Error, Result};
use crate::metrics::score_queries;
use crate::run::Run;

/// The constitution's benchmark set has three datasets; a "majority" is at least this many.
pub const BENCHMARK_MAJORITY: usize = 2;

/// The lexical stage commit every Feature 003 report refers to (ADR-0006 condition 1).
pub const LEXICAL_COMMIT: &str = "94ddbe67f926badf962b93e8bd29d687e300189a";

/// BEIR's 5-decimal presentation of the means.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Rounded {
    /// `round(mean_ndcg_10, 5)`.
    pub ndcg_10: f64,
    /// `round(mean_recall_100, 5)`.
    pub recall_100: f64,
}

/// FiQA-only observations (003 FR-018; 004 FR-023): numbers, units and the method that produced
/// them. The four optional fields were added by Feature 004 and are omitted when absent, so
/// Feature 003's reports serialise unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Observations {
    /// `du -sk` of the index directory (or the size of the vector index file), in bytes.
    pub index_dir_bytes: u64,
    /// Peak resident set size of the evaluating process, in bytes.
    pub peak_rss_bytes: u64,
    /// How the numbers were obtained.
    pub method: String,
    /// Wall time to embed the corpus, in milliseconds (dense).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed_corpus_ms: Option<u64>,
    /// Wall time to embed and search every judged query, in milliseconds (dense).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_ms: Option<u64>,
    /// Peak RSS of a fresh process that only loads the model through the buffered path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_bytes_buffered: Option<u64>,
    /// Peak RSS of a fresh process that only loads the model through the memory-mapped path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_bytes_mmapped: Option<u64>,
}

/// Identity of a non-lexical stage a report was produced with (Feature 004, spec FR-019/FR-021).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageInfo {
    /// `dense`.
    pub kind: String,
    /// `Embedder::fingerprint()` at run time.
    pub embedder_fingerprint: String,
    /// `buffered` or `mmap`.
    pub load_path: String,
    /// The engine's effective thread count during the run (`RAYON_NUM_THREADS`).
    pub thread_count: usize,
    /// `absolute`: the number is a baseline for a new stage, not a delta against another stage.
    pub baseline: String,
}

/// One dataset's evaluation. **Field order is the on-disk key order** (contract).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvalReport {
    /// Configuration name.
    pub config: String,
    /// Dataset name.
    pub dataset: String,
    /// [`LEXICAL_COMMIT`].
    pub lexical_commit: String,
    /// `git rev-parse HEAD` of the harness at run time.
    pub harness_commit: String,
    /// Manifest hashes of the files that were loaded.
    pub dataset_hashes: BTreeMap<String, String>,
    /// Manifest counts.
    pub counts: Counts,
    /// Judged queries with a result list — the mean's denominator.
    pub scored_queries: u32,
    /// Scored queries with no relevant document (counted as 0.0).
    pub no_relevant_queries: u32,
    /// Results dropped for `doc id == query id`.
    pub dropped_identical: u32,
    /// Queries run but not judged (0 by construction).
    pub unjudged_queries: u32,
    /// Judged queries absent from the run — excluded from the mean (FR-017: "skipped and why").
    pub not_retrieved_queries: u32,
    /// Full-precision mean nDCG@10.
    pub mean_ndcg_10: f64,
    /// Full-precision mean Recall@100.
    pub mean_recall_100: f64,
    /// BEIR-style rounding.
    pub beir_rounded: Rounded,
    /// Per-query `(ndcg_10, recall_100)`, sorted by id.
    pub per_query: BTreeMap<String, (f64, f64)>,
    /// FiQA observations, when measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observations: Option<Observations>,
    /// The stage a non-lexical report was produced with (Feature 004). Always the last key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<StageInfo>,
}

impl EvalReport {
    /// Markdown row set for `report.md`.
    pub fn to_markdown_table(reports: &[EvalReport]) -> String {
        let mut out = String::from(
            "| dataset | config | nDCG@10 | Recall@100 | BEIR-rounded | scored | no-relevant | dropped self-ids |\n|---|---|---|---|---|---|---|---|\n",
        );
        for r in reports {
            out.push_str(&format!(
                "| {} | {} | {:.6} | {:.6} | {} / {} | {} | {} | {} |\n",
                r.dataset,
                r.config,
                r.mean_ndcg_10,
                r.mean_recall_100,
                r.beir_rounded.ndcg_10,
                r.beir_rounded.recall_100,
                r.scored_queries,
                r.no_relevant_queries,
                r.dropped_identical
            ));
        }
        out
    }
}

fn round5(x: f64) -> f64 {
    (x * 1e5).round() / 1e5
}

/// Score a run against its dataset's judgements.
pub fn score(run: &Run, dataset: &Dataset, harness_commit: &str) -> Result<EvalReport> {
    if run.dataset != dataset.name {
        return Err(Error::Run(format!(
            "run is for `{}` but dataset is `{}`",
            run.dataset, dataset.name
        )));
    }
    let m = score_queries(&run.results, &dataset.qrels);
    Ok(EvalReport {
        config: run.config.clone(),
        dataset: run.dataset.clone(),
        lexical_commit: LEXICAL_COMMIT.to_owned(),
        harness_commit: harness_commit.to_owned(),
        dataset_hashes: dataset.hashes.clone(),
        counts: dataset.counts,
        scored_queries: m.scored_queries,
        no_relevant_queries: m.no_relevant_queries,
        dropped_identical: m.dropped_identical,
        unjudged_queries: m.unjudged_queries + run.unjudged_queries,
        not_retrieved_queries: m.not_retrieved_queries,
        mean_ndcg_10: m.mean_ndcg_10,
        mean_recall_100: m.mean_recall_100,
        beir_rounded: Rounded {
            ndcg_10: round5(m.mean_ndcg_10),
            recall_100: round5(m.mean_recall_100),
        },
        per_query: m.per_query,
        observations: None,
        stage: None,
    })
}

/// One metric's before/after on one dataset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeltaRow {
    /// Dataset name.
    pub dataset: String,
    /// `ndcg_10` or `recall_100`.
    pub metric: String,
    /// Baseline value.
    pub before: f64,
    /// Current value.
    pub after: f64,
    /// `after − before`.
    pub abs: f64,
    /// `abs / before`, `None` when `before == 0`.
    pub rel: Option<f64>,
}

/// A comparison of two report sets (spec FR-021, FR-022).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Delta {
    /// One row per dataset present in both sets × metric.
    pub rows: Vec<DeltaRow>,
    /// True iff nDCG@10 fell on the majority of the datasets compared.
    pub adr_trigger: bool,
}

impl Delta {
    /// The FR-021 table plus the FR-022 line, for a PR description.
    pub fn to_markdown(&self) -> String {
        let mut out = String::from(
            "| dataset | metric | before | after | abs | rel |\n|---|---|---|---|---|---|\n",
        );
        for r in &self.rows {
            let rel = r
                .rel
                .map_or_else(|| "n/a".to_owned(), |v| format!("{:+.2}%", v * 100.0));
            out.push_str(&format!(
                "| {} | {} | {:.6} | {:.6} | {:+.6} | {} |\n",
                r.dataset, r.metric, r.before, r.after, r.abs, rel
            ));
        }
        if self.adr_trigger {
            out.push_str(
                "\n**ADR required**: this change lowers nDCG@10 on the majority of benchmark \
                 datasets (constitution Principle II) — it does not merge without an ADR.\n",
            );
        } else {
            out.push_str("\nnDCG@10 did not fall on a majority of datasets; no ADR trigger.\n");
        }
        out
    }
}

/// Compare two report sets, matched by dataset name.
///
/// # Errors
///
/// `Error::Run` when a matched pair was produced by different configurations: a delta is only
/// meaningful within one configuration over time, never across stages (Feature 004 FR-021 — the
/// dense baseline is absolute, not a delta against the lexical one).
pub fn delta(before: &[EvalReport], after: &[EvalReport]) -> Result<Delta> {
    let mut rows = Vec::new();
    let mut fell = 0usize;
    for b in before {
        let Some(a) = after.iter().find(|a| a.dataset == b.dataset) else {
            continue; // present on one side only: skipped, not an error
        };
        if a.config != b.config {
            return Err(Error::Run(format!(
                "reports for `{}` are for different configurations (`{}` vs `{}`); a delta is only \
                 meaningful within one configuration (FR-021)",
                b.dataset, b.config, a.config
            )));
        }
        if a.mean_ndcg_10 < b.mean_ndcg_10 {
            fell += 1;
        }
        for (metric, bv, av) in [
            ("ndcg_10", b.mean_ndcg_10, a.mean_ndcg_10),
            ("recall_100", b.mean_recall_100, a.mean_recall_100),
        ] {
            let abs = av - bv;
            let rel = (bv != 0.0).then(|| abs / bv);
            rows.push(DeltaRow {
                dataset: b.dataset.clone(),
                metric: metric.to_owned(),
                before: bv,
                after: av,
                abs,
                rel,
            });
        }
    }
    // "Majority of benchmark datasets" (constitution II, FR-022) is a majority of the FIXED
    // three-dataset set — at least two — however many reports were supplied. A SciFact-only
    // smoke that falls is one of three, not a majority.
    Ok(Delta {
        rows,
        adr_trigger: fell >= BENCHMARK_MAJORITY,
    })
}

/// Why a smoke run failed (spec FR-024).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("smoke: {metric} fell from {baseline} to {current}")]
pub struct SmokeFailure {
    /// The metric that decreased.
    pub metric: String,
    /// Baseline value.
    pub baseline: f64,
    /// Current value.
    pub current: f64,
}

/// FR-024 with tolerance 0: fail if either metric is lower than the baseline.
///
/// The configuration check runs **first** (through [`delta`]), so a mixed-configuration pair is
/// always reported as such and never as an ordinary regression (Feature 004 FR-021).
pub fn smoke(
    baseline: &EvalReport,
    current: &EvalReport,
) -> std::result::Result<Delta, SmokeFailure> {
    let d = delta(
        std::slice::from_ref(baseline),
        std::slice::from_ref(current),
    )
    .map_err(|e| SmokeFailure {
        metric: format!("configuration ({e})"),
        baseline: baseline.mean_ndcg_10,
        current: current.mean_ndcg_10,
    })?;
    for (metric, b, c) in [
        ("ndcg_10", baseline.mean_ndcg_10, current.mean_ndcg_10),
        (
            "recall_100",
            baseline.mean_recall_100,
            current.mean_recall_100,
        ),
    ] {
        if c < b {
            return Err(SmokeFailure {
                metric: metric.to_owned(),
                baseline: b,
                current: c,
            });
        }
    }
    Ok(d)
}
