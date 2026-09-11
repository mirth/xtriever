# ADR-0002: Allow one `unsafe` block outside SIMD kernels to measure mmap'd weight loading

- **Status**: Accepted — 2026-09-10
- **Date**: 2026-09-10
- **Deciders**: mirth (repository owner), 2026-09-10
- **Spec**: [001-ios-build-spike](../../specs/001-ios-build-spike/spec.md)
- **Blocks**: Principle VII gate in [plan.md](../../specs/001-ios-build-spike/plan.md)

## Context

Principle VII states: "`unsafe` only in `xtriever-dense` SIMD kernels, each block preceded by
`// SAFETY:` and tested against the safe path." `Cargo.toml` enforces this workspace-wide with
`unsafe_code = "deny"`, and `xtriever-core` additionally carries `#![forbid(unsafe_code)]`.

Feature 001 must record peak memory footprint on a physical iPhone against a 300 MB ceiling
(FR-020). The bundled all-MiniLM-L6-v2 weights are **90,868,376 bytes** of fp32 safetensors — 87.1
MiB, or roughly 29% of the entire budget before a single document is indexed. How those bytes are
loaded is therefore the largest single lever on the verdict.

`candle-nn` 0.9.2 offers two loaders (read from
`candle-nn-0.9.2/src/var_builder.rs`):

```rust
pub unsafe fn from_mmaped_safetensors(...)                                   // line 642
pub fn from_buffered_safetensors(data: Vec<u8>, dtype, dev) -> Result<..>    // line 652
```

- The **safe** constructor takes a `Vec<u8>`, so all 87.1 MiB lands on the heap. Heap pages are
  dirty and count fully toward `phys_footprint`, which is the metric iOS enforces (Apple, WWDC22
  10106: "Memory footprint … is used for memory limit enforcement").
- The **mmap** constructor maps the file. Clean, file-backed pages are accounted differently, and may
  largely not count toward the footprint. At a 300 MB ceiling with 87.1 MiB at stake, the difference
  plausibly decides whether the spike passes or fails.

Whether that difference is real on iOS is **not knowable from documentation** — Apple does not
guarantee that `phys_footprint` equals the value jetsam compares against, and publishes no
per-device limits (research risk R7). It has to be measured, and measuring it requires calling the
`unsafe` constructor.

## Decision

Permit exactly one `unsafe` block in `xtriever-ffi`, outside a SIMD kernel, to call
`VarBuilder::from_mmaped_safetensors` as a **secondary, measured** weight-loading path.

Conditions, all of which are part of the decision and not optional:

1. **The safe path is primary.** `from_buffered_safetensors` is the default; the spike's correctness
   assertions run against it.
2. **Scope is one block.** A single `unsafe { … }` around the constructor call, inside one function
   whose only job is that load, under an explicit `#[allow(unsafe_code)]` scoped to that item — not
   to the crate, and never to a module.
3. **`// SAFETY:` comment required**, stating the actual invariant: the mapped file must not be
   modified or truncated for the lifetime of the `VarBuilder`, which holds because the weights are a
   read-only resource inside the signed application bundle.
4. **Tested against the safe path**, literally as Principle VII requires: an acceptance test asserts
   both loaders produce the same embedding for the fixture sentence, bit-for-bit. If they diverge,
   the mmap path is a finding and is dropped, not accommodated.
5. **It never reaches the pure crates.** `xtriever-core`'s `#![forbid(unsafe_code)]` and the other
   four pure crates are untouched.
   > **Amended 2026-09-10 by [ADR-0003](./0003-uniffi-scaffolding-requires-unsafe-allow.md).**
   > This condition originally implied the mmap call would be the only `unsafe` in the crate. That
   > is not achievable: uniffi's macros emit `unsafe` scaffolding, so `xtriever-ffi` carries a
   > boundary-scoped `#![allow(unsafe_code)]`. The invariant is now that `src/spike/` contains
   > exactly **one hand-written** `unsafe` block — zero before PR 2 — verified by
   > `grep -rn unsafe crates/xtriever-ffi/src/spike/`.
6. **It expires with the spike.** FR-031 declares the binding provisional. If the production
   `xtriever-dense` later wants mmap'd weights, that needs its own ADR with its own justification —
   this one authorises a measurement, not a pattern.

## Consequences

**Positive**

- Turns a Principle VII argument into a number. The plan does not have to guess whether mmap helps;
  the spike reports it.
- Condition 4 makes the deviation self-checking: the unsafe path has to agree with the safe path or
  it is discarded.
- If mmap does keep the weights out of the footprint, this materially changes what on-device
  configurations are feasible — knowledge worth far more than the spike itself.

**Negative**

- It establishes that `unsafe` can appear outside `xtriever-dense` SIMD kernels. That is a precedent,
  and condition 6 exists specifically to contain it.
- `#[allow(unsafe_code)]` in a leaf crate is easy to copy without copying the conditions.
- The measurement may show no benefit, in which case the deviation bought only a negative result —
  still worth recording, but the `unsafe` should then be deleted rather than left in place.

**Follow-up actions**

- If the measurement shows no material footprint difference, delete the mmap path and this
  deviation with it, and record that outcome in the spike report.
- If it does, the production decision belongs to the dense-stage spec, referencing this ADR's data.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Safe `from_buffered_safetensors` only | Leaves the largest memory lever in the spike unmeasured, in the one spike commissioned to measure memory. If the 300 MB verdict then failed, nobody would know whether mmap would have saved it. |
| Unsafe mmap path only | Undermines the Principle VII gate with no evidence it is needed, and removes the safe reference the correctness assertion depends on. |
| Put the `unsafe` in `xtriever-dense` so it is at least in the named crate | The letter of Principle VII restricts `unsafe` in `xtriever-dense` to **SIMD kernels**, so this would still be a deviation — while additionally leaving throwaway spike code squatting in a crate whose real spec has not landed (see the plan's layering entry). |
| Amend Principle VII to permit `unsafe` for FFI and memory-mapping generally | A governance amendment needs its own PR, ADR, human approval and a version bump. Far too broad a change to make on the basis of one spike's measurement need. Revisit once there is evidence from more than one case. |
| Load the weights via a different crate that offers a safe mmap API | Would add a dependency outside Principle I's named set purely to dodge a keyword, and `safetensors`' own mmap API is `unsafe` for the same legitimate reason: the borrow checker cannot see external file mutation. |
