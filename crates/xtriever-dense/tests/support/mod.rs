//! Shared helpers for the Feature 004 acceptance suite: fixture loading, the model directory,
//! float comparisons and the search-golden schema.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use serde::Deserialize;
use xtriever_core::{DocId, DocSet, Metric};

/// `reference/fixtures/004/`, resolved from the crate manifest so it works from any cwd.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/004")
}

/// The git-ignored model directory, overridable with `XTRIEVER_MODEL_DIR` (quickstart Step 1).
pub fn model_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_MODEL_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/models/all-MiniLM-L6-v2"),
        PathBuf::from,
    )
}

/// The git-ignored eight-bit model directory (Feature 026), overridable with
/// `XTRIEVER_MODEL_DIR_Q8`: the owner-pinned GGUF beside the float model's configuration and
/// tokenizer, staged by `scripts/fetch-model.sh --manifest reference/models/manifest-q8.json`.
pub fn model_dir_q8() -> PathBuf {
    std::env::var_os("XTRIEVER_MODEL_DIR_Q8").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/models/all-MiniLM-L6-v2-q8"),
        PathBuf::from,
    )
}

pub fn load_json<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let path = fixtures_dir().join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

// ── embeddings.json ─────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct Tolerance {
    pub cosine_min: f64,
    pub max_abs_diff: f64,
    pub unit_norm_abs: f64,
}

#[derive(Deserialize)]
pub struct EmbeddingCase {
    pub id: String,
    pub text: String,
    pub input_ids: Vec<u32>,
    pub attention_mask: Vec<u32>,
    pub n_real_tokens: usize,
    pub vector: Vec<f32>,
}

#[derive(Deserialize)]
pub struct EmbeddingGoldens {
    pub fingerprint: String,
    pub dim: usize,
    pub tolerance: Tolerance,
    pub cases: Vec<EmbeddingCase>,
}

pub fn embeddings() -> EmbeddingGoldens {
    load_json("embeddings.json")
}

// ── search.json / mutations.json ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct Expected {
    pub id: u32,
    pub score: f64,
}

#[derive(Deserialize)]
pub struct SearchCase {
    pub k: usize,
    pub allowed: Option<Vec<u32>>,
    pub expected: Vec<Expected>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub id: String,
    pub vector: Vec<f32>,
    pub cases: Vec<SearchCase>,
}

#[derive(Deserialize)]
pub struct Row {
    pub id: u32,
    pub vector: Vec<f32>,
}

#[derive(Deserialize)]
pub struct DesignedTie {
    pub query: String,
    pub k: usize,
    pub ids: Vec<u32>,
    pub winner: u32,
}

#[derive(Deserialize)]
pub struct SearchSet {
    pub id: String,
    pub dim: usize,
    pub metric: String,
    pub designed_ties: Vec<DesignedTie>,
    pub rows: Vec<Row>,
    pub queries: Vec<SearchQuery>,
}

#[derive(Deserialize)]
pub struct SearchGoldens {
    pub score_abs_tol: f64,
    pub sets: Vec<SearchSet>,
}

pub fn search() -> SearchGoldens {
    load_json("search.json")
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Step {
    Add {
        id: u32,
        vector: Vec<f32>,
    },
    Delete {
        ids: Vec<u32>,
    },
    Commit,
    Reopen,
    Expect {
        len: u64,
        query: Vec<f32>,
        k: usize,
        results: Vec<Expected>,
    },
}

#[derive(Deserialize)]
pub struct Mutations {
    pub dim: usize,
    pub metric: String,
    pub fingerprint: String,
    pub score_abs_tol: f64,
    pub steps: Vec<Step>,
}

pub fn mutations() -> Mutations {
    load_json("mutations.json")
}

pub fn parse_metric(s: &str) -> Metric {
    match s {
        "cosine" => Metric::Cosine,
        "dot" => Metric::Dot,
        "euclidean" => Metric::Euclidean,
        other => panic!("unknown metric {other}"),
    }
}

pub fn doc_set(ids: &[u32]) -> DocSet {
    let mut set = DocSet::new();
    for &id in ids {
        set.insert(DocId(id));
    }
    set
}

// ── float helpers ───────────────────────────────────────────────────────────────────────────

pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (&x, &y) in a.iter().zip(b) {
        dot += f64::from(x) * f64::from(y);
        na += f64::from(x) * f64::from(x);
        nb += f64::from(y) * f64::from(y);
    }
    dot / (na.sqrt() * nb.sqrt())
}

pub fn max_abs(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| (f64::from(x) - f64::from(y)).abs())
        .fold(0.0, f64::max)
}

pub fn norm(a: &[f32]) -> f64 {
    a.iter()
        .map(|&x| f64::from(x) * f64::from(x))
        .sum::<f64>()
        .sqrt()
}

pub fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// Compare a hit list against a golden: ids and order exact, scores within `tol`.
pub fn assert_hits(hits: &[xtriever_core::Hit], expected: &[Expected], tol: f64, label: &str) {
    let got: Vec<u32> = hits.iter().map(|h| h.id.0).collect();
    let want: Vec<u32> = expected.iter().map(|e| e.id).collect();
    assert_eq!(got, want, "{label}: ids/order differ");
    for (h, e) in hits.iter().zip(expected) {
        let diff = (f64::from(h.score) - e.score).abs();
        assert!(
            diff <= tol,
            "{label}: score for id {} differs by {diff:e} (got {}, want {})",
            h.id,
            h.score,
            e.score
        );
    }
}

