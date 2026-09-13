# PR text for Feature 006 — the re-rank stage

Hand-written line counts (goldens and baselines are generated data): `crates/xtriever-rerank/src`
~660 · `crates/xtriever-rerank/tests` ~760 · `crates/xtriever-pipeline/src` ~680 net ·
`crates/xtriever-pipeline/tests` ~1,030 · `xtriever-eval` (run/report/example/tests) ~450 net ·
Python ~450 · docs/governance ~210 (constitution, two ADRs, CLAUDE.md, plan template, scripts).
Suggested split (plan.md Rule 3):

| PR | contents | lines |
|---|---|---|
| 0 | **Governance, separately**: constitution v1.3.0, `CLAUDE.md`, plan template, ADR-0009, the ADR-0007 note (+ ADR-0008) | ~250 |
| 1 | `manifest-rerank.json` + `.gitignore`, `fetch-model.sh --manifest`, `gen_006_fixtures.py` + goldens, `Cargo.toml` deps, both scaffolds, **all tests (red)**, eval scaffolds, stubs-script and CI-filter checks | ~2,300 (tests ~1,800) |
| 2 | `xtriever-rerank`: `model.rs`, `bytes.rs`, `scorer.rs`, `budget.rs` — US1, US2 green | ~600 |
| 3 | `xtriever-pipeline`: `passages.rs`, `descriptor.rs`, `index.rs`, `rerank.rs`, `search.rs`, `types.rs` — format v2, US3, US4 green | ~700 |
| 4 | eval `run.rs` / `report.rs` / `beir.rs`, three baselines + verification, observations, report | ~700 |

---

## Title

`feat(rerank): cross-encoder re-rank stage on candle, passage text store (pipeline format v2), budgeted partial re-ranking, and the first delta against the guarded fused baseline (Feature 006)`

## Body

Implements `xtriever-core`'s `Reranker` in `xtriever-rerank` with the pinned
`cross-encoder/ms-marco-MiniLM-L-6-v2` (revision `233902d2…`, weights `sha256:821d1aa6…`,
size- and hash-verified before parsing): candle-transformers' `BertModel` plus the classification
head candle 0.9.2 does not ship (CLS → pooler → tanh → linear, two `candle_nn::Linear` layers).
Each query–passage pair is scored alone at its own length under `longest_first` truncation at
512, so scores are bit-identical across call composition, order and thread count; `rerank`
scores in input order and returns "not scored" past the item or time limit. The pipeline gains
`passages.bin` (every hybrid index stores its passage text, read per hit — **format version 2**,
ADR-0008), re-scores the first 20 fused candidates under the *remaining* budget after a third
check point, orders scored hits first then every unscored hit in fused order, degrades per stage
(strict mode errors), and explains `rerank.score` / `rerank.rank`. The harness gains
`hybrid-rerank-v1` and the first delta against a guarded number.

Spec: `specs/006-rerank-stage/spec.md` · Plan: `plan.md` · Report: `report.md`

**Governance**: constitution **v1.3.0** — Principle VII now names `xtriever-rerank` beside
`xtriever-dense` for the one read-only-mapping `unsafe` block behind a non-default feature
(**ADR-0009**, owner's decision over the buffered-only alternative); pipeline on-disk format
**v2** with no migration (**ADR-0008**). Exactly one `unsafe` in the new crate (`bytes.rs`,
`mmap` feature, tested bit-for-bit against the buffered path); zero in the pipeline.

**Tests**: `xtriever-rerank` 9 / 9 offline, 14 / 14 model-backed (15 / 15 with `mmap`);
`xtriever-pipeline` 60 / 60 offline, 61 / 61 with `mmap`, 2 / 2 model-backed (both models);
`xtriever-eval` 43 / 43; workspace 226 / 226. Committed red first (PR 1: rerank 4 pass / 5 fail
offline + 13 model-backed fail on the scaffold; pipeline 40 pass / 19 fail; eval 41 / 1).

**Oracles**: the HF sequence-classification pipeline under the 004 torch/transformers pins —
37 pairs incl. over-length, empty-passage, empty-query, both-empty, near-tie; tokenization parity
37 / 37, max abs diff **7.5e-6** (tolerance 1e-3), every per-query order exact; the generator
refuses gaps below 10× tolerance. The ordering rule has its own Python oracle (10 cases) and a
real-data check: `--verify-rerank` 1,271 / 1,271 queries. `--verify-run` (`pytrec_eval`) PASS on
all three baselines; SciFact re-run byte-identical.

**The baseline — `hybrid-rerank-v1`** (depth 20) and **the delta against `hybrid-baseline-v1`**:

| dataset | `hybrid-baseline-v1` | **`hybrid-rerank-v1`** nDCG@10 | abs | rel | Recall@100 |
|---|---|---|---|---|---|
| SciFact | 0.689727 | **0.703862** | +0.014135 | **+2.05 %** | 0.941667 (=) |
| NFCorpus | 0.345008 | **0.360287** | +0.015279 | **+4.43 %** | 0.320720 (=) |
| FiQA-2018 | 0.369210 | **0.374214** | +0.005005 | **+1.36 %** | 0.707111 (=) |

**SC-008 PASS 3 / 3** (re-ranked ≥ fused on ≥ 2 of 3). Recall@100 byte-equal everywhere. From
this feature on the re-ranked number is what the regression rule guards.

**Observations (M1 Pro, 4 threads, release)**: **116–163 ms per pair**, 2.3–3.3 s per query at
depth 20 — the re-ranker is 94 % of a query and an order of magnitude above the 005 pipeline
(report F-001; the plan estimated 50–100 ms). FiQA: 12,960 pairs in 1,504 s; directory 145.5 MiB
of which `passages.bin` 42.7 MiB; harness peak RSS 999 MiB (two dense copies + two models — the
harness's number). Cross-encoder fresh-process load: buffered 251.8 MB, mapped 245.0 MB (2.7 %
saving vs 004's 13 % — recorded, F-003; ADR-0009 applies no deletion clause).

**Spec corrections made at plan time** (dated notes): FR-003 stayed at both load paths through
the v1.3.0 amendment rather than the buffered-only fallback; FR-010's store reads on demand.
At implementation: the explain-only second search no longer re-ranks (F-002); a 005 persist
test's expected version text moved to 7 (F-005).

**Gate**: fmt · clippy `-D warnings` (workspace, pipeline `mmap`, rerank `mmap`) · nextest
(offline, `mmap`, model-backed) · deny · iOS / iOS-sim / Android · no stubs · toolchain · one
`unsafe` block per model crate and none — nor any clock — in the pure crates (`scripts/check-containment.sh`, syntax-aware) ·
eval library graph free of candle/tantivy/memmap2/pipeline/rerank · pipeline graph free of
`xtriever-rerank` · `xtriever-core`, `-lexical`, `-dense`, `deny.toml`, eval
`metrics.rs`/`dataset.rs` unchanged. No new dependency. CI stays the SciFact lexical smoke only
(standing rule; the path filter already covered `crates/xtriever-rerank/**`).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
