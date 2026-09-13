# ADR-0009: The same one read-only memory-mapping `unsafe` block in `xtriever-rerank`, behind a non-default feature

- **Status**: Accepted — 2026-09-13 (constitution amended to v1.3.0)
- **Date**: 2026-09-13
- **Deciders**: mirth (repository owner), 2026-09-13
- **Spec**: [006-rerank-stage](../../specs/006-rerank-stage/spec.md) FR-003
- **Blocks**: Principle VII gate in [plan.md](../../specs/006-rerank-stage/plan.md)
- **Extends**: [ADR-0007](./0007-unsafe-readonly-mmap-in-dense.md) — same block, same conditions,
  second crate

## Context

Principle VII at v1.2.0: "Hand-written `unsafe` only in `xtriever-dense`, and there only in SIMD
kernels and in read-only memory mapping behind a non-default feature". ADR-0007 admitted exactly
one such block (`bytes::map_readonly`) for the embedder's weights and the vector index, and
condition 5 measured what mapping the weights buys: **13 % of the load-time peak (~29 MB)**,
nothing at steady state, because candle 0.9.2 copies every tensor onto the heap.

Feature 006 loads a second model of the same size (`cross-encoder/ms-marco-MiniLM-L-6-v2`,
90,870,598 bytes of `F32` safetensors) in a second stage crate, `xtriever-rerank`. Its spec
(FR-003) asks for the same two load paths as the dense stage. Under v1.2.0 that is impossible
without one of:

- **(a)** amending the constitution to admit the same block in `xtriever-rerank` — this ADR;
- **(b)** a sideways dependency `xtriever-rerank → xtriever-dense` to reuse `map_readonly`,
  which is `pub(crate)`; exposing it modifies `xtriever-dense` (forbidden by the spec's FR-022)
  and bends Principle V's `core ← stage crates` direction into stage ← stage;
- **(c)** buffered-only loading in the re-rank crate (the plan's original recommendation,
  research D3 as first drafted).

The repository owner chose **(a)** on 2026-09-13, with (c) as the recorded alternative: the two
stage crates should be symmetrical, and an on-device deployment that loads both models pays the
transient buffer twice at start-up — the mapping is the mechanism the on-device budget will use,
and it should exist for both models before that feature measures it.

## Decision

Permit in `xtriever-rerank` **exactly one** hand-written `unsafe` block, the same function as
ADR-0007's, compiled only under the non-default feature `mmap`:

```rust
// crates/xtriever-rerank/src/bytes.rs
#[cfg(feature = "mmap")]
#[allow(unsafe_code)]
pub(crate) fn map_readonly(file: &std::fs::File) -> std::io::Result<memmap2::Mmap> {
    // SAFETY: … (ADR-0007 condition 2, restated for the weights only)
    unsafe { memmap2::MmapOptions::new().map(file) }
}
```

used by the weight loader only (`VarBuilder::from_slice_safetensors(&map[..], …)`, the safe
constructor). ADR-0007's conditions apply unchanged:

1. **Safe is the default.** `default = []`; `LoadPath::Buffered` is the primary path and the one
   the goldens run against.
2. **One block, one function, item-scoped allow, real invariant.** The mapped file is the
   hash-verified, read-only weights; the crate never writes it; the external-writer precondition
   is the caller's, documented on `LoadPath::Mmap`.
3. **Tested against the safe path, bit-for-bit** (`tests/load_paths.rs` over the golden set).
4. **No new dependency class** — `memmap2` optional, added with `cargo add`; `deny.toml` unchanged.
5. **Measured from cold** (`beir model-memory --model rerank --load-path buffered|mmap`, three
   fresh processes each). The deletion clause is **not** re-applied: ADR-0007 already measured
   this exact model family and kept the path at 13 %; a second measurement is recorded for the
   record, and the path stays regardless, because the decision here is symmetry with the dense
   stage, not a new saving.
6. **Principle VII is amended** (MINOR, v1.2.0 → v1.3.0): "Hand-written `unsafe` only in
   `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory
   mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against
   the safe path (ADR-0007, ADR-0009)." The two-word change admits one further crate for the
   same block; nothing else about the rule moves.

The block is duplicated rather than shared: a shared `xtriever-unsafe-io` helper crate would be a
new crate outside the stage/pipeline structure for eleven lines, and a `core` home would put
`unsafe` in the one crate the constitution most wants free of it.

## Consequences

**Positive**

- The two model-loading crates have the same shape, feature name, `LoadPath` type and parity
  test; the on-device feature meets one contract twice.
- A default consumer still compiles zero `unsafe` from either crate.

**Negative**

- Two identical `unsafe` blocks to keep in step (mitigated: eleven lines, same ADR conditions,
  the containment grep in each plan's gate lists both files).
- The constitution names crates rather than a property; a third model crate would need a third
  two-word amendment. Acceptable: each amendment is a deliberate, reviewed act, which is the point.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| (c) buffered-only in the re-rank crate | Asymmetric stage crates; the on-device start-up peak would carry one model's transient buffer; the owner chose symmetry |
| (b) reuse `xtriever-dense::map_readonly` | Modifies the dense crate (FR-022), creates a stage-to-stage dependency |
| A shared helper crate | A new crate outside Principle V's structure for one function |
| Put the block in `xtriever-core` | The crate the constitution most wants `unsafe`-free would carry the only block |

## Outcome

Constitution amended to **v1.3.0** on 2026-09-13 (`.specify/memory/constitution.md`; `CLAUDE.md`
and the plan template carry the wording). Condition 5's measurement is owed by Feature 006's
report.
