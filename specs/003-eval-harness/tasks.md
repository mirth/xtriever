# Tasks: The Evaluation Harness

**Input**: Design documents from `/specs/003-eval-harness/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/eval-harness.md](./contracts/eval-harness.md),
[quickstart.md](./quickstart.md)

**Tests**: **Mandatory** (Principle II, spec FR-029). Phase 2 lands every test red; offline tests
fail at runtime on the `NotImplemented` scaffold, dataset-backed tests are `#[ignore]` and run
where the cache exists (FR-030). Story phases contain implementation only and end with the task that
turns their tests green.

**Organization**: Setup → Oracle & red suite → Foundational → US1 → US2 → US3 → US4 → Polish. The
four-PR split from plan.md is marked at the checkpoints.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Measured facts are cited as research D-numbers — read them
  before implementing; the hashes and counts there are the ones to assert, verbatim.

## Path Conventions

Crate `crates/xtriever-eval/` (`src/`, `tests/`, `examples/`); Python oracle under `reference/`;
fixtures `reference/fixtures/003/`; dataset manifest `reference/datasets/beir-manifest.json`;
cache `reference/datasets/beir/` (git-ignored); baselines `specs/003-eval-harness/baselines/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependencies, the Python oracle environment, the dataset pins, and the fetch script.

- [X] T001 Add dependencies to `crates/xtriever-eval/Cargo.toml` **with `cargo add`**: `cargo add -p xtriever-eval serde --features derive`, `cargo add -p xtriever-eval serde_json sha2`, `cargo add -p xtriever-eval --dev xtriever-lexical anyhow tempfile serde_json` (the `xtriever-lexical` path dependency is **dev only** — research D6); keep `xtriever-core` and `[lints] workspace = true`; set the crate `description`
- [X] T002 Verify the library graph is pure after T001: `cargo tree -p xtriever-eval -e normal --prefix none | grep -Ei '(-sys|^cc |onig|zstd|tantivy)'` prints nothing (dev-deps excluded by `-e normal`); `cargo deny check` passes with no `deny.toml` change
- [X] T003 [P] Create `reference/requirements-003.in` (`pytrec_eval==0.5`, `numpy==2.5.3` — research D9; **not** the 001/002 torch stack) and compile `reference/requirements-003.txt` with `uv pip compile --python-version 3.12 --generate-hashes`; run `./scripts/setup-reference-venv.sh 003` and confirm `reference/.venv-003/bin/python -c 'import pytrec_eval'` works
- [X] T004 [P] Write `reference/datasets/beir-manifest.json` per data-model `DatasetManifest` with the **measured** values from research D1 verbatim: per dataset the `url`, `archive { bytes, sha256 }`, `files` for exactly `corpus.jsonl`, `queries.jsonl`, `qrels/test.tsv` with `{ bytes, sha256, lines }`, and `counts { documents, queries, judged_queries, judgement_pairs }` = SciFact 5183/1109/300/339, NFCorpus 3633/3237/323/12334, FiQA 57638/6648/648/1706
- [X] T005 [P] Write `scripts/fetch-beir.sh [dataset…]` (default: all three): for each dataset read url/hashes from the manifest with `jq`, download the archive with `curl -sSL` into `reference/datasets/beir/` only if absent, verify archive `bytes` and `sha256` (`shasum -a 256`), `unzip -qo`, verify every manifest file's `bytes` and `sha256`; any mismatch prints the path and **both** hashes and exits 1 (FR-007); idempotent — a second run downloads nothing and re-verifies (Story 2 scenario 2)
- [X] T006 Run `./scripts/fetch-beir.sh` end to end from an empty cache and confirm it verifies all 12 hashes; confirm `git status` shows nothing under `reference/datasets/beir/` (`.gitignore` already covers it, FR-008)

---

## Phase 2: Oracle & Red Suite (Blocking Prerequisite)

**Purpose**: Metric goldens from the reference implementation, the crate scaffold, and every test —
red. This phase is **PR 1**.

**⚠️ CRITICAL**: No story implementation begins until T019 confirms the suite fails for want of an
implementation and not for want of a fixture.

### Python oracle and goldens

- [X] T007 Create `reference/gen_003_fixtures.py` scaffold: interpreter guard (3.12 + venv), `--seed`, `--out`, `write_json`, `sha256_file`, `manifest.json` emission; and a **convention probe** that runs the exact synthetic case from research D3 through `pytrec_eval.RelevanceEvaluator` and **refuses to emit** unless every row of the D3 table holds (linear gain = 0.8597186998521972 for the graded case; unjudged run query absent; judged-not-in-run absent; no-relevant ⇒ 0.0; empty results ⇒ 0.0; tie order `d9, d2, d1`) — quickstart Step 2
- [X] T008 In `reference/gen_003_fixtures.py`, emit `metrics.json` with the eleven cases named in data-model `MetricGoldens` (`graded`, `no_relevant`, `fewer_than_cutoff`, `empty_results`, `unjudged_query`, `not_retrieved`, `duplicate_ids`, `ties_at_cutoff`, `identical_ids`, `grade_zero`, `big_mean` with 50 seeded queries); each case carries `qrels`, `run` as **ordered id lists**, and the reference's per-query and mean `ndcg_10` / `recall_100`; feed `pytrec_eval` scores `k − rank` so it scores the given order (research D4); apply BEIR's wrapper semantics — pop identical ids, mean over returned keys — when computing the means (research D3)
- [X] T009 In `reference/gen_003_fixtures.py`, implement `--verify-run <run.jsonl> --qrels <test.tsv> --report <report.json>`: read the Rust-exported ordered run, score it with `pytrec_eval` under BEIR's wrapper semantics (identical-id pop, mean over `scores.keys()`, `round(…, 5)`), parse the CRLF-with-header qrels (research D2), and compare against the report's `mean_ndcg_10` / `mean_recall_100` within **1e-6** and the `beir_rounded` values exactly; exit 1 on disagreement
- [X] T010 Run `reference/.venv-003/bin/python reference/gen_003_fixtures.py --seed 3 --out reference/fixtures/003/` and commit `metrics.json` + `manifest.json`; confirm `git check-attr text eol -- reference/fixtures/003/metrics.json` reports `text: unset`

### Crate scaffold

- [X] T011 Create the scaffold in `crates/xtriever-eval/src/lib.rs` + `src/scaffold.rs`: the public surface from [contracts/eval-harness.md](./contracts/eval-harness.md) (`dataset::{Manifest, Dataset}`, `metrics::{ndcg_at, recall_at}`, `run::{EvalConfig, build, execute}`, `report::{score, delta, smoke, EvalReport, Delta}`, `Error`) with every fallible fn returning `Err(Error::NotImplemented("…"))` and the two pure metric fns returning `f64::NAN` (so goldens fail on comparison, not on compile); `missing_docs` satisfied; deleted by T045
- [X] T012 Extend `scripts/check-no-stubs.sh` to scan `crates/xtriever-eval/src/` for `NotImplemented`

### Tests (red except `fixtures_valid`)

- [X] T013 [P] Write `crates/xtriever-eval/tests/support/mod.rs` (fixture loaders; `synthetic_dataset(dir)` that writes a tiny BEIR-shaped corpus/queries/qrels **with CRLF and a header line** plus a manifest with correct hashes and counts; `tamper(path)`) and `tests/fixtures_valid.rs` asserting every `reference/fixtures/003/manifest.json` hash — **green** at the red checkpoint
- [X] T014 [P] Write `crates/xtriever-eval/tests/metrics.rs` covering **US1 scenarios 1–6**: every golden case's per-query and mean values within 1e-6; the documented no-relevant / unjudged / not-retrieved treatments (US1 scenarios 2 and 6); fewer-than-cutoff (3); graded gain (4); duplicate ids counted once; ties scored in the given order; plus `tests/metrics_prop.rs` (proptest ≥ 500 cases): permuting result ids **below** the cutoff leaves nDCG@k unchanged; recall@k is monotone non-decreasing in k; the mean is independent of the order results are supplied in (FR-004)
- [X] T015 [P] Write `crates/xtriever-eval/tests/dataset.rs` (offline, synthetic files from `support`) covering **US2 scenarios 3–5**: a tampered file fails naming the path and **both** hashes (`Error::HashMismatch { path, expected, actual }`); counts match the synthetic manifest; a qrels row with `\r` parses to an integer grade; the header line is skipped; a dangling judged id is reported by count, not fatal (FR-010); the manifest refuses a dataset name outside `scifact|nfcorpus|fiqa`
- [X] T016 [P] Write `crates/xtriever-eval/tests/dataset_real.rs` (`#[ignore]`, needs the cache) covering **US2 scenarios 1, 2, 4, 5**: for each dataset `Dataset::load` succeeds, counts equal research D1 exactly (5183/300/339 · 3633/323/12334 · 57638/648/1706), NFCorpus contains grade 2, FiQA has 0 non-empty titles and **55** query ids colliding with document ids, dangling counts are 0
- [X] T017 [P] Write `crates/xtriever-eval/tests/run.rs` covering **US3 scenarios 1–2** offline: `EvalConfig::lexical_baseline_v1()` builds a `Schema` with `title` (`Text("standard_en")`, boost 2.0, not stored) and `text` (`Text("standard_en")`, boost 1.0); `build()` omits the `title` field for an empty title and assigns `DocId(i)` in corpus order with a reversible id map; `execute()` against a **stub `LexicalIndex`** (a test-local impl returning canned `Hit`s) submits every judged query, respects `k = 100`, drops a result whose doc id equals the query id and counts it in `dropped_identical`, and exports the run as ordered JSONL
- [X] T018 [P] Write `crates/xtriever-eval/tests/report.rs` covering **US4 scenarios 1–3, 5**: `score()` fills every `EvalReport` field in the contract's key order (assert the serialized key sequence); `delta()` yields before/after/abs/rel for 2 metrics × 3 datasets and sets `adr_trigger` iff nDCG@10 fell on ≥ 2 datasets; `smoke()` fails naming the metric and both values when either SciFact metric is lower, passes and returns the delta otherwise (tolerance 0); the Markdown table renders deterministically; plus `tests/baseline_real.rs` (`#[ignore]`) covering **US3 scenarios 1, 3** and FR-020: build SciFact through `TantivyIndex` (dev-dep), run twice ⇒ byte-identical reports; `mean_ndcg_10` within ±0.10 of **0.665** (research D5) — a miss fails the test and is a finding to record, never a wider band
- [X] T019 Run `cargo nextest run -p xtriever-eval` (suite in `crates/xtriever-eval/tests/`): `fixtures_valid` **passes**, every non-ignored test **fails** on `not implemented` or `NaN`, no failure names a fixture or parse problem. Commit red (Rule 4). **Checkpoint — PR 1.**