// ── Feature 024 helpers: one definition of "same result", one row layout, one generator ─────

/// A hit list as `(id, score bits)` — the comparison every 024 suite makes.
pub fn hit_bits(hits: &[xtriever_core::Hit]) -> Vec<(u32, u32)> {
    hits.iter().map(|h| (h.id.0, h.score.to_bits())).collect()
}

// ── Feature 026: the eight-bit scheme, the crate's own, in the shapes the suites use ─────────
//
// These are thin wrappers over `xtriever_dense::quantise`, **not** an independent restatement:
// a copy of the crate's code would share its bugs while claiming to check them (review round
// 5, finding 6). What is independent is `reference/dense_format3.py`, which mints every golden
// these suites replay (`search.json`, `mutations.json`, `hybrid.json`, `v3_oracle.json`) and
// checks them in CI. What the Rust suites add on top is independent in what they do *with* the
// codes — accumulation, cosine, order, persistence — not in how the codes are made.

use xtriever_dense::quantise::Quantised;

/// The crate's quantiser, as `(codes, scale)`.
pub fn quantise(vector: &[f32]) -> (Vec<i8>, f32) {
    let q = xtriever_dense::quantise::quantise(vector);
    (q.codes, q.scale)
}

/// `code × scale` per component, the crate's recovery.
pub fn recover(codes: &[i8], scale: f32) -> Vec<f32> {
    xtriever_dense::quantise::recover(&Quantised {
        codes: codes.to_vec(),
        scale,
    })
}

/// What the stage recovers for a stored row.
pub fn recovered(vector: &[f32]) -> Vec<f32> {
    xtriever_dense::quantise::recover(&xtriever_dense::quantise::quantise(vector))
}

/// The norm a stored row carries — of the row as stored, `sqrt(Σ code²) × scale` — which is
/// what cosine divides by.
pub fn recovered_norm(codes: &[i8], scale: f32) -> f64 {
    xtriever_dense::quantise::norm(&Quantised {
        codes: codes.to_vec(),
        scale,
    })
}

/// Assert that `got` is what dense format 3 recovers for `expected`: every component within
/// half a quantisation step — the row's scale, `max|component| / 127` floored at
/// `f32::MIN_POSITIVE`, so a vector below the floor recovers to within half the floor and its
/// smallest components to exactly zero (Feature 026, ADR-0015). A committed row is eight-bit
/// codes and a scale, so it never returns the bytes that were added — that is the format, not
/// a defect.
#[track_caller]
pub fn assert_recovered(got: Option<Vec<f32>>, expected: &[f32], label: &str) {
    let got = got.unwrap_or_else(|| panic!("{label}: no vector"));
    let (_, step) = quantise(expected);
    assert_eq!(got.len(), expected.len(), "{label}: width");
    for (i, (recovered, original)) in got.iter().zip(expected).enumerate() {
        assert!(
            (recovered - original).abs() <= step / 2.0 + f32::EPSILON,
            "{label}: component {i} recovered {recovered}, added {original}, step {step}"
        );
    }
}

/// Rewrite `manifest.bin`'s JSON header in place (magic · hdr_len · JSON · tombstones), for
/// the refusal suites.
pub fn rewrite_manifest_header(dir: &Path, edit: impl Fn(&str) -> String) {
    let path = dir.join("manifest.bin");
    let bytes = std::fs::read(&path).unwrap();
    let hdr_len = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    let header = std::str::from_utf8(&bytes[16..16 + hdr_len]).unwrap();
    let edited = edit(header);
    let mut out = Vec::new();
    out.extend_from_slice(&bytes[..8]);
    out.extend_from_slice(&(edited.len() as u64).to_le_bytes());
    out.extend_from_slice(edited.as_bytes());
    out.extend_from_slice(&bytes[16 + hdr_len..]);
    std::fs::write(&path, out).unwrap();
}

/// Bytes per row of dense format version 3: `id u32 · norm f32 · scale f32 · codes dim × i8`
/// (Feature 026, ADR-0015) — 396 at dimension 384, against 1,544 in version 2.
pub const fn row_bytes(dim: usize) -> u64 {
    12 + dim as u64
}

/// The row file of generation `g` under `dir`.
pub fn row_file(dir: &Path, generation: u64) -> PathBuf {
    dir.join(format!("vectors.{generation}.bin"))
}

/// The same 4-dim test vector for an id across the append / compact / crash suites.
pub fn vec_for(i: u32) -> Vec<f32> {
    let f = i as f32;
    vec![f + 1.0, (f * 0.7).sin(), (f * 0.3).cos(), 1.0]
}

/// The bytes of every file directly under `dir`.
pub fn dir_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum()
}

/// A 64-bit LCG (Knuth's MMIX constants): reproducible test data without a dependency. The
/// oracle (`index_oracle.rs`) and the bench share it, so a seed describes the same data in both.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() >> 33) as usize % n
    }

    /// A vector of `dim` components in roughly [-1, 1], never all-zero (cosine needs a norm).
    pub fn vector(&mut self, dim: usize) -> Vec<f32> {
        loop {
            let v: Vec<f32> = (0..dim)
                .map(|_| ((self.next_u64() >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0)
                .collect();
            if v.iter().any(|x| *x != 0.0) {
                return v;
            }
        }
    }
}
