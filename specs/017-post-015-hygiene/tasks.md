# Tasks: Hygiene after 013–016

**Input**: Design documents from `/specs/017-post-015-hygiene/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [quickstart.md](./quickstart.md); on disk: `target/xt-wiki/{index,expected.json}`
(the 008 artefact), the two models, `reference/fixtures/008/queries.json`, `python/.venv`,
`reference/.venv-012`; the iPhone for the owner's run.

**Tests**: The budget test rewrite *is* the test (spec FR-003; its mutation check is recorded);
the device record is the executable evidence for the goldens; the combined pytest run is the
check for the helper modules. One PR; commits: **C1** = Phases 1–3 (the budget test, the depth
lists, the regenerated goldens' verification), **C2** = Phases 4–6 (the record, suites, docs, report).

**Organization**: Setup → US2 (the budget test — first, it is the one red/green item) → US1
(goldens, harness depth, device run) → US3 (suites) → US4 (docs) → Polish.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4 from spec.md. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`crates/xtriever-ffi/tests/budget.rs`; `crates/xtriever-cli/src/wiki/{expected,mod}.rs`;
`swift/Xtriever/Tests/XtrieverTests/DeviceMeasurementTests.swift`; `reference/tests_012/`,
`reference/tests_014/`; `apps/ios-wiki-demo/README.md`; `specs/017-post-015-hygiene/{runs,report.md,pr-description.md}`.

---

## Phase 1: Setup

- [X] T001 Confirm the inputs: `ls target/xt-wiki/index/xtriever-pipeline.json target/xt-wiki/expected.json reference/fixtures/008/queries.json reference/models/all-MiniLM-L6-v2/model.safetensors reference/models/ms-marco-MiniLM-L-6-v2/model.safetensors`; `jq '.queries | length, (.queries[0].depths | keys)' target/xt-wiki/expected.json` → 20, `["0","20","5"]`; `cp target/xt-wiki/expected.json /tmp/xt-wiki-expected-before.json`; reproduce the budget test's failure on `main`'s code once for the record: `cargo nextest run -p xtriever-ffi --run-ignored all -E 'test(a_short_time_budget)'` → FAIL with `q0: N ms` (paste N); `mkdir -p specs/017-post-015-hygiene/runs`

---

## Phase 2: User Story 2 — The budget test asserts the contract (Priority: P1)

**Goal**: no elapsed-time assertion; the contract asserted more precisely; passes today.

**Independent Test**: the test passes on this machine; `grep "elapsed_ms <"` finds nothing; the mutation check fails it.

- [X] T002 [US2] Rewrite `a_short_time_budget_yields_a_partial_rerank_without_an_error` in `crates/xtriever-ffi/tests/budget.rs` (research D2, final form): per fixture query an unbudgeted depth-0 search and an unbudgeted full search — the negative half asserted on the latter (`rr.skipped.is_none()`, `rr.scored == rr.candidates > 0`) and their elapsed times the starting guess; a `probe(q, ms)` closure classifying a budgeted search (`Ok` in degrading mode, `!r.stages.time_limit_ignored`, scored hits a prefix equal to `rr.scored`) as too small / partial / full; endpoints verified by probing (double the upper budget until full, halve the lower until too small, ≤ 8 each), then ≤ 8 bisection steps re-measured every time; a partial probe anywhere ends the search; assert `partial ≥ 1` naming the probed budgets. **No assertion mentions `elapsed_ms`; no fixed budget, no "generous" number.** Doc comment: the three machine-dependent forms this replaced. `cargo clippy -p xtriever-ffi --all-targets -- -D warnings` clean
- [X] T003 [US2] `cargo nextest run -p xtriever-ffi --run-ignored all -E 'test(a_short_time_budget)'` → PASS alone, and `cargo nextest run -p xtriever-ffi --run-ignored all` → 22/22 inside the contended suite (where two earlier forms failed); mutation check: in `crates/xtriever-pipeline/src/search.rs` `rerank()` hand the re-ranker no remaining budget (`max_time: remaining` → `None`) → the test fails on "no query was partially re-ranked"; restore from HEAD (`git diff --stat main -- crates/xtriever-pipeline` empty); record all outcomes in `specs/017-post-015-hygiene/report.md`

---

## Phase 3: User Story 1 — The device agrees with the host under the new default (Priority: P1)

**Goal**: depth 10 in the generator and the harness; the goldens regenerated and verified; the device run.

**Independent Test**: the depth-0 responses byte-identical (modulo the added `rerank_combined_bits` key); the record with parity PASS at 0 / 5 / 10 / 20.

- [X] T004 [P] [US1] In `crates/xtriever-cli/src/wiki/expected.rs`: depths `[0u32, 5, 10, 20]` (`:27` loop and the doc comment `:21`); `cargo clippy -p xtriever-cli --all-targets -- -D warnings` clean
- [X] T005 [P] [US1] In `swift/Xtriever/Tests/XtrieverTests/DeviceMeasurementTests.swift`: `static let depths: [UInt32] = [0, 5, 10, 20]` (`:22`) and the header comment (`:8`); the parity loop adds an `incomplete` entry when a truth query's depth set is not exactly `Self.depths` (a stale golden with fewer depths can no longer read PASS — review round 1 #2); parity's tolerance 1e-3 stays
- [X] T006 [US1] Regenerate the goldens (quickstart Step 2): `cargo run --release -p xtriever-cli -- wiki expected --index target/xt-wiki/index --embedder-dir reference/models/all-MiniLM-L6-v2 --reranker-dir reference/models/ms-marco-MiniLM-L-6-v2 --queries reference/fixtures/008/queries.json --out target/xt-wiki/expected.json`; verify with the quickstart's Python snippet: depth-0 responses equal the previous file's (hits compared without the new `rerank_combined_bits` key) for all 20 queries (**⛔** otherwise — nothing un-re-ranked may change), depths `["0","10","20","5"]`, `info.rerank_mode == {"interpolate": {"alpha": 0.5}}`; count how many depth-5 / depth-20 heads changed; paste into the report
- [X] T007 [US1] `scripts/build-ios-package.sh --with-models --with-wiki --app` → PASS; then the **owner** runs on the phone through the harness app (a SwiftPM test target cannot be tool-hosted on a device): `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project swift/XtrieverHarnessApp/XtrieverHarnessApp.xcodeproj -scheme XtrieverHarnessApp -configuration Release -destination 'platform=iOS,id=XXXXX' -skipMacroValidation -allowProvisioningUpdates ARCHS=arm64 DEVELOPMENT_TEAM=XXXXX -only-testing:XtrieverHarnessAppTests/DeviceMeasurementTests 2>&1 | tee /tmp/017-device.log` (identifiers only on the command line); `scripts/extract-device-run.py /tmp/017-device.log specs/017-post-015-hygiene/runs/`; the record shows `parity.verdict == "PASS"` with 20 queries compared over four depths, the footprint verdict PASS under 600 MB, per-depth latency including `"10"` (**⛔** a parity FAIL is stop-and-report: the goldens or the harness, never the tolerance); paste the memory and per-depth latency medians into the report beside the 009 record's

---

## Phase 4: User Story 3 — The reference suites collect together (Priority: P2)

- [X] T008 [P] [US3] `reference/tests_012/`: add `helpers_012.py` with `REPO`; `conftest.py` imports it (keeps its `sys.path` insertion); `test_scorer.py`, `test_recipe.py` (and any other) `from helpers_012 import REPO`; `reference/tests_014/`: add `helpers_014.py` with `REPO`, `K`; `conftest.py` imports them; `test_order.py`, `test_variants.py` `from helpers_014 import K`; no test in any suite imports `conftest`; `grep -rn "from conftest import" reference/tests_0*` → nothing
- [X] T009 [US3] `reference/.venv-012/bin/python -m pytest reference/tests_012 reference/tests_014 reference/tests_016 -q` → collected in one run, 014 57 + 016 15 pass, 012's tests pass or skip as they do alone (its model-backed ones skip without caches); each suite alone still passes; paste the counts into the report

---

## Phase 5: User Story 4 — The remaining documents (Priority: P2)

- [X] T010 [P] [US4] `apps/ios-wiki-demo/README.md`: a paragraph under the pipeline description — the re-ranked head is ordered by `0.5·minmax(fused) + 0.5·minmax(cross-encoder)` (015, ADR-0012; +1.45 mean nDCG@10 over the previous replace-order rule on the BEIR sets), the shipped Wikipedia index adopts it on upgrade with no rebuild, `SearchOptions(rerankMode: .replace)` gives the previous order, `explain.rerankCombined` is the ordering score; `crates/xtriever-cli/src/wiki/mod.rs` command docs: the same in two sentences plus "`wiki expected` follows the index's recorded mode — regenerate the goldens after a default change (017)"

---

## Phase 6: Polish

- [X] T011 Write `specs/017-post-015-hygiene/report.md`: the four items with their evidence — the budget test's before/after and the mutation check; the goldens' verification counts; the device record's parity, memory and per-depth latency beside 009's (and the depth-10 latency as the number 014 F-003 lacked); the combined pytest counts; the two documents; "Deliberately not done" (no default change, no rebuild)
- [X] T012 Gate (quickstart Step 4): fmt; clippy workspace; `cargo nextest run --workspace`; deny; iOS / iOS-sim / Android checks; `cargo nextest run -p xtriever-ffi --run-ignored all` fully green; `pytest reference/tests_012 reference/tests_014 reference/tests_016 -q`; the Swift suite on the simulator (owner, `-skip-testing:XtrieverTests/DeviceMeasurementTests`); `git diff --stat main -- crates/` shows only `xtriever-cli/src/wiki/{expected,mod}.rs` and `xtriever-ffi/tests/budget.rs`; `git diff --stat main -- specs/*/baselines specs/014-rerank-depth-study/runs specs/016-sparse-remeasure/runs` empty; `grep -rn <device id>|<team id>` over the tree → nothing (the record's `device` field is the model name, not the UDID — check)
- [X] T013 Write `specs/017-post-015-hygiene/pr-description.md` (the four items in four lines each with their evidence, the record's headline numbers, what is unchanged, the attribution line). **⛔ Commit C2**; the owner pushes and merges

---

## Dependencies & Execution Order

T001 → T002 → T003 (C1 may be cut here or after T006) → (T004 ‖ T005) → T006 → T007 (owner) →
(T008 → T009) ‖ T010 → T011 → T012 → T013 (C2).

### User story completion order

US2 (the one code change with a red/green) → US1 → US3 ‖ US4 → polish.

### Parallel opportunities

T004 ‖ T005 (two files); T008–T009 ‖ T010 while the owner runs T007.

## Implementation Strategy

**MVP** = Phases 1–3: the test fixed and the device agreeing with the host under the new
default. **Rule 6 stop-points**: T006 (a depth-0 response changed), T007 (parity FAIL — fix
the goldens or the harness, never the tolerance), T012 (any gate failure).
