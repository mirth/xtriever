# Contract: the public surface of `xtriever-dense` and the harness extension

**Feature**: `004-dense-stage` | **Date**: 2026-09-12 | **Plan**: [../plan.md](../plan.md)

This is the whole public API after Feature 004. Anything not listed is `pub(crate)`. No
`candle::*`, `tokenizers::*` or `memmap2::*` type appears in a public signature (Principle V). The
spec promises no API stability yet (Assumptions); the stable contracts are the **on-disk index
format** (data-model), the **fingerprint string** (research D6) and the **report extension**.

```rust
// ── model identity ──────────────────────────────────────────────────────────────────────
pub mod model {
    pub struct PinnedFile { pub name: &'static str, pub bytes: u64, pub sha256: &'static str }
    pub struct PinnedModel {
        pub repository: &'static str, pub revision: &'static str,
        pub files: [PinnedFile; 3],          // config.json, tokenizer.json, model.safetensors
        pub dim: usize, pub max_tokens: usize,
    }
    pub const PINNED: PinnedModel;
    /// research D6 — assembled from `PINNED` at compile time.
    pub const FINGERPRINT: &str;
    /// Verify every pinned file's size and SHA-256 in `dir`; first failure wins (FR-003).
    pub fn verify_files(dir: &Path) -> xtriever_core::Result<()>;
}

// ── loading ─────────────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadPath {
    /// `std::fs::read` into memory (default, no `unsafe`).
    Buffered,
    /// Read-only memory map (feature `mmap`, ADR-0007).
    #[cfg(feature = "mmap")]
    Mmap,
}

// ── the embedder ────────────────────────────────────────────────────────────────────────
pub struct MiniLmEmbedder { /* private */ }

impl MiniLmEmbedder {
    /// Verify (FR-003), assert (FR-002) and build. `dir` holds the three pinned files.
    pub fn load(dir: &Path, load_path: LoadPath) -> xtriever_core::Result<Self>;
    /// The path this instance was loaded through (for observations).
    pub fn load_path(&self) -> LoadPath;
    /// `candle`'s effective thread count (`RAYON_NUM_THREADS`, else CPU count), for the record.
    pub fn thread_count() -> usize;
}

impl xtriever_core::Embedder for MiniLmEmbedder { /* all five methods */ }

// ── the vector index ────────────────────────────────────────────────────────────────────
pub struct FlatIndex { /* private */ }

impl FlatIndex {
    /// Create at `dir` (created if absent; must be empty). Writes an empty generation.
    pub fn create(dir: &Path, dim: usize, metric: Metric, fingerprint: &str) -> Result<Self>;
    /// Open, reading the current generation into memory.
    pub fn open(dir: &Path) -> Result<Self>;
    /// Open, mapping the current generation (feature `mmap`).
    #[cfg(feature = "mmap")]
    pub fn open_mapped(dir: &Path) -> Result<Self>;
    /// `open` + fingerprint / dim / metric agreement with `embedder` (FR-015).
    pub fn open_for(dir: &Path, embedder: &dyn Embedder) -> Result<Self>;
    #[cfg(feature = "mmap")]
    pub fn open_mapped_for(dir: &Path, embedder: &dyn Embedder) -> Result<Self>;
    /// The committed vector under `id`, exactly as added (`None` if not a committed row).
    pub fn vector(&self, id: DocId) -> Option<Vec<f32>>;
    /// The directory this index lives in.
    pub fn dir(&self) -> &Path;
}

impl xtriever_core::VectorIndex for FlatIndex { /* all eight methods */ }

/// On-disk format version this build reads and writes.
pub const FORMAT_VERSION: u32 = 1;
```

Two types, one enum, one module of constants, two trait impls. No builder, no trait of its own,
no async, no threads.

## `Embedder` method semantics

| method | behaviour | errors |
|---|---|---|
| `dim()` | 384 | — |
| `metric()` | `Metric::Cosine` | — |
| `fingerprint()` | `model::FINGERPRINT` (research D6) | — |
| `max_input_tokens()` | `Some(256)` | — |
| `embed(texts, kind)` | for each text in order: tokenize (truncate 256, pad to 256), forward with batch 1, mask-weighted mean, L2 normalise → `Vec<f32>` of 384. `kind` ignored (FR-007). Empty slice ⇒ empty `Vec`. Deterministic per research D2/D3 | `Model` (tokenizer or forward failure) |

**Determinism promise**: same fingerprint + same architecture ⇒ bit-identical vectors, regardless
of batch composition, order, size, and (subject to the D3 test) thread count. Across architectures:
within the golden tolerance, not bit-identical (research D3).

## `VectorIndex` method semantics

| method | behaviour | errors |
|---|---|---|
| `dim()` / `metric()` / `fingerprint()` | from the header | — |
| `add(id, v)` | validate width, finiteness, (Cosine) non-zero norm; stage `Some(v)` under `id`, replacing any pending entry | `DimensionMismatch`, `Schema` |
| `delete(ids)` | stage `None` per id; unknown ids are a no-op | — |
| `commit()` | merge committed ⊕ pending in ascending id order into a new `index.bin` via `.tmp` + `rename`; reload; clear pending. No-op if nothing pending | `Io`, `Corrupt` |
| `search(q, allowed, k)` | validate `q`; `k == 0` or empty `allowed` ⇒ `Ok(vec![])`; score every live row (∩ `allowed`) in `f64`, round to `f32`; sort by `(score DESC, id ASC)`; truncate to `k` | `DimensionMismatch`, `InvalidQuery` |
| `len()` | committed row count | — |

