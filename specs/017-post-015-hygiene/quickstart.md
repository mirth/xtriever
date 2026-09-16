# Quickstart: validating Hygiene after 013–016

**Feature**: `017-post-015-hygiene` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## Step 1 — The budget test (Rule 4: the rewritten test is the oracle)

```bash
cargo nextest run -p xtriever-ffi --run-ignored all -E 'test(a_short_time_budget)'   # PASS on this machine today (it failed on main)
grep -n "elapsed_ms <" crates/xtriever-ffi/tests/budget.rs                            # nothing
```

Mutation check (recorded in the report, not committed): comment out check point C in
`crates/xtriever-pipeline/src/search.rs` `rerank()` → the test fails on `partial >= 1` /
"no query was partially re-ranked"; restore.

## Step 2 — The host goldens and the device run

```bash
cp target/xt-wiki/expected.json /tmp/xt-wiki-expected-before.json
cargo run --release -p xtriever-cli -- wiki expected --index target/xt-wiki/index --embedder-dir reference/models/all-MiniLM-L6-v2 \
  --reranker-dir reference/models/ms-marco-MiniLM-L-6-v2 --queries reference/fixtures/008/queries.json --out target/xt-wiki/expected.json
python3 - <<'EOF'
import json
a = json.load(open("/tmp/xt-wiki-expected-before.json")); b = json.load(open("target/xt-wiki/expected.json"))
strip = lambda r: {**r, "hits": [{k: v for k, v in h.items() if k != "rerank_combined_bits"} for h in r["hits"]]}
assert all(qa["depths"]["0"] == strip(qb["depths"]["0"]) for qa, qb in zip(a["queries"], b["queries"])), "depth-0 changed"
print("depth-0 identical for", len(a["queries"]), "queries; depths:", sorted(b["queries"][0]["depths"]), "; info.rerank_mode:", b["info"]["rerank_mode"])
EOF
scripts/build-ios-package.sh --with-models --with-wiki --app
# Owner, on the phone (identifiers only on the command line):
# the harness app hosts the tests — a SwiftPM test target cannot be tool-hosted on a device (008 quickstart Step 7)
TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project swift/XtrieverHarnessApp/XtrieverHarnessApp.xcodeproj -scheme XtrieverHarnessApp -configuration Release \
  -destination 'platform=iOS,id=XXXXX' -skipMacroValidation -allowProvisioningUpdates ARCHS=arm64 DEVELOPMENT_TEAM=XXXXX \
  -only-testing:XtrieverHarnessAppTests/DeviceMeasurementTests 2>&1 | tee /tmp/017-device.log
scripts/extract-device-run.py /tmp/017-device.log specs/017-post-015-hygiene/runs/
```

Expected: `parity.verdict: "PASS"` with four depths compared, `footprint` verdict PASS under
600 MB, per-depth latency including `"10"`.

## Step 3 — The suites together, the docs

```bash
reference/.venv-012/bin/python -m pytest reference/tests_012 reference/tests_014 reference/tests_016 -q   # collected together; 012's model-backed tests skip without caches
grep -n "Interpolate\|interpolat" apps/ios-wiki-demo/README.md crates/xtriever-cli/src/wiki/mod.rs
```

## Step 4 — Gate (Rule 5)

fmt · clippy workspace · nextest workspace · deny · iOS / iOS-sim / Android checks · the
model-backed FFI suite (`--run-ignored all`, all green now) · Swift suite on the simulator
(owner) · `git diff --stat main -- crates/ specs/*/baselines specs/014-*/runs specs/016-*/runs`
shows only `crates/xtriever-cli/src/wiki/{expected,mod}.rs` and `crates/xtriever-ffi/tests/budget.rs`
· no identifiers in the tree.