---

## Phase 3: Foundational Implementation (Blocking Prerequisite)

**Purpose**: The error type and the manifest — the two things every story reads.

- [X] T020 Implement `crates/xtriever-eval/src/error.rs`: `thiserror` enum `Error { Manifest(String), HashMismatch { path: PathBuf, expected: String, actual: String }, Parse { path: PathBuf, line: usize, msg: String }, Io(#[from] std::io::Error), Core(#[from] xtriever_core::Error) }` — messages name the path and both hashes (FR-007); unit tests in-file
- [X] T021 Implement `Manifest` in `crates/xtriever-eval/src/dataset.rs`: serde structs mirroring data-model `DatasetManifest`; `Manifest::load(path)`; `Manifest::dataset(name)` ⇒ `Error::Manifest` for anything but the three names; `verify_file(cache_dir, dataset, rel_path)` streaming SHA-256 in 1 MiB chunks comparing **bytes and hash** ⇒ `HashMismatch` (research D1)
- [X] T022 Wire `crates/xtriever-eval/src/lib.rs` to the real `error` and `dataset::Manifest`, keep the scaffold for everything else; run `cargo nextest run -p xtriever-eval --test dataset` and confirm the manifest/hash cases are **green**, the loader cases still red

**Checkpoint**: hashes are enforced. Story work begins.

