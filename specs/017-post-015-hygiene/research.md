# Research: Hygiene after 013–016

**Feature**: `017-post-015-hygiene` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## D1 — The Wikipedia host goldens and the device parity check

`crates/xtriever-cli/src/wiki/expected.rs` (`xtriever wiki expected`) writes
`target/xt-wiki/expected.json` for the 20 measurement queries at depths **0 / 5 / 20**, k 10,
explained, through the same FFI surface the device uses; `scripts/build-ios-package.sh
--with-wiki` stages it (`:179–184`). The device harness is
`swift/Xtriever/Tests/XtrieverTests/DeviceMeasurementTests.swift` under
`TEST_RUNNER_XTRIEVER_CORPUS=wikipedia`: `static let depths: [UInt32] = [0, 5, 20]` (`:22`),
parity per query and depth — hit counts equal, fused order identical at depth 0 only, BM25
bits identical, dense and re-rank scores within `1e-3` **matched by id, never by rank**
(`:204–235`) — so the interpolating rule changes nothing in the parity logic as long as both
sides run it; the current host file was written by the pre-015 engine, so the re-ranked
depths' hit *sets* (top-10 out of a re-ordered top-20) would differ and the check would report
"not among the host's hits". The record (`RunRecord`, `feature: "008-wiki-corpus"` for the
wikipedia corpus, `:275`) is extracted from the xcodebuild log by
`scripts/extract-device-run.py <log> <runs-dir>`. **Decision**: add depth **10** to both the
generator's depth list and the harness's `depths`; the record's `feature` for a wikipedia run
becomes an argument-free constant change to `"017-post-015-hygiene"`? No — the harness names
the corpus's feature; keep `008-wiki-corpus` in the record's `feature` field and commit the
record under this feature's `runs/` with the extractor's directory argument (the file name
carries device and timestamp; the report says which feature ran it). Rebuild the package with
`--with-models --with-wiki`; the owner runs the harness on the phone; the record is committed.

## D2 — The budget test

`crates/xtriever-ffi/tests/budget.rs:32–60` (before): 200 ms budget, degrading mode, for each
fixture query asserts `elapsed_ms < 1000` (the machine), `!time_limit_ignored` (a clock is
attached), counts partially re-ranked queries and asserts the scored prefix, then `partial >=
1`. On this laptop today the first query takes ~3.4 s (a cold model in `Buffered` mode) and
the test fails on `main`. **Decision (final, after three iterations recorded in the report)**:
no number about the machine anywhere. Per query, an unbudgeted depth-0 search and an
unbudgeted full search give the negative half (no budget → nothing skipped, everything
re-ranked) and a starting guess for the budget; the endpoints are then *verified* by probing
(double the upper budget until a probe re-ranks everything, halve the lower until one scores
nothing) and bisected between them, every probe re-measuring the machine as it is now and
asserting only the contract (`Ok`, `!time_limit_ignored`, scored hits first); the test passes
when any query lands on a partial re-rank. Why not a fixed budget: too small on a slow
machine (the stage is skipped before scoring anything), too large on a fast one. Why not a
budget derived once from measured cost: under a loaded machine the cost measured a moment
earlier does not predict the next call. Why not a "generous" 60 s negative: it was exhausted
once under 22 parallel model-backed tests. The mutation check for the report: hand the
re-ranker no remaining budget (`search.rs` `max_time: remaining` → `None`) — the test must
fail on `partial >= 1`; restored, never committed. (Check point C is a different path: with it
removed the stage is never *skipped*, but the re-ranker still honours its remaining budget.)

## D3 — Collecting the reference suites together

`tests_012` (`from conftest import REPO`), `tests_014` (`from conftest import K`) import the
`conftest` module by name; pytest's default import mode puts every test directory on
`sys.path`, so with two suites the name resolves to whichever `conftest` was inserted last
(the 016 gate had to run them separately). **Decision**: the 016 pattern — a plain helper
module per suite (`helpers_012.py`, `helpers_014.py`) holding the shared names, `conftest`
importing from it, tests importing from it; no `__init__.py` (which would change how pytest
names the modules and is a larger change). Verified by `pytest reference/tests_012
reference/tests_014 reference/tests_016 -q` (012's model-dependent tests skip when their
caches are absent, as they do today).

## D4 — The two documents

`apps/ios-wiki-demo/README.md` describes the pipeline ("fused list first, then the re-ranked
order with what moved") without naming the rule; `crates/xtriever-cli/src/wiki/mod.rs` (the
`wiki` command docs) and `chunking.rs` describe the index schema and say nothing about
re-ranking. **Decision**: one paragraph each — the default (`Interpolate { alpha: 0.5 }`, depth
20, ADR-0012), that an index built before 015 adopts it on upgrade (the shipped Wikipedia
index does), how to select `Replace` (Swift `SearchOptions(rerankMode: .replace)`, the CLI
`wiki expected` follows the index's recorded mode), and where the numbers are
(`specs/015-rerank-interpolation/report.md`).

## D5 — Costs and not done

The package rebuild with wiki: minutes (the models and index are on disk); the device run:
~10 minutes of phone time (20 queries × 4 depths + footprint samples); everything else is
minutes. Not done: no default change (depth 10 is a measurement), no Wikipedia index rebuild,
no engine change, no baseline change.