**The tie-break** is the implementation's, as ADR-0005 requires of `VectorIndex` implementations,
and holds at the `k`-th rank because the sort is total over `(score, id)` (FR-011).

## Error mapping (no new core variants)

| situation | variant |
|---|---|
| pinned file missing / wrong size / wrong hash; config or header assertion fails; tokenizer or model build fails; forward pass fails | `Model { model: "all-MiniLM-L6-v2", message }` |
| vector or query width | `DimensionMismatch { expected, actual }` |
| non-finite or zero-norm vector offered to `add` | `Schema(msg)` |
| non-finite or zero-norm query | `InvalidQuery(msg)` |
| bad magic, unknown format version, inconsistent length, unordered ids, `dim`/`metric` disagreement with the embedder | `Corrupt(msg)` |
| fingerprint disagreement | `FingerprintMismatch { index, current }` |
| filesystem | `Io` |

## Cargo features

| feature | default | adds | effect |
|---|---|---|---|
| (none) | yes | — | buffered loading for weights and index; zero `unsafe` compiled |
| `mmap` | no | `memmap2` | `LoadPath::Mmap`, `FlatIndex::open_mapped*`; compiles the one `unsafe` block in `bytes::map_readonly` (ADR-0007) |

## Harness extension (`xtriever-eval`, additive)

```rust
pub mod run {
    pub struct PassageSpec { pub title_then_text: bool, pub separator: String, pub omit_empty_title: bool }
    pub struct DenseConfig { pub name: String, pub passage: PassageSpec, pub k: usize }
    impl DenseConfig { pub fn validate(&self) -> Result<()>; pub fn dense_baseline_v1() -> Self; }
    pub fn build_passages(dataset: &Dataset, cfg: &DenseConfig) -> Result<(Vec<String>, IdMap)>;
    pub fn execute_dense(embedder: &dyn Embedder, index: &dyn VectorIndex, ids: &IdMap,
                         dataset: &Dataset, cfg: &DenseConfig) -> Result<Run>;
    pub struct EmbeddingCacheKey { pub format_version: u32, pub config: String, pub dataset: String,
                                   pub embedder_fingerprint: String, pub corpus_sha256: String, pub documents: u64 }
    impl EmbeddingCacheKey { pub fn write(&self, dir: &Path) -> Result<()>; pub fn matches(&self, dir: &Path) -> bool; }
}
pub mod report {
    pub struct StageInfo { pub kind: String, pub embedder_fingerprint: String, pub load_path: String,
                           pub thread_count: usize, pub baseline: String }
    // EvalReport { …, observations: Option<Observations>, stage: Option<StageInfo> }
    // Observations { index_dir_bytes, peak_rss_bytes, method,
    //                embed_corpus_ms?, search_ms?, model_bytes_buffered?, model_bytes_mmapped? }
    // delta(before, after) -> Result<Delta>   (was Delta; the 003 API is not stable — spec Assumptions)
    //   Err(Run("…different configurations…")) when any paired before.config != after.config (FR-021)
}
```

### `beir` example commands (added)

| command | does | exit |
|---|---|---|
| `run --dataset D --config dense-baseline-v1 --model-dir M --cache-dir C [--load-path buffered\|mmap] [--out F] [--export-run F]` | verify model; load; embed corpus into `C/D/` unless the cache key matches; embed + search every judged query; score; write | 0 / 1 |
| `model-memory --model-dir M --load-path P` | load the model, embed one sentence, print the fingerprint and exit — run under `/usr/bin/time -l` per path (research D12) | 0 / 1 |
| `export-vectors --cache-dir C --dataset D --sample N --out F` | write `{"doc_id","text","vector"}` per line for `N` evenly spaced cached rows, for `gen_004_fixtures.py --verify-embed` (research D14) | 0 / 1 |
| `delta`, `smoke`, `verify` | unchanged; `delta` now refuses mixed configurations | as 003 |

### Report file

The 003 key order is unchanged; **one optional trailing key `stage`** (after `observations`) and
four optional keys inside `observations`. A report without them serialises exactly as before.

### Baseline location

`specs/004-dense-stage/baselines/dense-baseline-v1.{scifact,nfcorpus,fiqa}.json`.

## Scripts

- `scripts/fetch-model.sh` — curl the three pinned files at the pinned revision into
  `reference/models/all-MiniLM-L6-v2/`, verify size + SHA-256 against
  `reference/models/manifest.json`; idempotent; download failure and hash failure are distinct
  messages (the `fetch-beir.sh` discipline).
- `reference/gen_004_fixtures.py` — goldens (`embeddings.json`, `search.json`, `mutations.json`,
  `manifest.json`); `--verify-embed vectors.jsonl` re-embeds with torch and reports the worst
  cosine / max-abs; `--refresh-manifest`.
- `scripts/check-no-stubs.sh` — extended to `crates/xtriever-dense/src/`.