---

## Phase 4: User Story 1 — Trustworthy metrics (Priority: P1) 🎯 MVP

**Goal**: nDCG@10 and Recall@100 over an ordered id list, matching `pytrec_eval` on every golden.

**Independent Test**: `tests/metrics.rs` + `tests/metrics_prop.rs` green — no dataset needed.

- [X] T023 [US1] Implement `crates/xtriever-eval/src/metrics.rs` per research **D3**: `ndcg_at(ranked: &[&str], grades: &BTreeMap<String, u32>, k)` — DCG = Σ over the first `k` **distinct** ids of `grade / log2(rank + 1)` (linear gain; a repeated id counts once, at its first rank), IDCG from all grades > 0 sorted descending cut at `k`, `0.0` when IDCG is 0 (no relevant document); `recall_at(ranked, grades, k)` = |{relevant} ∩ first-k distinct| / |relevant|, `0.0` when there is no relevant document; never re-sort `ranked`
- [X] T024 [US1] Implement the per-query aggregation in `crates/xtriever-eval/src/metrics.rs`: `score_queries(run: &BTreeMap<String, Vec<String>>, qrels: &Qrels) -> QueryMetrics` — queries in the run but not in qrels are **ignored and counted**; judged queries not in the run are **excluded from the mean and counted**; a judged query with an empty list scores 0.0 and **is** counted; the mean sums in ascending query-id order (FR-004) and also reports `round(…, 5)` (BEIR presentation, D3)
- [X] T025 [US1] Run `cargo nextest run -p xtriever-eval --test metrics --test metrics_prop --test fixtures_valid` (files under `crates/xtriever-eval/tests/`) — **green**, 1e-6 on every golden (SC-001). **Checkpoint — MVP: the instrument exists.**

