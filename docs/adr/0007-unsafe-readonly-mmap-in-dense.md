# ADR-0007: One hand-written `unsafe` block in `xtriever-dense` for read-only memory mapping, behind a non-default feature

- **Status**: Accepted — 2026-09-12 (option (a) chosen: constitution amended to v1.2.0)
- **Date**: 2026-09-12
- **Deciders**: mirth (repository owner), 2026-09-12
- **Spec**: [004-dense-stage](../../specs/004-dense-stage/spec.md) FR-008
- **Blocks**: Principle VII gate in [plan.md](../../specs/004-dense-stage/plan.md)
- **Amended by**: [ADR-0013](./0013-dense-format-v2-append-tombstone-compact.md) — condition 2's
  "never modifies `index.bin` in place, only replaces it" became, for dense format version 2,
  "never modifies a mapped byte: the row file is only extended beyond every mapping or replaced
  by rename" (Feature 024)
- **Supersedes in part**: [ADR-0002](./0002-unsafe-mmap-safetensors-measurement.md) condition 6
  ("if the production `xtriever-dense` later wants mmap'd weights, that needs its own ADR") — this
  is that ADR.

## Context

Principle VII as it stood at v1.1.0: "Hand-written `unsafe` only in `xtriever-dense` SIMD kernels, each block
preceded by `// SAFETY:` and tested against the safe path." The workspace lint is
`unsafe_code = "deny"`.

Feature 004 productionises the embedder Feature 001 spiked and adds a flat vector index. Two of its
artifacts are large read-only files: the 90,868,376-byte fp32 weights and the vector index
(88.5 MB at FiQA's 57,638 rows; ~154 MB at the constitution's 100k-chunk configuration). Reading
either into a heap buffer is the safe path; mapping it needs `memmap2::MmapOptions::map`
(`memmap2-0.9.11/src/lib.rs:429`), which is `unsafe` because the borrow checker cannot see
external modification of the underlying file.

What mapping actually buys, established in [research D1](../../specs/004-dense-stage/research.md)
and correcting Feature 001's headline:

- **Weights**: candle 0.9.2 copies every tensor onto the heap whichever loader is used
  (`candle-core-0.9.2/src/safetensors.rs:115-137` → `Tensor::from_slice`). Mapping therefore
  removes only the *transient* `Vec<u8>` of the file during load — roughly halving the peak
  during `BertModel::load` — and changes nothing about steady state. Feature 001's "2.56 MB vs
  101 MB" was measured second in the same process after the buffered path had freed its buffer;
  the report flagged the ordering caveat, and D1 explains the number.
- **Vector index**: our own code reads rows straight from the mapped bytes, so mapping keeps the
  vectors as clean, evictable, file-backed pages rather than dirty heap. On iOS that is the
  difference between counting toward `phys_footprint` and (largely) not — the property ADR-0002
  set out to test and could not, because the weights are copied. This benefit is structural.

The user chose (spec Q1 = C) to ship both paths behind one Cargo feature: safe by default, mapping
opt-in, both measured.

## Decision

Permit **exactly one** hand-written `unsafe` block in `xtriever-dense`, outside a SIMD kernel,
compiled only under the non-default feature `mmap`:

```rust
// crates/xtriever-dense/src/bytes.rs
#[cfg(feature = "mmap")]
#[allow(unsafe_code)]
pub(crate) fn map_readonly(file: &std::fs::File) -> std::io::Result<memmap2::Mmap> {
    // SAFETY: … (condition 2)
    unsafe { memmap2::MmapOptions::new().map(file) }
}
```

used by both the weight loader (`VarBuilder::from_slice_safetensors(&map[..], …)`, the safe
constructor at `candle-nn-0.9.2/src/var_builder.rs:658`) and the vector index
(`FlatIndex::open_mapped`). Conditions, all part of the decision:

1. **Safe is the default.** `default = []`; a consumer that never enables `mmap` compiles zero
   `unsafe` from this crate. `LoadPath::Buffered` and `FlatIndex::open` are the primary paths and
   the paths the correctness oracles run against.
2. **One block, one function, item-scoped allow.** `#[allow(unsafe_code)]` on `map_readonly`
   only; never on a module or the crate. The `// SAFETY:` comment states the real invariant: the
   mapped file is not modified or truncated for the lifetime of the map. For the weights that
   holds because they are a read-only, hash-verified resource; for the index because the writer
   **never modifies `index.bin` in place** — it writes `index.bin.tmp` and `rename`s, so a mapped
   inode stays intact (research D8). Those are the parts this crate's own code guarantees. What
   no code can guarantee — that no *other* process modifies or truncates the file while the map
   lives — is stated as the caller's precondition on `LoadPath::Mmap` and every `open_mapped*`
   constructor, exactly as tantivy's `MmapDirectory` (already admitted by Principle I) does
   behind its safe API. That inherent limit is why the path is opt-in rather than the default
   (review round 1, comment 1).
