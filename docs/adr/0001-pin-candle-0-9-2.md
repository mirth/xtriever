# ADR-0001: Pin candle to 0.9.2 rather than the current 0.11.0

- **Status**: Accepted — 2026-09-10
- **Date**: 2026-09-10
- **Deciders**: mirth (repository owner), 2026-09-10
- **Spec**: [001-ios-build-spike](../../specs/001-ios-build-spike/spec.md)
- **Blocks**: Principle VII gate in [plan.md](../../specs/001-ios-build-spike/plan.md)

## Context

Principle VII requires dependencies to be "added with `cargo add` at current versions — never from
memory". The current published candle release is **0.11.0**. Feature 001 needs
`candle-core`/`candle-nn`/`candle-transformers` to build for `aarch64-apple-darwin` (host, and the
macOS CI runner), `aarch64-apple-ios` and `aarch64-apple-ios-sim` on the pinned stable toolchain
**1.91.1**.

Two independent defects in candle ≥ 0.10 were found by measurement during Phase 0 (see
[research.md](../../specs/001-ios-build-spike/research.md) D3).

### Defect (a): a non-optional C dependency that `deny.toml` bans

`candle-core` 0.10.1 and 0.11.0 declare, per the crates.io dependency metadata:

```
candle-core 0.11.0 -> tokenizers req=^0.22.0 kind=normal optional=False
                      default_features=False features=['onig']
```

The dependency is **normal and non-optional**, and it **hard-codes the `onig` feature**. No candle
feature flag can switch it off. The resulting graph is:

```
onig_sys v69.9.3 (C, compiled with cc v1.4.5)
└── onig v6.5.3 └── tokenizers v0.22.2 └── candle-core v0.11.0
```

`deny.toml` denies `onig_sys` outright, with the comment "tokenizers: use the fancy-regex feature".
That comment assumed the only route to `onig` was our own `tokenizers` dependency; it is not. The
dependency also pins a second copy of `tokenizers` (0.22.2 alongside the 0.23.2 the spike uses
directly), tripping `multiple-versions`.

Note that `deny.toml` sets `[graph] all-features = true`, so gating candle behind a non-default
Xtriever feature would **not** hide `onig_sys` from `cargo deny check`.

Measured version boundary: 0.11.0 and 0.10.1 declare the dependency; **0.9.1, 0.8.4, 0.7.2 and 0.6.0
have no `tokenizers` dependency at all.**

### Defect (b): 0.11.0 does not compile on stable Rust

`cargo check` on toolchain 1.91.1:

| target | candle-core 0.11.0 | candle-core 0.9.2 |
|---|---|---|
| `aarch64-apple-darwin` (host, macOS CI) | **FAIL** | PASS |
| `aarch64-apple-ios` (device) | PASS | PASS |
| `aarch64-apple-ios-sim` | **FAIL** | PASS |

