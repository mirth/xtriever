# PR text for Feature 004 — the dense stage

Hand-written line counts (goldens, baselines and the lockfile are generated data):
`crates/xtriever-dense/src` ~1,330 · `crates/xtriever-dense/tests` ~1,670 · `xtriever-eval`
(run/report/example/tests) ~800 net · Python + script ~600 · docs/ADR/constitution ~250.
Suggested split (plan.md Rule 3):

| PR | contents | lines |
|---|---|---|
| 1 | constitution v1.2.0 + ADR-0007, `reference/models/manifest.json`, `fetch-model.sh`, `requirements-004`, `gen_004_fixtures.py` + goldens, `Cargo.toml` deps, scaffold, **all tests (red)** | ~2,500 (tests 1,670) |
| 2 | `error.rs`, `model.rs`, `bytes.rs`, `embedder.rs` — US1 green, thread-count and load-path parity | ~600 |
| 3 | `index/{format,search,mod}.rs` — US2 green under both feature sets | ~700 |
| 4 | eval `run.rs` / `report.rs` / `beir.rs` extension, three baselines + `--verify-run`, observations, report | ~900 |

---

## Title

`feat(dense): candle embedder and flat exact vector index for all-MiniLM-L6-v2, with the dense BEIR baseline (Feature 004)`

## Body

Implements `xtriever-dense`: `MiniLmEmbedder` (`Embedder`) for the pinned
`sentence-transformers/all-MiniLM-L6-v2` — three size-and-SHA-256-verified files, shape asserted
from the files, one text per forward pass at a fixed 256 tokens so vectors are bit-identical
whatever batch or thread count — and `FlatIndex` (`VectorIndex`): a flat, exact, versioned
`index.bin` per generation replaced by `rename`, `f64`-accumulated scores rounded once, total
`(score DESC, DocId ASC)` order including at the `k`-th rank. Both artifacts load buffered by
default and memory-mapped behind the non-default `mmap` feature — the crate's one `unsafe` block,
admitted by **constitution v1.2.0 / ADR-0007** and tested bit-for-bit against the safe path. The
Feature 003 harness gains `dense-baseline-v1`, an embedding cache keyed by fingerprint + corpus
hash, and one optional trailing `stage` key in the report.

Spec: `specs/004-dense-stage/spec.md` · Plan: `plan.md` · Report: `report.md`

**Tests**: 33 / 33 offline + 16 / 16 model-backed (`cargo nextest run -p xtriever-dense`,
`--run-ignored only`), 37 / 37 + 17 / 17 under `--features mmap`; `xtriever-eval` 32 / 32 + 4 / 4;
workspace 142 / 142. Committed red first (PR 1: 27 offline tests, 4 pass, 23 fail on the scaffold,
0 fixture-caused; 16 model-backed all `NotImplemented`).

**Oracles**: torch at Feature 001's exact pins (13 embedding cases incl. over-length, empty,
whitespace, OOV/Unicode; cosine ≥ 0.9999, max-abs ≤ 1e-3, token ids checked first); NumPy
`float64` with `fsum` for exact search (4 sets × 3 queries × every `k`/`allowed` case, 8 designed
ties at rank `k`, scores within 1e-6); 50 real SciFact rows re-embedded by torch: worst max-abs
2.16e-07.

**Determinism (Principle VI)**: three batch arrangements, `Query` vs `Passage`, two loads,
buffered vs mapped, and `RAYON_NUM_THREADS=1` vs `4` in separate processes — all bit-identical.

**The baseline — `dense-baseline-v1`** (`title + " " + text`, 256-token truncation, k = 100,
cosine), all three datasets, each `--verify-run` PASS:

| dataset | nDCG@10 | Recall@100 |
|---|---|---|
| SciFact | 0.645082 | 0.925000 |
| NFCorpus | 0.316673 | 0.311450 |
| FiQA-2018 | 0.368671 | 0.706057 |

**eval delta: not applicable — this feature establishes the dense stage's absolute baseline
(FR-021); `beir delta` refuses to pair it with the lexical baseline; lexical smoke unchanged.**

**Observations (FR-023, FiQA, M1 Pro, 4 threads)**: `index.bin` 88,993,414 B; peak RSS of the
whole run 419,217,408 B; corpus embedded in 5,433 s (94 ms/passage); 648 queries in 73.8 s
(~114 ms each, almost all query embedding); model memory from cold, median of 3 fresh processes:
buffered 226,328,576 B, mapped 197,394,432 B (−13 % ⇒ ADR-0007 condition 5 keeps the mmap weight
path). Extrapolation to 100k rows, labelled as such: ~147 MiB index, ~2.6 h to embed once.

**Corrections to the record**: Feature 001's "39.5× from mmap" was an ordering artifact — candle
copies tensors to the heap on both paths (research D1; a dated correction is in the 001 report).
The 003 baselines did not round-trip through `serde_json` without `float_roundtrip` (1-ulp parse
drift, a latent smoke risk) — enabled; one hand-typed escape in the FiQA lexical file normalised.

**Gate**: fmt · clippy `-D warnings` (default and `mmap`) · nextest · deny · iOS / iOS-sim /
Android (+ `mmap` on iOS) · no stubs · toolchain · one `unsafe {}` in `bytes.rs` under one
item-scoped allow · eval library graph free of candle/tantivy/memmap2 · `xtriever-core`,
`xtriever-lexical`, `deny.toml`, eval `metrics.rs`/`dataset.rs` unchanged. wasm32 best-effort still
fails (now at `getrandom` via candle, previously `errno` via tantivy).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
