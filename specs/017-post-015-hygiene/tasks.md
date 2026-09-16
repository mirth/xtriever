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

- [ ] T001 Confirm the inputs: `ls target/xt-wiki/index/xtriever-pipeline.json target/xt-wiki/expected.json reference/fixtures/008/queries.json reference/models/all-MiniLM-L6-v2/model.safetensors reference/models/ms-marco-MiniLM-L-6-v2/model.safetensors`; `jq '.queries | length, (.queries[0].depths | keys)' target/xt-wiki/expected.json` → 20, `["0","20","5"]`; `cp target/xt-wiki/expected.json /tmp/xt-wiki-expected-before.json`; reproduce the budget test's failure on `main`'s code once for the record: `cargo nextest run -p xtriever-ffi --run-ignored all -E 'test(a_short_time_budget)'` → FAIL with `q0: N ms` (paste N); `mkdir -p specs/017-post-015-hygiene/runs`

---

## Phase 2: User Story 2 — The budget test asserts the contract (Priority: P1)

**Goal**: no elapsed-time assertion; the contract asserted more precisely; passes today.

**Independent Test**: the test passes on this machine; `grep "elapsed_ms <"` finds nothing; the mutation check fails it.

- [ ] T002 [US2] Rewrite `a_short_time_budget_yields_a_partial_rerank_without_an_error` in `crates/xtriever-ffi/tests/budget.rs` (research D2): loop over budgets `[200, 100, 50, 25, 10]` ms — for each budget, for every fixture query: `search` is `Ok` (degrading mode), `!r.stages.time_limit_ignored` (a clock is attached), the scored hits form a prefix (`take_while(rerank_score.is_some()).count() == rr.scored`), count `partial` where `rr.skipped.is_none() && 0 < rr.scored < rr.candidates`; stop at the first budget with `partial ≥ 1`; assert `partial ≥ 1` with the message naming the budgets tried; then the negative: `budgeted(60_000, false)` on every query → `rr.skipped.is_none() && rr.scored == rr.candidates` (a generous budget re-ranks everything). **No assertion mentions `elapsed_ms`.** Doc comment: why (the 015 F-003 failure on `main`, `q0: 3233 ms` uncontended). `cargo clippy -p xtriever-ffi --all-targets -- -D warnings` clean
- [ ] T003 [US2] Run `cargo nextest run -p xtriever-ffi --run-ignored all -E 'test(a_short_time_budget)'` → PASS (record the budget at which `partial ≥ 1` appeared); mutation check: comment out check point C in `crates/xtriever-pipeline/src/search.rs` `rerank()` (the `match check_budget(opts)` block) so no budget is ever applied → the test fails on `partial >= 1`; restore (`git diff --stat crates/xtriever-pipeline` empty); record both outcomes in `specs/017-post-015-hygiene/report.md`

---

## Phase 3: User Story 1 — The device agrees with the host under the new default (Priority: P1)

**Goal**: depth 10 in the generator and the harness; the goldens regenerated and verified; the device run.

**Independent Test**: the depth-0 responses byte-identical (modulo the added `rerank_combined_bits` key); the record with parity PASS at 0 / 5 / 10 / 20.

- [ ] T004 [P] [US1] In `crates/xtriever-cli/src/wiki/expected.rs`: depths `[0u32, 5, 10, 20]` (`:27` loop and the doc comment `:21`); `cargo clippy -p xtriever-cli --all-targets -- -D warnings` clean
- [ ] T005 [P] [US1] In `swift/Xtriever/Tests/XtrieverTests/DeviceMeasurementTests.swift`: `static let depths: [UInt32] = [0, 5, 10, 20]` (`:22`) and the header comment (`:8`); nothing else in the harness changes (parity is per id, tolerance 1e-3 stays)
- [ ] T006 [US1] Regenerate the goldens (quickstart Step 2): `cargo run --release -p xtriever-cli -- wiki expected --index target/xt-wiki/index --embedder-dir reference/models/all-MiniLM-L6-v2 --reranker-dir reference/models/ms-marco-MiniLM-L-6-v2 --queries reference/fixtures/008/queries.json --out target/xt-wiki/expected.json`; verify with the quickstart's Python snippet: depth-0 responses equal the previous file's (hits compared without the new `rerank_combined_bits` key) for all 20 queries (**⛔** otherwise — nothing un-re-ranked may change), depths `["0","10","20","5"]`, `info.rerank_mode == {"interpolate": {"alpha": 0.5}}`; count how many depth-5 / depth-20 heads changed; paste into the report
- [ ] T007 [US1] `scripts/build-ios-package.sh --with-models --with-wiki` → PASS; then the **owner** runs on the phone: `cd swift/Xtriever && TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -scheme Xtriever -destination 'platform=iOS,id=XXXXX' -configuration Release ARCHS=arm64 DEVELOPMENT_TEAM=XXXXX -allowProvisioningUpdates -only-testing:XtrieverTests/DeviceMeasurementTests 2>&1 | tee /tmp/017-device.log` (identifiers only on the command line); `scripts/extract-device-run.py /tmp/017-device.log specs/017-post-015-hygiene/runs/`; the record shows `parity.verdict == "PASS"` with 20 queries compared over four depths, the footprint verdict PASS under 600 MB, per-depth latency including `"10"` (**⛔** a parity FAIL is stop-and-report: the goldens or the harness, never the tolerance); paste the memory and per-depth latency medians into the report beside the 009 record's