The failure is `error[E0658]: use of unstable library feature 'stdarch_neon_f16'` at
`candle-core-0.11.0/src/cpu/neon.rs:160,188,189` (`float16x8_t`), i.e. nightly-only
(rust-lang/rust#136306).

The mechanism, from the crate source: `neon.rs:71` opens `mod fp16`; `:81` is the portable path under
`#[cfg(not(target_feature = "fp16"))]`; `:153` is the `float16x8_t` path under
`#[cfg(target_feature = "fp16")]`. And `rustc --print cfg`:

| target | `neon` | `fp16` |
|---|---|---|
| `aarch64-apple-ios` | yes | **no** |
| `aarch64-apple-ios-sim` | yes | **yes** |
| `aarch64-apple-darwin` | yes | **yes** |

So the iOS *device* target compiles only because `fp16` happens to be absent there. Every
Apple-silicon developer machine, the macOS CI runner, and the simulator have `fp16` enabled and
therefore require nightly. This breaks the constitution's blocking `nextest on macOS` gate — it is
not merely an iOS problem.

## Decision

Pin `candle-core`, `candle-nn` and `candle-transformers` to **0.9.2** with default features
(`default = []`), for the duration of Feature 001.

Measured on the full proposed stack (tantivy 0.26.2 C-free set + tokenizers 0.23.2 + candle 0.9.2 +
uniffi 0.32.1): `cargo check` passes on host, device and simulator, and the graph contains no `cc`,
no `-sys` crate, no `zstd` and no `onig`.

This is a Principle VII deviation, recorded in the plan's Complexity Tracking. It held the Principle
VII gate row at FAIL until this ADR was accepted on 2026-09-10; that row now reads PASS.

## Consequences

**Positive**

- `cargo deny check` passes without any change to `deny.toml`, so Agent Operating Rule 2 is
  respected and no gate is weakened. This is the main reason to prefer the pin over the
  alternatives.
- The pinned stable toolchain in `rust-toolchain.toml` stays authoritative (Principle VII).
- The blocking macOS CI gate keeps working.
- The BERT API needed by the spike is present and was read from the 0.9.2 source directly:
  `candle_transformers::models::bert::{Config, BertModel, DTYPE}`, with
  `BertModel::forward(&self, input_ids, token_type_ids, attention_mask: Option<&Tensor>)`.

**Negative**

- Two minor versions of upstream fixes and performance work are forgone.
- The pin will age. Whatever Xtriever eventually needs from candle ≥ 0.10 is deferred, and the
  `onig` problem must be solved upstream before the project can move forward on candle at all.
- A future contributor running `cargo add candle-core` will silently get 0.11.0 and reintroduce both
  defects. The pin needs to be visible where that happens, not only in this ADR.

**Review triggers** — revisit this ADR when any of these becomes true:

1. **The next candle release after 0.11.0 ships.** Defect (b) is already fixed upstream: PR #3845,
   "Avoid unstable AArch64 FP16 vector type", merged **2026-08-13**, replaces `float16x8_t` with the
   stable `uint16x8_t` plus inline assembly precisely to "avoid `stdarch_neon_f16`, which fails on
   stable Rust when `target-feature=+fp16` is enabled". Release 0.11.0 predates it (2026-06-26), so
   the fix is on `main` and in no published version. This makes the pin explicitly temporary.
2. `candle-core` makes its `tokenizers` dependency optional, or drops the hard-coded `onig` feature.
   **Note this is a separate condition from (1) and is not addressed by PR #3845** — both defects
   must be resolved before moving off 0.9.2, and only one has a fix in flight.
3. Xtriever needs a candle capability that only ≥ 0.10 provides. The GGUF-metadata tokenizer helper
   that motivates the `onig` dependency is not such a capability.

**Follow-up actions**

- File an upstream issue against `huggingface/candle` for the non-optional `tokenizers`/`onig`
  dependency (defect (a)), noting that it forces a C toolchain on consumers who never touch GGUF.
  Defect (b) needs no issue — PR #3845 already covers it; track the release instead. Record URLs
  here.
- Add a comment at the dependency declaration pointing to this ADR, so `cargo add`-driven upgrades
  are caught in review.
- **Do not enable candle's `ug` feature on 0.9.2.** The `not(target_os = "ios")` guard that avoids
  App Store rejection `ITMS-90755` (prohibited instructions via `gemm`) was added for 0.11.0 and is
  absent from 0.9.2. It is off by default; keep it that way (research risk R9).

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Use current candle 0.11.0 on a **nightly** toolchain | Violates Principle VII's pinned stable toolchain, for a spike whose whole purpose is to measure the boring, reproducible path. Nightly also makes the measurements less meaningful as a baseline. |
| Use current candle 0.11.0 and add a `deny.toml` exception for `onig_sys` | Violates Agent Operating Rule 2 ("do not modify `deny.toml` unless the spec says so; otherwise stop and ask"), and weakens a portability gate to accommodate a dependency defect instead of avoiding it. `[graph] all-features = true` means feature-gating would not confine it anyway. |
| Vendor or patch candle (e.g. `[patch.crates-io]`) | Forbidden outright by FR-005: the spike must not vendor, patch or fork a crate to make a build succeed. |
| Replace candle with ONNX Runtime (`ort`) | Technically viable — `deny.toml` already anticipates `ort-sys` under `xtriever-dense`/`-rerank` wrappers, and the model repository ships ONNX and int8 variants. But Principle I names `candle` as the default ML inference engine, so this changes the **spec**, not the plan. Retained as the fallback if candle 0.9.2 fails on device. |
| Use candle 0.9.1 (the exact version checked for the `tokenizers` boundary) | `cargo add candle-core@0.9.1` resolves to 0.9.2 under the caret requirement; 0.9.2 is the newest 0.9.x and was the version actually measured. |
| Skip the embedding operation and spike only tantivy + tokenizers | Abandons the spike's central question. Would be the honest outcome only if no candle version worked at all — and one does. |