---

## Phase 5: User Story 2 — Datasets pinned and loaded (Priority: P1)

**Goal**: Hash-verified loading of the three datasets with the exact counts.

**Independent Test**: `tests/dataset.rs` (offline) and `tests/dataset_real.rs` (with the cache).

- [X] T026 [US2] Implement `Corpus`, `QuerySet`, `Qrels` loaders in `crates/xtriever-eval/src/dataset.rs` per data-model: each file is `verify_file`d **before** parsing; `corpus.jsonl` ⇒ `ids: Vec<String>` in file order + `titles`/`texts` (empty title kept as `""`); `queries.jsonl` ⇒ `(id, text)` in file order; `qrels/test.tsv` ⇒ skip the header line, strip `\r`, split on `\t`, parse `grade: u32` (research D2 — the CRLF trap), into `BTreeMap<query, BTreeMap<doc, grade>>`; a malformed row ⇒ `Error::Parse { path, line, msg }`
- [X] T027 [US2] Implement `Dataset::load(manifest, name, cache_dir)` in `crates/xtriever-eval/src/dataset.rs`: load all three, compute `dangling { queries, documents }` (judged ids absent from the query set / corpus) and report them by count without failing (FR-010), then assert the manifest `counts` (documents, queries, judged_queries, judgement_pairs) ⇒ `Error::Manifest` naming the mismatched count otherwise (FR-009)
- [X] T028 [US2] Run `cargo nextest run -p xtriever-eval --test dataset` — **green**; then `cargo nextest run -p xtriever-eval --test dataset_real --run-ignored only` with the cache present — **green** (SC-002); then the quickstart Step 4 tamper check: append a byte to `reference/datasets/beir/scifact/qrels/test.tsv`, confirm `Dataset::load` fails with `HashMismatch` naming the file and both hashes (SC-003), restore with `./scripts/fetch-beir.sh scifact`. **Checkpoint — PR 2.**

---

## Phase 6: User Story 3 — The lexical baseline (Priority: P2)

**Goal**: Run `lexical-baseline-v1` on all three datasets; commit the reports; cross-check them
against the reference; record the FR-020 verdicts and the FiQA observations.

**Independent Test**: `tests/run.rs` (offline, stub index) and `tests/baseline_real.rs` (SciFact
end-to-end, twice).

