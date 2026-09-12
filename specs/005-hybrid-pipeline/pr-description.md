# PR text for Feature 005 — the hybrid pipeline

Hand-written line counts (goldens and baselines are generated data): `crates/xtriever-pipeline/src`
~1,260 · `crates/xtriever-pipeline/tests` ~1,690 · `xtriever-eval` (run/report/example/tests)
~425 net · Python ~340 · docs ~250. Suggested split (plan.md Rule 3):

| PR | contents | lines |
|---|---|---|
| 1 | `gen_005_fixtures.py` + goldens, `Cargo.toml` deps, scaffold, **all tests (red)**, eval scaffolds | ~2,100 (tests 1,690) |
| 2 | `error.rs`, `descriptor.rs`, `ids.rs`, `index.rs` — US1 green | ~700 |
| 3 | `fusion.rs`, `search.rs` — US2–US4 green under both feature sets | ~400 |
| 4 | eval `run.rs` / `report.rs` / `beir.rs`, three baselines + verification, observations, report | ~600 |

---

## Title

`feat(pipeline): hybrid index — external ids, RRF fusion, graceful degradation, explain, and the first delta-guarded BEIR baseline (Feature 005)`

## Body

Implements `xtriever-pipeline`: `HybridIndex` composes the 002 lexical stage and the 004 dense
stage under one directory with a versioned descriptor and a persisted external-id ↔ `DocId` map
(ids never reused; both stages see only `DocId`s). Commit order is lexical → dense → id map →
descriptor, and `open` refuses any partial state by a four-count check. Search resolves a filter
once for both stages, fuses by reciprocal rank fusion (`k = 60`, `f64`, `(score DESC, DocId
ASC)`), degrades to the lexical list when the dense stage fails or a caller-supplied elapsed-time
source says the budget is spent (strict mode errors instead), and explains every hit under the
core's feature names. The crate reads no clock and contains no `unsafe`. The 003 harness gains a
stage-agnostic closure runner, `hybrid-baseline-v1`, a cross-configuration `compare`, and reuses
the 004 embedding cache by value (0 documents embedded).

Spec: `specs/005-hybrid-pipeline/spec.md` · Plan: `plan.md` · Report: `report.md`

**Tests**: 37 / 37 offline (`cargo nextest run -p xtriever-pipeline`), 38 / 38 under
`--features mmap`, 1 / 1 model-backed; `xtriever-eval` 37 / 37; workspace 188 / 188. Committed red
first (PR 1: 31 pipeline tests, 2 pass, 29 fail on the scaffold; 4 eval tests red on scaffolds).

**Oracles**: Python RRF over hand-made lists (10 cases, 1e-9 — bit-identical by construction);
`--verify-fusion` recomputes RRF from the exported per-query stage lists of every real run
(1,271 / 1,271 queries agree); the 004 `fsum` cosine oracle asserted through the pipeline's
explanations; `--verify-run` (`pytrec_eval`) PASS on all three baselines.

**The baseline — `hybrid-baseline-v1`** and the **first delta section** (against both stages):

| dataset | lexical | dense | **hybrid** nDCG@10 | vs better stage | Recall@100 |
|---|---|---|---|---|---|
| SciFact | 0.627044 | 0.645082 | **0.689727** | +0.044645 (+6.9 %) | 0.941667 |
| NFCorpus | 0.311523 | 0.316673 | **0.345008** | +0.028335 (+8.9 %) | 0.320720 |
| FiQA-2018 | 0.250238 | 0.368671 | **0.369210** | +0.000538 (+0.15 %) | 0.707111 |

**SC-011 PASS 3 / 3** (fused ≥ better stage on ≥ 2 of 3). From this feature on the fused
number is what the regression rule guards. `beir delta` still refuses mixed configurations;
`beir compare` produces the table above without an ADR trigger.

**Observations (FiQA, M1 Pro, 4 threads)**: ingest from cache 5.2 s; hybrid directory 102.9 MiB
(`ids.json` 508 KB); 113 ms per query end to end of which ~95 ms is query embedding;
`Filter::Ids` over half of SciFact adds ~1.7 ms; harness peak RSS 776 MB (holds two copies of
the dense vectors — the harness's number, not the pipeline's).

**Spec corrections made at implementation** (report F-001, F-003): the filter clause "equals the
unrestricted result filtered to the set" was false under rank fusion and replaced by the
intended contract (fusion of both stages restricted before ranking); `search_lexical` gained the
`dense_text` argument the dense stage needs.

**Gate**: fmt · clippy `-D warnings` (default and `mmap`) · nextest · deny · iOS / iOS-sim /
Android · no stubs · toolchain · zero `unsafe` and zero `Instant`/`SystemTime`/`std::thread` in
the pipeline · eval library graph free of candle/tantivy/memmap2/pipeline · `xtriever-core`,
`-lexical`, `-dense`, `deny.toml`, eval `metrics.rs`/`dataset.rs` unchanged. No ADR, no new
dependency. CI stays the SciFact lexical smoke only (standing rule).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