---

## Phase 4: User Story 3 — The reference suites collect together (Priority: P2)

- [ ] T008 [P] [US3] `reference/tests_012/`: add `helpers_012.py` with `REPO`; `conftest.py` imports it (keeps its `sys.path` insertion); `test_scorer.py`, `test_recipe.py` (and any other) `from helpers_012 import REPO`; `reference/tests_014/`: add `helpers_014.py` with `REPO`, `K`; `conftest.py` imports them; `test_order.py`, `test_variants.py` `from helpers_014 import K`; no test in any suite imports `conftest`; `grep -rn "from conftest import" reference/tests_0*` → nothing
- [ ] T009 [US3] `reference/.venv-012/bin/python -m pytest reference/tests_012 reference/tests_014 reference/tests_016 -q` → collected in one run, 014 57 + 016 15 pass, 012's tests pass or skip as they do alone (its model-backed ones skip without caches); each suite alone still passes; paste the counts into the report

---

## Phase 5: User Story 4 — The remaining documents (Priority: P2)

- [ ] T010 [P] [US4] `apps/ios-wiki-demo/README.md`: a paragraph under the pipeline description — the re-ranked head is ordered by `0.5·minmax(fused) + 0.5·minmax(cross-encoder)` (015, ADR-0012; +1.45 mean nDCG@10 over the previous replace-order rule on the BEIR sets), the shipped Wikipedia index adopts it on upgrade with no rebuild, `SearchOptions(rerankMode: .replace)` gives the previous order, `explain.rerankCombined` is the ordering score; `crates/xtriever-cli/src/wiki/mod.rs` command docs: the same in two sentences plus "`wiki expected` follows the index's recorded mode — regenerate the goldens after a default change (017)"

---

## Phase 6: Polish

- [ ] T011 Write `specs/017-post-015-hygiene/report.md`: the four items with their evidence — the budget test's before/after and the mutation check; the goldens' verification counts; the device record's parity, memory and per-depth latency beside 009's (and the depth-10 latency as the number 014 F-003 lacked); the combined pytest counts; the two documents; "Deliberately not done" (no default change, no rebuild)
- [ ] T012 Gate (quickstart Step 4): fmt; clippy workspace; `cargo nextest run --workspace`; deny; iOS / iOS-sim / Android checks; `cargo nextest run -p xtriever-ffi --run-ignored all` fully green; `pytest reference/tests_012 reference/tests_014 reference/tests_016 -q`; the Swift suite on the simulator (owner, `-skip-testing:XtrieverTests/DeviceMeasurementTests`); `git diff --stat main -- crates/` shows only `xtriever-cli/src/wiki/{expected,mod}.rs` and `xtriever-ffi/tests/budget.rs`; `git diff --stat main -- specs/*/baselines specs/014-rerank-depth-study/runs specs/016-sparse-remeasure/runs` empty; `grep -rn <device id>|<team id>` over the tree → nothing (the record's `device` field is the model name, not the UDID — check)
- [ ] T013 Write `specs/017-post-015-hygiene/pr-description.md` (the four items in four lines each with their evidence, the record's headline numbers, what is unchanged, the attribution line). **⛔ Commit C2**; the owner pushes and merges

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