- [X] T029 [US3] Implement `EvalConfig` and `build()` in `crates/xtriever-eval/src/run.rs` per research **D5** / data-model `EvalConfig`: `lexical_baseline_v1()` = fields `title` and `text`, both `Text(AnalyzerId("standard_en"))`, boosts **2.0** / 1.0, `indexed: true`, `stored: false`; `omit_empty_fields: true`; `query: MatchAll`; `k: 100` (must be ≥ 100, else `Error::Manifest`); `build(dataset, cfg) -> (Schema, Vec<Document>, IdMap)` assigns `DocId(i)` in corpus order, omits a text field whose value is empty, never puts the BEIR string id into a `Document` (FR-014); `IdMap` = `Vec<String>` with `external(DocId) -> &str`
- [X] T030 [US3] Implement `execute(index: &dyn LexicalIndex, ids: &IdMap, dataset, cfg) -> Result<Run>` in `crates/xtriever-eval/src/run.rs`: for every **judged** query in ascending id order build `LexicalQuery::Match(None, text)`, `search(&q, None, cfg.k)`, map hits to external ids **preserving the retriever's order**, drop any result whose external id equals the query id and count it in `dropped_identical` (BEIR wrapper rule, D3 — FiQA has 55 such id pairs), collect into `results: BTreeMap<String, Vec<String>>`; `Run::export_jsonl(path)` writes `{"query_id", "doc_ids"}` per line
- [X] T031 [US3] Implement `score()` and `EvalReport` in `crates/xtriever-eval/src/report.rs`: call `metrics::score_queries`, fill every field in the contract's **stated key order** (`config`, `dataset`, `lexical_commit`, `harness_commit`, `dataset_hashes`, `counts`, `scored_queries`, `no_relevant_queries`, `dropped_identical`, `unjudged_queries`, `mean_ndcg_10`, `mean_recall_100`, `beir_rounded`, `per_query` sorted by id, `observations` optional) using a `serde` struct declared in that order; `lexical_commit` is the constant `94ddbe67f926badf962b93e8bd29d687e300189a` (ADR-0006 condition 1); `harness_commit` is passed in by the caller; `to_markdown_table()` for `report.md`
- [X] T032 [US3] Write `crates/xtriever-eval/examples/beir.rs` subcommands `verify` and `run` per the contract: `run --dataset D --config lexical-baseline-v1 [--out F] [--export-run F] [--index-dir DIR]` loads the dataset, `build`s, creates a `TantivyIndex` in `--index-dir` (default a `tempfile` dir; kept when given so `du` can measure it), `add`s all documents in **one batch**, `commit`s, `execute`s, `score`s with `harness_commit = git rev-parse HEAD`, writes the report; `anyhow` for errors; prints the two means and the counts
- [X] T033 [US3] Produce the SciFact baseline: `cargo run --release -p xtriever-eval --example beir -- run --dataset scifact --config lexical-baseline-v1 --out specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json --export-run target/run-scifact.jsonl`, then `reference/.venv-003/bin/python reference/gen_003_fixtures.py --verify-run target/run-scifact.jsonl --qrels reference/datasets/beir/scifact/qrels/test.tsv --report specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json` — must agree within 1e-6 (SC-001 at scale); run it a second time to a temp path and `diff` — identical (SC-004); `time` the run and record the seconds (SC-008)
- [X] T034 [US3] Produce the NFCorpus and FiQA baselines the same way into `specs/003-eval-harness/baselines/lexical-baseline-v1.{nfcorpus,fiqa}.json`, each cross-checked with `--verify-run`; for FiQA use `--index-dir target/xt-eval-index/fiqa` and measure **outside** the process: `/usr/bin/time -l … | grep 'maximum resident'` and `du -sk target/xt-eval-index/fiqa`; add them to the FiQA report's `observations { index_dir_bytes, peak_rss_bytes, method }` (FR-018, SC-009)
- [X] T035 [US3] Apply FR-020 to the three `mean_ndcg_10` values against the pinned figures **0.665 / 0.325 / 0.236** (research D5, ±0.10) and record each verdict in `specs/003-eval-harness/report.md` with the published Recall@100 alongside for information; a miss is recorded as a finding with the number and a first hypothesis (analyzer, k1/b, field handling) — the band is **not** changed (Rule 6)
- [X] T036 [US3] Run `cargo nextest run -p xtriever-eval --test run` (files under `crates/xtriever-eval/tests/`) — **green**; then `cargo nextest run -p xtriever-eval --test baseline_real --run-ignored only` — **green** (US3 scenarios 1, 3; FR-020 on SciFact)

**Checkpoint**: the project has its first retrieval-quality numbers, cross-checked.

---

## Phase 7: User Story 4 — Deltas and the gate (Priority: P2)

**Goal**: Delta tables for PR descriptions and a smoke that fails on a decrease.

**Independent Test**: `tests/report.rs` green; quickstart Step 7's degraded-baseline run exits 2.

