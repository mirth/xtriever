# ADR-0003: uniffi scaffolding requires `#![allow(unsafe_code)]` in `xtriever-ffi`

- **Status**: Accepted — 2026-09-10
- **Date**: 2026-09-10
- **Deciders**: mirth (repository owner), 2026-09-10
- **Spec**: [001-ios-build-spike](../../specs/001-ios-build-spike/spec.md)
- **Relates to**: [ADR-0002](./0002-unsafe-mmap-safetensors-measurement.md) (amended by this ADR)
- **Outcome**: drove the **constitution amendment to v1.1.0** (2026-09-11), which expanded
  Principle VII to admit this case. The containment design below is therefore no longer a
  documented deviation — it *is* the rule. This ADR is the amendment's supporting ADR, as
  Governance requires.

## Context

Principle VII states: "`unsafe` only in `xtriever-dense` SIMD kernels, each block preceded by
`// SAFETY:` and tested against the safe path." The workspace enforces it with
`unsafe_code = "deny"` in `[workspace.lints.rust]`, and `xtriever-core` goes further with
`#![forbid(unsafe_code)]`.

Feature 001 exposes three operations to Swift through `uniffi` 0.32.1. **Every uniffi entry point
generates `unsafe` code**, and the crate cannot compile under `unsafe_code = "deny"`.

Verified by reading `uniffi_macros-0.32.1` in the local registry — 38 emission sites across:

| macro | emits | source |
|---|---|---|
| `uniffi::setup_scaffolding!()` | `#[unsafe(no_mangle)] pub unsafe extern "C" fn` (×5) | `src/setup_scaffolding.rs:41-100` |
| `#[uniffi::export]` | `#[unsafe(no_mangle)] pub unsafe extern "C" fn` | `src/export/scaffolding.rs:242,281,320-321,342-343` |
| `#[derive(uniffi::Record)]` | `unsafe impl` | `src/record.rs:120` |
| `#[derive(uniffi::Enum)]` | `unsafe impl` | `src/enum_.rs:254` |
| `#[derive(uniffi::Error)]` | `unsafe impl` (×3) | `src/error.rs:92,119,144` |
| `#[uniffi::remote]` | `unsafe impl` | `src/remote.rs:37` |

The macros emit `#[allow(clippy::missing_safety_doc, missing_docs)]` on their output, so those two
lints are handled — but **not `unsafe_code`**. There is no uniffi feature or attribute that
suppresses it, and no way to write the FFI surface without these macros short of hand-writing the
entire C ABI, which would mean far more hand-written `unsafe`, not less.

This was discovered while planning the first implementation PR. It falsifies two claims already
accepted:

- **ADR-0002 condition 5** and its consequences section assume the single mmap block is the only
  `unsafe` the spike introduces.
- **[plan.md](../../specs/001-ios-build-spike/plan.md)**'s Principle VII row was marked PASS partly
  on that basis.

Both are corrected by this ADR rather than left to be discovered at the first `cargo check`.

## Decision

Permit `#![allow(unsafe_code)]` in `crates/xtriever-ffi`, scoped to a dedicated FFI boundary module,
for **compiler-generated code only**.

Conditions, all part of the decision:

1. **Containment by module, not by crate.** The allow lives at the crate root (`src/lib.rs`, which
   holds only `uniffi::setup_scaffolding!()` and module declarations) and in `src/ffi/`, which
   contains nothing but the uniffi-facing type definitions and thin delegating shims.
2. **`src/spike/mod.rs` re-declares `#![deny(unsafe_code)]`**, restoring the workspace guarantee for
   every module that contains hand-written logic. The relaxation does not propagate.
3. **`src/ffi/` contains no logic.** The exported functions delegate immediately to `src/spike/`.
   A reviewer can confirm the lint relaxation covers nothing but wire format and generated code.
4. **Hand-written `unsafe` in `src/ffi/` is forbidden.** The only hand-written `unsafe` anywhere in
   this crate is the single ADR-0002 block in `src/spike/embed.rs`, which is exempted from
   condition 2 by an item-scoped `#[allow(unsafe_code)]` and a `// SAFETY:` comment.
5. **It never reaches the pure crates.** `xtriever-core`'s `#![forbid(unsafe_code)]` and the other
   four `std`-only crates are untouched.

### Amendment to ADR-0002

ADR-0002 condition 5 and its "Negative" consequences are amended: the invariant is that
`src/spike/` contains exactly **one hand-written** `unsafe` block (zero before PR 2), not that the
crate contains one `unsafe` token. Generated scaffolding in `src/ffi/` is governed by this ADR.

The corresponding verification (tasks T060) becomes:

```sh
# ADR-0002: exactly one hand-written unsafe, and only in embed.rs
grep -rn 'unsafe' crates/xtriever-ffi/src/spike/

# ADR-0003: the relaxation is confined to the boundary
grep -rn 'allow(unsafe_code)' crates/xtriever-ffi/src/
```

## Consequences

**Positive**

- The crate compiles. Without this, `uniffi::setup_scaffolding!()` fails on its first `cargo check`
  and Feature 001 cannot proceed at all.
- The `ffi/` ÷ `spike/` split is a better structure independently of the lint. Because `mod ffi` is
  itself `#[cfg(feature = "spike")]`, **no `#[cfg]` ever appears next to a `#[uniffi::export]`** —
  which eliminates research risk R3 (uniffi ignores `#[cfg]` *inside* an export block and generates
  scaffolding anyway) by construction rather than by remembering a rule.
- The Principle VII invariant that actually matters — no unaudited `unsafe` in code we wrote — is
  preserved and is greppable.

**Negative**

- `xtriever-ffi` no longer satisfies Principle VII as literally written. Any future FFI crate will
  need the same exemption; this ADR is the precedent, and condition 1 is what keeps it from becoming
  a crate-wide licence.
- A reviewer skimming `src/lib.rs` sees `#![allow(unsafe_code)]` without immediate context. The
  attribute carries a comment citing this ADR and the specific macro emission sites.
- The workspace lint no longer protects this crate's boundary module, so condition 4 is enforced by
  review and by the greps above rather than by the compiler.

**Follow-up**

- If a future Principle VII amendment adds a general FFI carve-out, this ADR is superseded and the
  per-module allows can be removed.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Crate-wide `#![allow(unsafe_code)]` in `xtriever-ffi` | Simplest, but forfeits the guarantee for `src/spike/`, which is where all hand-written logic and the ADR-0002 mmap block live. That is precisely the code the lint should still be guarding. |
| Amend Principle VII to permit `unsafe` for FFI generally | A governance amendment needs its own PR, ADR, human approval and a MINOR version bump. Too broad on the evidence of one crate; revisit when a second case exists. |
| Widen the already-accepted ADR-0002 to cover this too | Conflates two unrelated things: our own hand-written `unsafe` with a real safety invariant, versus third-party generated code with none. It would also rewrite a decision already signed off. ADR-0002 is amended in one narrow respect instead. |
| Hand-write the C ABI instead of using uniffi | Vastly more hand-written `unsafe`, all of it ours to audit, to avoid a lint on generated code. Also contradicts Principle I (reuse) and the spec, which names uniffi. |
| Drop `unsafe_code = "deny"` from the workspace | Removes the protection from all ten crates to accommodate one. `xtriever-core`'s `#![forbid]` would still stand, but every stage crate would lose it. |
