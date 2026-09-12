# Quickstart: validating the Lexical Stage

**Feature**: `002-lexical-stage` | **Date**: 2026-09-11 | **Plan**: [plan.md](./plan.md)

> **Status (2026-09-12, implementation complete)**: every command below has been executed and
> passes — see [report.md](./report.md). The Constitution Check passes on all 14 rows under
> [ADR-0006](../../docs/adr/0006-defer-beir-eval-gate.md) (accepted). The ADR-0005 k-boundary
> amendment that this file once cited was **superseded during implementation** (report.md F-001);
> the tie-break now holds at the boundary and `xtriever-core` is untouched.

Everything in this feature runs on a host. No device, no model, no network after the Python
environment is set up.

---

## Step 0 — Toolchain provenance (do not skip)

```bash
./scripts/check-toolchain.sh
```

Same reason as Feature 001: a shadowing Rust install makes every cross-target check a false FAIL.

## Step 1 — Python reference environment

The oracle needs the pinned venv from Feature 001 plus one package (`snowballstemmer`, for the
`standard_en` analyzer). The requirements file is extended in place and re-pinned.

```bash
./scripts/setup-reference-venv.sh 002      # creates/updates reference/.venv-002 from requirements-002.txt
source reference/.venv-002/bin/activate
python3 --version                          # 3.12.x — the generator refuses anything else
```

Every `python3` below assumes that activation; without it the generator exits at its interpreter
guard rather than running on the unsupported system interpreter.

## Step 2 — Prove the oracle refactor is byte-neutral (before anything else)

Research D17 moves the BM25 transcription into `reference/xtref/`. The proof that the move changed
nothing is that Feature 001's fixtures regenerate identically:

```bash
# The 001 generator requires --out under the repo root (it prints paths relative to it).
reference/.venv-002/bin/python reference/gen_001_fixtures.py --seed 1 --out "$PWD/target/xt001-check/"
diff <(jq -S '.files | del(."ranking.json")' target/xt001-check/manifest.json) \
     <(jq -S '.files | del(."ranking.json")' reference/fixtures/001/manifest.json)
```

Expected: empty diff over the five Python-generated files — `bm25_reference.json` is the one that
exercises the moved code. `ranking.json` is excluded because the generator writes an empty
placeholder for it and the committed file is Rust-minted (`provenance: host-tantivy-run`); it never
went through the refactored code. **Any other difference stops the work** — the refactor changed an
oracle (Rule 6). *(Run 2026-09-12: empty diff.)*

## Step 3 — Generate the 002 fixtures

```bash
python3 reference/gen_002_fixtures.py --seed 2 --out reference/fixtures/002/
git status --short reference/fixtures/002/   # schema, corpus, queries, filters, stats, mutations, manifest
```

Re-running the generator preserves already-minted `expected` rankings for entries whose query,
filter and `k` are unchanged, so a fixture tweak never silently un-mints `queries.json`.

The generator refuses to emit a `Term` golden without a genuine tie at the k-boundary and refuses a
`Phrase` golden whose phrase occurs only once — both are planted deliberately (data-model,
`FixtureCorpus`). Ranking goldens are minted afterwards by the Rust example and cross-checked:

```bash
cargo run -p xtriever-lexical --example gen_ranking -- --fixtures reference/fixtures/002/ [--force]
python3 reference/gen_002_fixtures.py --verify-ranking reference/fixtures/002/    # Python ≈ Rust within 1e-5
python3 reference/gen_002_fixtures.py --refresh-manifest reference/fixtures/002/  # re-hashes; refuses if verify fails
```

(Minting needs the implementation, so this step is re-run at the end of PR 3 — see Step 5.)

## Step 4 — Confirm the red state (Rule 4)

After PR 1 (fixtures + tests, no implementation):

```bash
cargo nextest run -p xtriever-lexical 2>&1 | tail -30
```

Expected: `fixtures_valid` **passes** (hashes match), every other test **fails**, and every failure
message names a missing implementation rather than a fixture problem (SC-010). A fixture-caused
failure at this checkpoint is a PR 1 bug.

## Step 5 — Run the acceptance suite (after each implementation PR)

```bash
cargo nextest run -p xtriever-lexical
cargo nextest run -p xtriever-lexical -- --ignored   # the FR-015 / FR-025 divergence measurement, prints DivergenceRecord
```

Per story, the tests that must be green:

| story | test files |
|---|---|
| 1 — ranked results | `index_query.rs` (goldens per query shape, boost, unknown analyzer, k = 0, empty result) |
| 2 — ties & segments | `determinism.rs` (single vs multi-batch, before/after `merge`, k-boundary tie, repeat call) |
| 3 — replace & delete | `mutation.rs` |
| 4 — filters | `filters.rs` (goldens) + `filter_algebra_prop.rs` (proptest, ≥ 1,000 cases per property) |
| 5 — statistics | `stats.rs` + the divergence measurement |
| 6 — shared under a lock | `concurrency.rs` (threads over `RwLock<TantivyIndex>`, two handles on one dir) |
| invariants | `analyzer_prop.rs` (determinism), `roundtrip_prop.rs` (add → commit → reopen) |

## Step 6 — The full local gate (Rule 5)

```bash
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets
cargo nextest run --workspace
cargo deny check                                           # bans still deny onig_sys; no new ignores
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target aarch64-apple-ios-sim
cargo check --workspace --target aarch64-linux-android
cargo check --workspace --target wasm32-unknown-unknown    # expected to FAIL (mmap/threads) — tracked, not blocking
./scripts/check-no-stubs.sh
```

Plus the feature's own purity assertion — zero C/C++ in the graph reachable from the crate:

```bash
cargo tree -p xtriever-lexical -e normal --prefix none | grep -Ei '(-sys|^cc |onig|zstd)' && echo "C DEP FOUND" || echo "pure"
```

## Step 7 — What goes in each PR description

- The nextest summary for `xtriever-lexical`.
- After PR 4: the `DivergenceRecord` table (FR-025) verbatim, whichever way it came out.
- The eval/bench delta line reads **"N/A — see ADR-0006"** until `xtriever-eval` exists. Do not omit
  the line; its presence is what shows the gate was considered.

## What "done" looks like

- SC-001…SC-013 each have a test that names them.
- `report.md` carries the divergence measurement and any findings.
- ADR-0005 resolution recorded; `xtriever-core` untouched (the amendment was superseded — the tie-break holds at the boundary).
- ADR-0006's three conditions honoured (baseline commit in `report.md`, N/A line in every PR).
