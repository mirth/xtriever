# PR text for Feature 003 — the evaluation harness

Hand-written line counts (baseline JSONs and goldens are generated data): `src/` ~1,060 ·
`tests/` ~1,100 · `examples/beir.rs` ~240 · Python + script ~320 · CI/docs ~120. Suggested split:

| PR | contents | lines |
|---|---|---|
| 1 | `requirements-003`, `gen_003_fixtures.py` + goldens, `beir-manifest.json`, `fetch-beir.sh`, scaffold, **all tests (red)** | ~1,500 (tests 1,100) |
| 2 | `error.rs`, `dataset.rs`, `metrics.rs` — US1 + US2 green | ~550 |
| 3 | `run.rs`, `report.rs`, `examples/beir.rs`, three baselines + `--verify-run` cross-check — US3 + US4 green | ~600 |
| 4 | `eval-smoke` CI job, scaffold removal, docs, report | ~150 |

---

## Title

`feat(eval): BEIR evaluation harness with pinned datasets, pytrec_eval-verified metrics, and the lexical baseline (Feature 003)`

## Body

Implements `xtriever-eval`: hash-verified loading of BEIR SciFact / NFCorpus / FiQA-2018, nDCG@10
and Recall@100 under exactly the conventions BEIR's `pytrec_eval` wrapper uses, a runner for any
`LexicalIndex`, committed JSON reports, deltas, and a blocking SciFact smoke in CI.

Spec: `specs/003-eval-harness/spec.md` · Plan: `plan.md` · Report: `report.md`

**Tests**: 25 / 25 offline (`cargo nextest run -p xtriever-eval`), 4 / 4 dataset-backed
(`--run-ignored only`), workspace 98 / 98. Committed red first (PR 1: 25 tests, 3 pass, 22 fail on
the scaffold, 0 fixture-caused).

**Oracle**: `pytrec_eval 0.5` behind BEIR's `evaluate()` semantics (identical-id pop, mean over
returned keys, 5-decimal rounding), pinned in `reference/requirements-003.txt`; 12 golden cases
agree within 1e-6 (measured ≤ 2.2e-16); every real baseline cross-checked by `--verify-run`.

**The baseline — `lexical-baseline-v1`** (title + text under `standard_en`, title boost 2.0,
`Match(None, q)`, k = 100), lexical stage at `94ddbe6`:

| dataset | nDCG@10 | Recall@100 | published BM25 (BEIR paper) | band ±0.10 |
|---|---|---|---|---|
| SciFact | 0.627044 | 0.887556 | 0.665 / 0.908 | inside |
| NFCorpus | 0.311523 | 0.247820 | 0.325 / 0.250 | inside |
| FiQA-2018 | 0.250238 | 0.551775 | 0.236 / 0.539 | inside |

SciFact end to end: 2.0 s. FiQA observations (FR-018): index dir 18,370,560 bytes for 57,638 docs;
harness-process peak RSS 343,441,408 bytes (`/usr/bin/time -l`) — recorded, not budgeted.

**eval delta: this feature establishes the baseline (ADR-0006 condition 2 discharged).**

**Unchanged**: `xtriever-core`, `xtriever-lexical`, `deny.toml`. Library graph is `std`-only
(`cargo tree -p xtriever-eval -e normal` has no `tantivy`); the example binary reaches
`TantivyIndex` through a dev-dependency.

**Gate**: fmt ✓ · clippy `-D warnings` ✓ · nextest ✓ · deny ✓ · iOS / iOS-sim / Android ✓ ·
no stubs ✓. New CI job `eval-smoke` (blocking, path-filtered, cached SciFact) — first run URLs to
be recorded in `report.md` after this push (T042).

Findings (report.md): F-001 identical-id rule moved to scoring time; F-002 a dedupe-blind test;
F-003 `fetch-beir.sh` self-heals extracted files (the Rust loader is the hard gate).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
