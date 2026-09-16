# Quickstart: The Python Wikipedia Demo

Everything runs from the repository root on the host. No device, no simulator. Identifiers
never go into a tracked file (no hostnames in records — the machine is named by its
hardware model).

## Prerequisites

```bash
ls reference/models/all-MiniLM-L6-v2/model.safetensors reference/models/ms-marco-MiniLM-L-6-v2/model.safetensors   # scripts/fetch-model.sh (both manifests)
ls target/xt-wiki/index/xtriever-pipeline.json target/xt-wiki/expected.json target/xt-wiki/ATTRIBUTION.txt        # the 008 artefact + 017 goldens
ls reference/datasets/wiki/simple.jsonl                                                                             # scripts/fetch-wiki.sh
ls swift/Xtriever/Tests/Fixtures/index/xtriever-pipeline.json swift/Xtriever/Tests/Fixtures/expected.json            # the 007 fixture + goldens
ls target/wheels/xtriever-*.whl                                                                                     # (cd python && .venv/bin/maturin build --release)
unset SDKROOT
```

## Step 0 — the demo's environment

```bash
cd apps/python-wiki-demo
uv venv .venv --python 3.12
uv pip install --python .venv/bin/python ../../target/wheels/xtriever-*.whl -e ".[test]"
.venv/bin/wikidemo --help
cd ../..
```

## Step 1 — red (Rule 4)

```bash
apps/python-wiki-demo/.venv/bin/pytest apps/python-wiki-demo/tests -q
```

Expected at the red commit: every test fails on import (`wikidemo.<module>` does not
exist) — collection errors count. Record the failure in the report.

## Step 2 — the model-free suite, then the model-backed one

```bash
apps/python-wiki-demo/.venv/bin/pytest apps/python-wiki-demo/tests -q -m "not models"   # chunker replay (57 cases), rules, URL (11), marks, identity, inputs, rendering
apps/python-wiki-demo/.venv/bin/pytest apps/python-wiki-demo/tests -q                   # + search/about/build against the 007 fixture
```

Expected: all green; the model-backed tests skip with the missing path in the reason if a
prerequisite is absent (never silently green).

## Step 3 — search the shipped index (US1, US2, US4)

```bash
apps/python-wiki-demo/.venv/bin/wikidemo search "why is the sky blue"
apps/python-wiki-demo/.venv/bin/wikidemo search --explain --depth 20 "who painted the Mona Lisa"
apps/python-wiki-demo/.venv/bin/wikidemo search --depth 0 "how do vaccines work"
apps/python-wiki-demo/.venv/bin/wikidemo search --budget-ms 300 "what is the capital of Australia"     # a degradation line, exit 0
apps/python-wiki-demo/.venv/bin/wikidemo search --budget-ms 300 --strict "what is the capital of Australia"; echo "exit $?"   # the engine's error, exit 1
apps/python-wiki-demo/.venv/bin/wikidemo search "the of and"; echo "exit $?"                              # no passages found, exit 0
apps/python-wiki-demo/.venv/bin/wikidemo about
XTRIEVER_WIKI_ARTEFACT=/nonexistent apps/python-wiki-demo/.venv/bin/wikidemo about; echo "exit $?"     # missing input named with its producer, exit 1
```

Expected: the output layout of `contracts/cli.md`; fused list before re-ranked list with
marks; eight features under `--explain`; About's attribution equals
`target/xt-wiki/ATTRIBUTION.txt` (`diff <(wikidemo about | sed -n '/^Text from/,/^Corpus identity/p') target/xt-wiki/ATTRIBUTION.txt`).

## Step 4 — the host measurement record (US5, FR-016)

```bash
apps/python-wiki-demo/.venv/bin/wikidemo measure
```

Expected: `parity: PASS` (20 queries, 4 depths, lexical 20/20, fused order 20/20, max
diffs 0.0 on the machine that minted the goldens); the record under
`specs/019-python-wiki-demo/runs/`; `latency.medianFusedMs ≤ 1000`,
`latency.medianRerankedMs ≤ 3000` (SC-001). A `FAIL` is stop-and-report (Rule 6).

## Step 5 — build a slice and check it against the Rust build (US3, FR-014, SC-005)

```bash
cargo run --release -p xtriever-cli -- wiki build --limit 2000 --out target/xt-wiki-slice-rs    # seconds to minutes (cache)
time apps/python-wiki-demo/.venv/bin/wikidemo build --limit 2000 --out target/xt-wiki-slice-py    # ≈ 93 ms per passage
apps/python-wiki-demo/.venv/bin/wikidemo about --artefact target/xt-wiki-slice-py
apps/python-wiki-demo/.venv/bin/wikidemo search --artefact target/xt-wiki-slice-py "April"
apps/python-wiki-demo/.venv/bin/wikidemo measure --artefact target/xt-wiki-slice-py --against target/xt-wiki-slice-rs
diff <(jq -S 'del(.counts)' target/xt-wiki-slice-py/index/corpus.json) <(jq -S 'del(.counts)' target/xt-wiki-slice-rs/index/corpus.json)   # identity, snapshot, rules, chunker equal
diff <(jq -S .counts target/xt-wiki-slice-py/index/corpus.json) <(jq -S .counts target/xt-wiki-slice-rs/index/corpus.json)               # counts equal
```

Expected: the two `corpus.json`s differ in nothing; the slice record says `PASS` with
`allBitsIdentical == hitsCompared`; the Python build finished under 30 minutes. A
difference in ids or order is stop-and-report; a bits difference with the order intact goes
into the record and the report as found.

Also once: `wikidemo build --out target/xt-wiki-slice-py` (no limit) → the cost warning
printed, Ctrl-C within the pause; `wikidemo build --limit 0 …` → usage error;
`wikidemo build --limit 3 --out target/xt-wiki-slice-py` (exists) → refused, exit 1.

## Step 6 — the gate (Rule 5)

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo nextest run --workspace && cargo deny check   # unchanged, run once
git diff --stat main -- crates/ swift/ python/src specs/*/baselines            # empty (SC-007)
apps/python-wiki-demo/.venv/bin/pytest apps/python-wiki-demo/tests -q         # all green
python/.venv/bin/pytest python/tests -q                                       # the package suite, untouched
grep -rn "$(scutil --get ComputerName 2>/dev/null)" specs/019-python-wiki-demo apps/python-wiki-demo || true   # no hostname in any record
git status --short | grep -v '^??' ; ls target/xt-wiki-slice-* >/dev/null   # slice artefacts stay under target/ (gitignored)
```

## Step 7 — documents

`apps/python-wiki-demo/README.md` (the commands in run order, inputs and producers, flags
and defaults with the 018 trade-off, the 013 schema note, the full build's cost, the
records); repository `README.md` (the second demo); `specs/009-ios-wiki-demo/{spec,report}.md`
(pointer); `specs/019-python-wiki-demo/report.md`; `pr-description.md`.
