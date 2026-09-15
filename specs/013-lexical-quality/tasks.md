# Tasks: Lexical Quality — One Field for BM25

**Input**: Design documents from `/specs/013-lexical-quality/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/eval-configs.md](./contracts/eval-configs.md),
[quickstart.md](./quickstart.md); the BEIR sets, the 004 vector cache (`target/xt-dense-cache/`),
the re-rank model (`reference/models/ms-marco-MiniLM-L-6-v2`), `reference/.venv-003` (the scorer).

**Tests**: **Mandatory** (Principle II; spec FR-007). Phase 2 commits the v2 tests not
compiling (`Source::TitleAndText`, `lexical_baseline_v2`, … do not exist); Phase 3 turns them
green; Phases 4–5 are runs and records. One PR; commits: **C1** = Phases 1–2 (red), **C2** =
Phase 3, **C3** = Phases 4–6.

**Organization**: Setup → Red → US1 (the variant, the configurations, the lexical baselines)
→ US2 (hybrid and re-rank baselines, CI smoke) → US3 (docs, report) → Polish (gate, PR).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US3 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Design facts are research D-numbers, data-model rows and
  the contract. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`crates/xtriever-eval/{src/run.rs, src/lib.rs, examples/beir.rs, tests/run.rs, tests/hybrid_run.rs}`;
baselines `specs/013-lexical-quality/baselines/`; runs exported to `target/xt-*-run.*.jsonl`
(git-ignored); indexes `target/xt-rerank-index-v2/<d>/`.

---

## Phase 1: Setup

- [X] T001 Confirm the inputs are present and the v1 baselines reproduce before anything changes: `cargo run --release -p xtriever-eval --example beir -- run --dataset scifact --config lexical-baseline-v1 --out /tmp/lex1-before.json` and `diff <(jq '.mean_ndcg_10,.mean_recall_100' /tmp/lex1-before.json) <(jq '.mean_ndcg_10,.mean_recall_100' specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json)` empty; `ls target/xt-dense-cache/{scifact,nfcorpus,fiqa}/index.bin reference/models/ms-marco-MiniLM-L-6-v2/model.safetensors`; `mkdir -p specs/013-lexical-quality/baselines`

---

## Phase 2: Foundational — the red tests

**Purpose**: The v2 tests, not compiling. **⛔ Commit C1 at the end of this phase.**

- [X] T002 [P] Add to `crates/xtriever-eval/tests/run.rs`: `v2_config_is_v1_with_one_joined_field` — `EvalConfig::lexical_baseline_v2()` has `name "lexical-baseline-v2"`, exactly one field `FieldSpec { name: "contents", from: Source::TitleAndText, analyzer: "standard_en", boost: 1.0 }`, and `k`, `omit_empty_fields`, `query` equal to `lexical_baseline_v1()`'s; `v2_hybrid_and_rerank_wrap_the_v2_lexical` — `HybridConfig::hybrid_baseline_v2()` equals `hybrid_baseline_v1()` except `name` and `lexical == lexical_baseline_v2()`; `RerankConfig::hybrid_rerank_v2()` likewise against v1 (compare every other field explicitly); `v1_is_unchanged` — `lexical_baseline_v1()` still has `title` 2.0 / `text` 1.0 (the existing `baseline_config_is_as_specified` covers it; add a `hybrid_baseline_v1` field assertion)
- [X] T003 [P] Add to `crates/xtriever-eval/tests/hybrid_run.rs` (which has the `mini()` dataset with an empty-title document): `title_and_text_joins_with_one_space_and_omits_empty_sides` — `build_external(&ds, &EvalConfig::lexical_baseline_v2())`: document `d1` (title and text) → `contents == "<title> <text>"`; `d2` (empty title) → `contents == "<text>"` with no leading space; a document with an empty text and a title (add one to `mini()` if absent, or construct a `Dataset` in the test) → `contents == "<title>"`; both empty → no `contents` field under `omit_empty_fields`; and `contents` equals the dense passage `dense_baseline_v1`'s builder produces for the same document (call the passage builder used by `dense_run`; if it is private, assert against the literal expected strings and note it)
- [X] T004 Run `cargo nextest run -p xtriever-eval -E 'test(v2) | test(title_and_text)'` → does not compile (the red state); record in `specs/013-lexical-quality/report.md` ("Red checkpoint"). **⛔ Commit C1**: the two test files, the report stub

---

## Phase 3: User Story 1 — The lexical baseline indexes one joined field (Priority: P1)

**Goal**: `Source::TitleAndText`, the three v2 constructors, the example's dispatch and
dense-field derivation; the v2 lexical baselines with their deltas.

**Independent Test**: quickstart Step 2's lexical loop — three verified baselines, SC-001.

- [X] T005 [US1] In `crates/xtriever-eval/src/run.rs`: add `Source::TitleAndText` (doc: "`title + \" \" + text`; an empty title contributes nothing and no separator, an empty text likewise — the shape BEIR's reference BM25 indexes as `contents` and the dense passage already uses"); extend `document_fields` (`:449–461`) so `TitleAndText` builds that string (an owned `String`; `omit_empty_fields` skips it when both are empty); add `EvalConfig::lexical_baseline_v2()` (data-model row), `HybridConfig::hybrid_baseline_v2()` (`name "hybrid-baseline-v2"`, `lexical: lexical_baseline_v2()`, the rest as v1), `RerankConfig::hybrid_rerank_v2()` (v1 over the hybrid v2, `name "hybrid-rerank-v2"`) — each with a doc comment citing research D1's numbers; `cargo nextest run -p xtriever-eval` green (T002/T003 included)
- [X] T006 [US1] In `crates/xtriever-eval/examples/beir.rs`: dispatch `"lexical-baseline-v2"`, `"hybrid-baseline-v2"`, `"hybrid-rerank-v2"` (`:110–113`; extend the "known:" error text); replace the hard-coded `dense_fields = ["title","text"]` (`:487–491`) with the lexical configuration's field names in order (`cfg.lexical.fields.iter().map(|f| FieldName::from(f.name.as_str()))`) — for v1 that is `["title","text"]` as before; update the usage doc comment (`:10–13`) with the v2 names; `cargo clippy -p xtriever-eval --all-targets -- -D warnings` clean
- [X] T007 [US1] Run the v2 lexical baselines (quickstart Step 2, first loop) for scifact, nfcorpus, fiqa with `--export-run`, verify each with `reference/.venv-003/bin/python reference/gen_003_fixtures.py --verify-run … --report …` (PASS), and `beir compare` each against `specs/003-eval-harness/baselines/lexical-baseline-v1.<d>.json`; paste the three deltas into the report. **⛔** SciFact < v1 + 0.05, NFCorpus < v1 + 0.008, or FiQA outside ±0.001 nDCG@10 is stop-and-report (SC-001). **⛔ Commit C2** (Phase 3 code + the three lexical baselines)

