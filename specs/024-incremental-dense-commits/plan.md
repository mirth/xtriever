# Implementation Plan: Incremental Dense Commits — an Append/Tombstone/Compact Vector File

**Branch**: `024-incremental-dense-commits` | **Date**: 2026-09-18 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/024-incremental-dense-commits/spec.md`

## Summary

Dense format version 2 replaces the single rewritten `dense/index.bin` with two files: an
append-only row file `vectors.<generation>.bin` (`id, norm, vector` per row, in commit order)
and an atomically replaced `manifest.bin` (magic, JSON header with the committed row count
and the current generation, then the tombstone set as a serialised `roaring` bitmap).
`commit` appends the pending rows, marks superseded and deleted rows dead, and renames a new
manifest; `compact` (new, inherent on `FlatIndex`) writes the live rows in ascending id order
to `vectors.<generation+1>.bin` and switches the manifest — the current `commit` algorithm,
moved. The pipeline's `merge` calls `compact`; a new optional `dense_compact_dead_share` in
`HybridConfig` (and the FFI `IndexConfig`, additive, default `None`) makes `commit` compact
when the dead share crosses it. Version 1 is refused at open; the fixture index and the
shipped Wikipedia artefact are regenerated with unchanged vectors and must pass their
committed goldens. Research D1–D10 in [research.md](./research.md); two PRs (Rule 3).

## Technical Context

**Language/Version**: Rust, edition 2024, the pinned toolchain. **Primary Dependencies**:
`roaring` (already a workspace dependency used by `xtriever-core`; added to
`xtriever-dense` with `cargo add roaring -p xtriever-dense` at the workspace version), `memmap2` (present, feature `mmap`), `serde_json` (present); dev: `criterion`
(new, `cargo add --dev criterion -p xtriever-dense`, the workspace's first bench),
`proptest` and `tempfile` (present). **Storage**: `dense/manifest.bin` +
`dense/vectors.<gen>.bin` (D2, D3). **Testing**: the dense crate's existing golden /
mutation / persist / prop / errors suites re-pointed at version 2 plus new tests: bytes
written, truncation at every point, compaction, the threshold, version-1 refusal; a
criterion bench for the scan and the commit; the pipeline's merge-determinism tests
extended; the fixture goldens and the host goldens as the regeneration proof.
**Target Platform**: host + iOS / iOS-sim / Android checks; wasm best-effort (the dense
crate is not a pure crate, unchanged). **Project Type**: library (stage crate) + pipeline
+ FFI. **Performance Goals**: unfiltered scan within 5 % of version 1 at 100k × 384;
10-row commit into 100k rows under 100 KB written and 50 ms (SC-001, SC-004).
**Constraints**: no `xtriever-core` change; no `deny.toml` change; no thread, no timer;
the mmap SAFETY argument amended, not weakened (D4). **Scale/Scope**: `xtriever-dense`
format + index (~700 lines changed), tests (~450 new), bench (~80), ADR-0013; pipeline +
FFI (~120); ~1,350 lines → two PRs.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Authority: `.specify/memory/constitution.md` (v1.4.0, ratified 2026-09-10, last amended 2026-09-13). Every row below MUST be
marked **PASS**, **FAIL**, or **N/A** with a one-line justification — an empty verdict is a FAIL.

- Any **FAIL** blocks the phase. Do not proceed, and do not weaken the gate to make it pass.
- Any **N/A** MUST state why the principle cannot apply to this feature.
- A violation that is genuinely required is recorded in Complexity Tracking below and REQUIRES an
  accepted ADR in `docs/adr/`; the row stays **FAIL** until that ADR is merged.
- These are plan-time gates. They do not replace the blocking CI gates in the constitution's
  "Quality Gates (CI)" table.

### Core Principles

| # | Principle | Gate | Verdict | Justification |
|---|---|---|---|---|
| I | Reuse Before Build | Commodity components come from `tantivy`, `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR. | PASS | The bitset is `roaring`; the mapping `memmap2`. The survey (D1) found no crate offering an exact, contiguous, mappable, appendable f32 matrix — `vecstore` rewrites whole files; KV stores and columnar formats lose the contiguous scan; ANN crates change results. A flat f32 file is not on the principle's "do not build" list (inverted index, ANN graph, tensor runtime). |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests first, committed failing; reference behaviour pinned to goldens at stated tolerances; ranking work reports eval deltas. | PASS | Tests first (quickstart Step 1). The oracle is the current implementation: search goldens minted from version 1 on scripted sequences (exact bits) and a property test comparing both implementations on random sequences; the 004 search goldens (`reference/fixtures/004`) stay; the regenerated fixture index must reproduce its committed `expected.json` bit for bit; the host goldens 800/800. No ranking change — no eval deltas (bit-identity is the stronger claim; the SciFact hybrid baseline is re-run once as a check, D9). |
| III | Portability Is a Feature | Pure crates stay pure; cross-target checks pass; on-device RSS under the ceiling. | PASS | `xtriever-dense` is not a pure crate and gains no new C dep; `roaring` is pure Rust and already in `xtriever-core`. Compaction on a threshold is synchronous — no thread, no timer. A mapped version-2 index maps the same number of bytes as version 1 (one row file + a small manifest); RSS is measured on the Wikipedia artefact (D9). |
| IV | Measured, Not Asserted | Performance claims backed by benchmarks/records; reproducible: model revisions pinned by SHA. | PASS | The first `criterion` bench in the workspace: the unfiltered scan (v1 vs v2, 100k × 384, fixed seed) and the 10-row commit (bytes, time); results in the PR against the spec's budgets. |
| V | Small, Explicit Interfaces | Core traits, on-disk format, error semantics unchanged — or human review plus an ADR. | PASS with ADR | The `VectorIndex` trait is unchanged (FR-009). The dense on-disk format changes → **ADR-0013** (`docs/adr/0013-dense-format-v2-append-tombstone-compact.md`), owner-approved by the spec's decisions (Q1 = C, Q2 = C). Error semantics unchanged: a version-1 file is refused by the existing `Corrupt` version error. `HybridConfig` and the FFI `IndexConfig` gain one optional field with a default — additive. |
| VI | Graceful Degradation and Determinism | Degrade not error unless strict; identical results for identical inputs. | PASS | Results are bit-identical to version 1 by construction (per-row `f64` accumulation, total order) and by test. A crash leaves the previous committed state (manifest last, rename); a half-state or a version mismatch is a hard error at open, never silent. No fallback. |
| VII | Rust Hygiene | Toolchain, lints, `cargo add`, no `unwrap` in libraries, `unsafe` confined. | PASS | `cargo add` for `roaring` and `criterion`; no new `unsafe` — the one block stays in `bytes.rs`, its SAFETY comment amended for "extend past every live mapping's end" (D4, ADR-0013 supersedes the relevant sentence of ADR-0007's condition 2). |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external item read from the pinned docs and cited. | PASS | `roaring::RoaringBitmap::{insert, contains, serialize_into, deserialize_from, serialized_size, len}` (0.10, D5); `memmap2::MmapOptions::map` (present); `std::fs::File::{set_len, sync_all}`, `OpenOptions::append`; `criterion::{criterion_group, criterion_main, Criterion, BenchmarkId}` (D8). |
| 2 | Don't touch the contract uninvited | No `xtriever-core` trait or `deny.toml` change. | PASS | Neither touched; compaction is inherent on `FlatIndex`, which the pipeline holds concretely. (After `/code-review`, `xtriever-core` gained an additive `fs` module and `Error::read_only()` — helpers, not a trait, format or error-variant change — at the owner's "fix all" instruction.) |
| 3 | One spec, small PRs | Scoped to this spec, under ~800 lines — or the split stated. | PASS | Two PRs: **A** = `xtriever-dense` format v2, tests, bench, ADR-0013, fixture regeneration (~900 lines, mostly tests); **B** = pipeline `merge` → `compact`, the threshold in `HybridConfig` / descriptor / FFI, docs, the Wikipedia artefact regeneration record (~350). |
| 4 | Tests first | Acceptance tests committed failing first. | PASS | Quickstart Step 1; goldens minted from version 1 *before* the format changes (they are the oracle). |
| 5 | Full gate before done | The local gate run and deltas pasted. | PASS | Quickstart Step 6; bench numbers and the SciFact check in the PR. |
| 6 | Never weaken the oracle | Stop and report on a drop. | PASS | A single differing bit against the goldens, a truncation point that does not recover, a bench over budget, or a fixture `expected.json` that no longer reproduces is stop-and-report. |
| 7 | Prefer boring code | No macros, trait gymnastics, premature generics. | PASS | Two files, fixed-size rows, one bitmap, one manifest; the scan loop is the current one with a `contains` check. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-18, the agent (Principle V carries the ADR obligation).