- [X] T037 [US4] Implement `delta(before, after) -> Delta` and `Delta::to_markdown()` in `crates/xtriever-eval/src/report.rs`: per dataset (matched by name) and metric: `before`, `after`, `abs = after − before`, `rel = abs / before` (`rel` = `NaN` shown as `n/a` when `before == 0`); `adr_trigger = true` iff nDCG@10 fell on ≥ 2 of the datasets present, with the sentence the constitution uses ("lowers nDCG@10 on the majority of benchmark datasets — needs an ADR") printed when true (FR-021, FR-022)
- [X] T038 [US4] Implement `smoke(baseline, current) -> Result<Delta, SmokeFailure>` in `crates/xtriever-eval/src/report.rs`: `SmokeFailure { metric, baseline, current }` when `current < baseline` for either metric (tolerance **0**, spec Assumptions); the `Display` names the metric and both values (FR-024)
- [X] T039 [US4] Add the `delta` and `smoke` subcommands to `crates/xtriever-eval/examples/beir.rs`: `delta before.json after.json` prints the table and the trigger line, exit 0; `smoke --dataset scifact --baseline F` runs SciFact, prints the delta, exits **0** on pass and **2** on `SmokeFailure`
- [X] T040 [US4] Run `cargo nextest run -p xtriever-eval --test report` (files under `crates/xtriever-eval/tests/`) — **green** (SC-005); then quickstart Step 7 end to end: `smoke` against the committed SciFact baseline exits 0 and prints an all-zero delta; against a `jq`-raised baseline it exits 2 naming `ndcg_10` and both values (SC-006). **Checkpoint — PR 3.**

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: The CI job that restores the gate, the report, and the full gate. Completes **PR 4**.