---

## Phase 4: User Story 2 — Everything downstream is re-baselined (Priority: P1)

**Goal**: hybrid and re-rank v2 baselines; the CI smoke on v2.

**Independent Test**: quickstart Step 2's second loop; SC-002; the smoke green on push.

- [X] T008 [US2] Run `hybrid-baseline-v2` on the three sets with `--cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index-v2/<d>` and `--export-run`; verify each run with the 003 scorer; `beir compare` against `specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.<d>.json`; paste the deltas. Also confirm the dense list is unchanged: `beir run --dataset scifact --config dense-baseline-v1 --cache-dir target/xt-dense-cache --out /tmp/dense-check.json` equals the 004 baseline (the cache is by value; a sanity check that T006's `dense_fields` change altered nothing dense)
- [ ] T009 [US2] Run `hybrid-rerank-v2` on the three sets (`--rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2`, `RAYON_NUM_THREADS=4`, the same `--index-dir`), verify, `beir compare` against `specs/006-rerank-stage/baselines/hybrid-rerank-v1.<d>.json`; paste the deltas. **⛔** A fused (hybrid or re-rank) v2 mean nDCG@10 across the three sets below its v1 mean is stop-and-report (SC-002); a per-dataset drop is a finding, stated
- [X] T010 [US2] In `.github/workflows/ci.yml`, the `eval-smoke` step: `beir smoke --dataset scifact --config lexical-baseline-v2 --baseline specs/013-lexical-quality/baselines/lexical-baseline-v2.scifact.json` (confirm `smoke` honours `--config` through `evaluate` — `examples/beir.rs:786–790`; if it does not, make it), with a comment naming this feature and that v1 remains the example's default; add `specs/013-lexical-quality/baselines/**` to the job's path filter; run the same command locally → `eval-smoke: PASS`

---

## Phase 5: User Story 3 — The measurements are on record (Priority: P2)

- [X] T011 [P] [US3] Docs: `python/README.md` (the schema example — one `contents` text field for BM25, the `title`/`text` split kept only if a caller needs them separately, with research D1's +6.0 / +1.1 in one sentence); `crates/xtriever-cli/src/wiki/chunking.rs` schema comment (the Wikipedia index keeps `title` 2.0 / `text` until rebuilt; 014 decides); `crates/xtriever-eval/src/lib.rs` crate docs (the v2 configurations and why); `specs/012-sparse-spike/report.md` F-002 → "resolved by 013" with the measured engine deltas
- [ ] T012 [US3] Write `specs/013-lexical-quality/report.md`: verdict; the attribution table (research D1, verbatim); the baselines table (v1 vs v2 for lexical / hybrid / rerank × 3 datasets, nDCG@10 and Recall@100, deltas); SC-001–SC-006 with numbers; findings (anything the engine's numbers showed that the spike's did not — e.g. the tokenizer's share; per-dataset fused drops if any); "Deliberately not done" (parameters, stop words, extra title field — with their numbers; Wikipedia rebuild; fixture goldens)

---

## Phase 6: Polish

- [ ] T013 Gate (quickstart Step 4): fmt; clippy host + `x86_64-pc-windows-msvc`; `cargo nextest run --workspace`; deny; iOS / iOS-sim / Android checks; no-stubs; `git diff --stat main -- crates/ | grep -v xtriever-eval` empty; `git diff --stat main -- specs/003-eval-harness specs/004-dense-stage specs/005-hybrid-pipeline specs/006-rerank-stage` empty (SC-004); v1 reproduction (quickstart Step 3); no identifiers in the tree
- [ ] T014 Write `specs/013-lexical-quality/pr-description.md` (for the reviewer: the one change, the attribution in three lines, the baselines table with deltas, what stays v1, the CI smoke move, the attribution lines). **⛔ Commit C3**; the owner pushes (the smoke runs on v2) and merges

---

## Dependencies & Execution Order

T001 → (T002 ‖ T003) → T004 (C1) → T005 → T006 → T007 (C2) → T008 → T009 → T010 →
(T011 ‖ T012) → T013 → T014 (C3). The runs (T007–T009) are the wall time: ~5 min lexical,
~10 min hybrid, ~50 min re-rank on the three sets.

### User story completion order

US1 → US2 → US3.

### Parallel opportunities

T002 ‖ T003 (two test files); T011 ‖ T012; T008's runs can start while T010 is edited.

## Implementation Strategy

**MVP** = Phases 1–3: the joined field and the three lexical baselines — the six points.
**Rule 6 stop-points**: T004 (red), T007 (SC-001 floors), T009 (SC-002), T013 (any gate
failure or a v1 baseline that no longer reproduces).