**Post-design gate (post-Phase 1)**: PASS — 2026-09-18, the agent (design D1–D10; ADR-0013 written in PR A).

## Project Structure

### Documentation (this feature)

```text
specs/024-incremental-dense-commits/
├── plan.md · research.md · data-model.md · quickstart.md
├── contracts/dense-format-v2.md            # the file formats and the commit / compact protocols
├── runs/                                   # bench results; the regeneration records
├── report.md · pr-description-a.md · pr-description-b.md
└── tasks.md
docs/adr/0013-dense-format-v2-append-tombstone-compact.md
```

### Source Code (repository root)

```text
crates/xtriever-dense/
├── Cargo.toml                    # + roaring (workspace version); [dev] criterion; [[bench]] scan
├── src/lib.rs                    # FORMAT_VERSION = 2; docs
├── src/bytes.rs                  # SAFETY comment amended (D4)
├── src/index/format.rs           # v2: manifest (magic XTDENSE2 + header + roaring), row layout, decode/encode
├── src/index/mod.rs              # FlatIndex: open (truncate-guard, stale-file sweep), add/delete, commit (append), compact, search (skip dead), vector, len; compaction threshold
├── src/index/search.rs           # unchanged
├── benches/scan.rs               # new: v1-shaped vs v2 scan; the 10-row commit
└── tests/                        # index_golden / mutation / persist / prop / errors updated; index_append.rs, index_compact.rs, index_crash.rs new; support/ (v1 goldens)
crates/xtriever-pipeline/src/
├── types.rs                      # HybridConfig.dense_compact_dead_share: Option<f32> (validated 0..=1)
├── descriptor.rs                 # #[serde(default)] dense_compact_dead_share
└── index.rs                      # create/open pass the threshold; merge → dense.compact(); tests
crates/xtriever-ffi/src/ffi/types.rs   # IndexConfig.dense_compact_dead_share: Option<f32>, #[uniffi(default = None)]
crates/xtriever-ffi/examples/fixture_index.rs   # unchanged; re-run to regenerate the fixture
```

Not touched: `crates/xtriever-core`, `deny.toml`, `apps/`, `reference/fixtures`, baselines,
`python/src`, `swift/` sources (the uniffi-generated bindings pick the field up).

**Structure Decision**: the change stays inside `xtriever-dense` (PR A) with a thin
plumbing layer in the pipeline and FFI (PR B). Dependencies still point downward:
`core` ← `dense` ← `pipeline` ← `ffi`.

## Complexity Tracking

| Violation | Principle / Rule | Why Needed | Simpler Alternative Rejected Because | ADR |
|-----------|------------------|------------|--------------------------------------|-----|
| Dense on-disk format v1 → v2 | V | A commit must not rewrite the whole vector file (the spec's problem) | Keeping v1 and appending is impossible: v1 is columnar (ids, norms, vectors as three contiguous sections), so nothing can be appended without rewriting | `docs/adr/0013-dense-format-v2-append-tombstone-compact.md` (PR A) |