3. **Tested against the safe path, bit-for-bit.** `tests/load_paths.rs` (buffered vs mapped
   embeddings over the golden set) and the index suite run twice (`open` vs `open_mapped`) must
   agree to the bit. Disagreement is a defect in the mapped path and is fixed or the path is
   dropped — never accommodated.
4. **No new dependency class.** `memmap2` is already in `Cargo.lock` (via candle); it is added as
   an optional direct dependency with `cargo add`. `deny.toml` is unchanged.
5. **Measured from cold, with a deletion clause for the weights.** FR-023 measures each load
   path's peak RSS in its own process. If `buffered_peak − mapped_peak < 0.10 × buffered_peak`, the
   `LoadPath::Mmap` variant is deleted in the same feature and the report says why; the index
   mapping stays regardless, since its benefit (item above) does not depend on this number.
   ADR-0002 imposed the same discipline on the spike.
6. **The Principle VII wording is reconciled by amendment** — option (a) below, chosen by the
   repository owner on 2026-09-12; (b) is kept for the record of what was rejected:
   - **(a) Amend the constitution to v1.2.0** (MINOR: principle expanded). Replace the first
     sentence of the `unsafe` bullet with: "Hand-written `unsafe` only in `xtriever-dense`, and
     there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each
     block preceded by `// SAFETY:` and tested against the safe path (ADR-0007)." The rule then
     reads true of the codebase, as the v1.1.0 amendment did for uniffi. **Recommended**: the
     mapping is not an experiment, it is the mechanism the on-device memory budget will rely on,
     and a standing deviation listed in every future dense plan is the wrong shape for that.
   - **(b) ADR-scoped exception**, no amendment: the Principle VII row stays justified-by-ADR in
     this and every later plan touching the block, and Complexity Tracking carries the row.

## Consequences

**Positive**

- The on-device index footprint lever (evictable pages) exists in production code with a
  self-checking parity test, and the "mmap helps" claim is finally measured from cold.
- A default consumer gets a crate with no `unsafe` at all.

**Negative**

- Two code paths to keep bit-identical (mitigated: they share every line after the bytes are
  obtained).
- The feature flag is a choice every consumer must make; the pipeline feature will make it once.
- Condition 5 may delete the weights-mmap path a day after it lands — that is the intended outcome
  of measuring rather than assuming.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Safe buffered only (spec Q1 option A) | Leaves the index's evictable-pages lever unavailable to the on-device feature and the from-cold weight measurement unmade; the user chose otherwise. |
| `unsafe` path only (no feature) | Every consumer compiles `unsafe`; no safe reference for the parity test. |
| `VarBuilder::from_mmaped_safetensors` (candle's own `unsafe fn`) | Would be a second `unsafe` call site for no benefit — the safe slice constructor takes our map; one block covers both artifacts. |
| A safe-mmap wrapper crate | Hides the same invariant behind someone else's `unsafe`, adds a dependency outside Principle I's set, and the invariant (no external modification) is ours to state anyway. |
| Copy-free tensors from the map | Not available on candle 0.9.2's CPU device (`Tensor::from_slice` copies). A different runtime is a Principle I decision with its own ADR. |

## Outcome

Option (a) was taken on 2026-09-12: Principle VII now reads "Hand-written `unsafe` only in
`xtriever-dense`, and there only in SIMD kernels and in read-only memory mapping behind a
non-default feature; each block preceded by `// SAFETY:` and tested against the safe path
(ADR-0007)" — constitution **v1.2.0** (MINOR: principle expanded; the SIMD rule is unchanged and
one further case is admitted, as v1.1.0 did for uniffi scaffolding). `CLAUDE.md` and the plan
template carry the new wording. Conditions 1–5 remain in force as the terms under which the
amendment applies; condition 5's measurement is owed by Feature 004's report.

**Condition 5, measured 2026-09-12** (Feature 004 T043/T044; Apple M1 Pro, macOS 25.6,
`RAYON_NUM_THREADS=4`, `/usr/bin/time -l` on `beir model-memory`, three fresh processes per
path): peak RSS buffered **222.8 / 226.3 / 226.7 MB**, mapped **193.1 / 197.4 / 197.5 MB** —
a difference of ~29 MB, **13 % of the buffered peak**, above the 10 % threshold. **The weights-mmap
path stays.** The saving is a third of the 90.9 MB file buffer it avoids, not all of it, because
macOS counts touched file-backed pages in RSS and candle touches every page while copying the
tensors onto the heap (research D1); steady state is identical between the paths. The number is
recorded in `specs/004-dense-stage/baselines/dense-baseline-v1.fiqa.json` (`observations`) and
the feature report.

**Extended by [ADR-0009](./0009-unsafe-readonly-mmap-in-rerank.md)** (2026-09-13): the same block,
under the same conditions 1–4, is admitted in `xtriever-rerank` for the cross-encoder's weights;
constitution v1.3.0 names both crates. "Exactly one block in `xtriever-dense`" still holds for
this crate.