- [X] T041 Add the `eval-smoke` job to `.github/workflows/ci.yml` per research **D8**: `ubuntu-latest`; a first step using `dorny/paths-filter@v3` with `ranking: crates/xtriever-{lexical,dense,rerank,ltr,pipeline}/**` and every later step gated on `steps.filter.outputs.ranking == 'true'`; `actions/cache@v4` on `reference/datasets/beir/scifact.zip` + `reference/datasets/beir/scifact/` keyed by the manifest's SciFact archive sha256; `./scripts/fetch-beir.sh scifact`; `cargo run --release -p xtriever-eval --example beir -- smoke --dataset scifact --baseline specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json`; **blocking** (no `continue-on-error`); a comment block quoting FR-023's escape hatch verbatim so a future demotion has to edit it in place
- [ ] T042 **(pending the first push — needs GitHub; see report.md "CI")** Verify the CI job on a throwaway branch: one commit touching `crates/xtriever-lexical/README.md` (or a comment) must run `eval-smoke` and pass, a second push must hit the cache (no download in the log), and a commit touching only `docs/` must skip the job (US4 scenario 4); record the run URLs in `report.md`
- [X] T043 [P] Write `specs/003-eval-harness/report.md`: the baseline table (3 datasets × nDCG@10 / Recall@100, full precision and BEIR-rounded, published figures and FR-020 verdicts alongside), counts and `dropped_identical` per dataset, the `--verify-run` agreement, SC-008's SciFact wall time, the FiQA observations with method, the CI run URLs, any findings, the ADR-0006 line ("condition 2 discharged: SciFact measured at `94ddbe6`"), and a per-SC table SC-001…SC-010 naming the proving test or artefact
- [X] T044 [P] Re-validate `specs/003-eval-harness/checklists/requirements.md` against the shipped behaviour and append an "Iteration 3 — implementation" note (interpretations made, if any; the spec's "≈" counts became exact via research D1)
- [X] T045 Delete `crates/xtriever-eval/src/scaffold.rs` and every `NotImplemented` / `NAN` placeholder; `./scripts/check-no-stubs.sh` must PASS for all three crates
- [X] T046 Finalize crate docs: `crates/xtriever-eval/src/lib.rs` crate-level docs state the D3 conventions in one table (what is ignored, excluded, or scored zero) since FR-003 requires them documented; `cargo doc -p xtriever-eval --no-deps` with zero warnings; `Cargo.toml` `description` final
- [X] T047 Run the full gate from [quickstart.md](./quickstart.md) Step 8: `cargo fmt --all --check`; `RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets`; `cargo nextest run --workspace`; `cargo deny check`; `cargo check --workspace` for `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android`; the purity grep on `cargo tree -p xtriever-eval -e normal`; `./scripts/check-no-stubs.sh`; and `git diff --stat main -- crates/xtriever-core crates/xtriever-lexical deny.toml` **empty** (FR-027, SC-010)
- [X] T048 Write the PR description into `specs/003-eval-harness/pr-description.md` per quickstart "PR description contents": nextest summary, the baseline table, band verdicts, FiQA observations, SciFact wall time, and the line **"eval delta: this feature establishes the baseline (ADR-0006 condition 2 discharged)"**; suggest the 4-commit split with hand-written line counts
- [X] T049 Final review pass of `crates/xtriever-eval/src/` against Rule 7 and Principle VII: `grep -rn 'unwrap()\|expect(\|panic!\|todo!\|unimplemented!\|unsafe' crates/xtriever-eval/src/` returns nothing outside doc comments; no generics over stages, no macros beyond `thiserror`/`serde` derives; the only place `TantivyIndex` is named is `examples/beir.rs` and `tests/baseline_real.rs`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1**: T001 → T002; T003 ∥ T004 ∥ T005; T006 needs T004 + T005.
- **Phase 2**: needs Phase 1. T007 → T008 → T009 → T010 (one script); T011 → T012; T013–T018
  parallel after T010 + T011; T019 last. **Blocks everything below.**
- **Phase 3**: needs T019. T020 → T021 → T022. **Blocks all stories.**
- **US1 (Phase 4)**: needs Phase 3. T023 → T024 → T025.
- **US2 (Phase 5)**: needs Phase 3. Independent of US1. T026 → T027 → T028.
- **US3 (Phase 6)**: needs US1 (metrics) and US2 (datasets). T029 → T030 → T031 → T032 → T033 →
  T034 → T035 → T036.
- **US4 (Phase 7)**: needs US3 (a baseline to compare against). T037 → T038 → T039 → T040.
- **Phase 8**: needs US4. T041 → T042; T043 ∥ T044; T045 → T046 → T047 → T048 → T049.

### User Story Dependencies

| story | depends on | why |
|---|---|---|
| US1 metrics | Phase 3 | error type only |
| US2 datasets | Phase 3 | manifest + hashing |
| US3 baseline | US1, US2 | scores datasets |
| US4 deltas/gate | US3 | needs a committed baseline |

US1 ∥ US2 after Phase 3.

### Parallel Opportunities

- Phase 1: T003 ∥ T004 ∥ T005. Phase 2: T013–T018 (six test files). After Phase 3: **US1 ∥ US2**.
  Phase 8: T043 ∥ T044.

---

## Parallel Example: Phase 2 red suite

```bash
# After T010 (goldens) and T011 (scaffold):
Task: "Write crates/xtriever-eval/tests/support/mod.rs + tests/fixtures_valid.rs"   # T013
Task: "Write crates/xtriever-eval/tests/metrics.rs + tests/metrics_prop.rs"         # T014
Task: "Write crates/xtriever-eval/tests/dataset.rs"                                 # T015
Task: "Write crates/xtriever-eval/tests/dataset_real.rs (#[ignore])"                # T016
Task: "Write crates/xtriever-eval/tests/run.rs"                                     # T017
Task: "Write crates/xtriever-eval/tests/report.rs + tests/baseline_real.rs"         # T018
# Then T019: confirm red for the right reason, commit.
```

## Parallel Example: after Phase 3

```bash
Task: "US1 — metrics.rs, goldens green"          # T023–T025
Task: "US2 — dataset loaders, real datasets green" # T026–T028
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 → Phase 2 (**PR 1**, red) → Phase 3.
2. Phase 4: metrics green against every `pytrec_eval` golden (T025).
3. **STOP and VALIDATE**: the instrument agrees with the reference on all eleven cases — that is
   demonstrable with no dataset and no network.

### Incremental Delivery

1. PR 1: oracle, goldens, manifest, fetch script, every test red.
2. PR 2: metrics + datasets (US1, US2).
3. PR 3: runner, three baselines, delta/smoke (US3, US4).
4. PR 4: CI job, report, docs, full gate.

### Where this plan pre-commits to stopping (Rule 6)

- T007: the generator refuses if `pytrec_eval`'s conventions differ from research D3.
- T033/T034: a `--verify-run` disagreement stops the baseline from being committed.
- T035: an FR-020 band miss is recorded as a finding; the band is not widened.
- T040: a smoke that passes against a raised baseline is a bug in the gate, not in the baseline.

---

## Notes

- The hashes and counts to assert are in research D1 — copy them, do not re-derive from memory.
- `qrels/test.tsv` has a header and CRLF line endings in all three datasets (research D2).
- The runner never re-sorts the retriever's results; the oracle is fed rank-based scores (D4).
- The scaffold (T011) exists only so Phase 2 ends in runtime failures; T045 removes it.
