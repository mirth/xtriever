# Research: Accelerated Inference Spike (Feature 028)

**Date**: 2026-09-27 · **Spec**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

Every item below was read from the pinned sources in the local cargo registry
(`candle-core 0.9.2`, `candle-nn 0.9.2`, `candle-metal-kernels 0.9.2`, `accelerate-src 0.3.2`,
`objc2-metal 0.3.2`) or measured with a throwaway probe crate outside the repository
(`probe028`, the scratchpad; nothing in the repository was changed to find these out).

## D1 — What the pinned engine offers

**Decision**: measure `accelerate` and `metal`, both candle 0.9.2 opt-in features; nothing else.

**Evidence**: `candle-core-0.9.2/Cargo.toml` `[features]`: `accelerate`, `cuda`, `cudnn`, `metal`,
`mkl`, `nccl`, `ug`; `default = []`. There is no Vulkan, OpenCL or NNAPI backend, so Android has
no GPU path in this engine (spec Assumptions). `cuda` needs an NVIDIA GPU neither measurement
device has.

**Alternatives rejected**: a second inference engine (ONNX Runtime, LiteRT) for Android — a C++
runtime and different model files the owner would have to supply; its own decision. Upgrading
candle — pinned by ADR-0001; not part of a spike.

## D2 — Both paths build for iOS

**Decision**: attempt both paths on the iPhone; the build is not the risk.

**Evidence**: `cargo check --target aarch64-apple-ios --features metal` and `--features accelerate`
on `probe028` (candle-core, candle-nn, candle-transformers at 0.9.2 with the matching features):
both finish. The same for `aarch64-apple-darwin`. `candle-core`'s manifest already excludes
`candle-ug` on `target_os = "ios"` (`[target.'cfg(all(not(target_arch = "wasm32"),
not(target_os = "ios")))'.dependencies.candle-ug]`), i.e. the Metal path is meant to exist on iOS.
A `cargo check` is not a link and not a run: linking is the implementation's first check, running is the
spike's.

## D3 — How Accelerate enters

**Decision**: the `accelerate` path keeps `Device::Cpu`; the feature alone reroutes the float
matrix multiply.

**Evidence**: `candle-core-0.9.2/src/cpu_backend/mod.rs:1440` — `#[cfg(feature = "accelerate")]`
matmul calls `crate::accelerate::sgemm` (line 1496) for `f32`; the default (line 1357,
`#[cfg(all(not(feature = "mkl"), not(feature = "accelerate")))]`) calls the `gemm` crate. Both
models compute in `f32` since Feature 026 (`quantised_bert.rs`, `compute!()` = `"f32"`: each
eight-bit matrix is expanded by `QTensor::dequantize` at load), so every large multiply takes
Accelerate's path. It rejects `f16` (line 1482), which the spike does not use. `accelerate-src
0.3.2`'s `build.rs` emits `cargo:rustc-link-lib=framework=Accelerate`.

## D4 — How Metal enters

**Decision**: the `metal` path builds both models on `Device::new_metal(0)`; everything else in
the forward pass follows the tensors' device.

**Evidence**:
- `candle-core-0.9.2/src/device.rs:258` `pub fn new_metal(ordinal: usize) -> Result<Self>`; line
  331 `metal_if_available`; `utils.rs:27` `metal_is_available`. The probe opened a Metal device on
  this Mac (`Device::new_metal(0).is_ok() == true`).
- `QTensor::dequantize(&self, device: &Device)` (`quantized/mod.rs:624`) dequantizes and moves
  the result `to_device(device)`: our `expand` (`xtriever-dense/src/quantised_bert.rs:55`, the
  same in `xtriever-rerank`) already passes the model's device, so the expanded `f32` weights
  land on the GPU with no model-code change.
- `candle-nn-0.9.2/src/ops.rs` carries `metal_fwd` for its custom ops (softmax-last-dim,
  layer norm, rms norm and others); the unary kernels include `gelu_erf` for `f32`
  (`candle-core-0.9.2/src/metal_backend/mod.rs:679`), which both models use (Feature 027 found
  exact GELU matters).
- Kernels are compiled from source at first use
  (`candle-metal-kernels-0.9.2/src/kernel.rs:121`, `new_library_with_source`), which iOS
  permits. The first call pays the compilation; the harnesses' warm-up already separates it.
- Device selection sits in exactly two places per crate: `Device::Cpu` in `embedder.rs:116, 156`
  (float, eight-bit) and `scorer.rs:118, 162`. The sparse encoder (`sparse.rs:184`) is out of
  scope.

**Unknown until run**: whether every op in the two forward passes has a Metal kernel for `f32`
(embedding lookup, broadcast add/mul, matmul, softmax over the last dim, layer norm, `gelu_erf`,
`tanh` in the re-ranker's pooler). A missing kernel is a result (spec Edge Cases), not a bug to
patch.

## D5 — Selection is compile-time, never run-time

**Decision**: two new non-default features per model crate, `spike-accelerate` and
`spike-metal`, mutually exclusive (`compile_error!` if both), forwarded by `xtriever-ffi`. No
environment variable, no runtime switch.

**Rationale**: the crates already hold that "an environment variable must not be able to change a
number" (`xtriever-dense/src/quantised_bert.rs` module docs, on `QMatMul::from_arc`). A
compile-time feature also makes FR-011 checkable: a default build is byte-for-byte today's.

**Alternatives rejected**: a `LoadPath`-style runtime enum on the public API — changes a binding
surface (`scripts/check-demos.sh` territory) for a spike; an env var — rejected above.

## D6 — Identity is not changed by the spike

**Decision**: under a spike feature the embedder's fingerprint and the re-ranker's model id stay
exactly today's; the compute path is recorded in the run records instead (D8).

**Rationale**: a CPU-built index must open on every path (the fingerprint is checked at open,
Principle VI) or the phone could not search the shipped index at all. Whether a shipping GPU
path should name itself in the identity — and so refuse CPU-built indexes, or be tolerated by
them — is a question the verdict hands the follow-up, with the measured score differences as its
evidence.

## D7 — Batched re-ranking needs no model change

**Decision**: a third non-default feature, `spike-batch` in `xtriever-rerank` (combinable with
either path), scores all pairs of a call in one forward pass: each pair tokenised exactly as
today, padded to the longest with `[PAD]` (id 0), token type 0 and attention mask 0 on padding;
the budget is checked once before the batch; the result is all scores or none.

**Evidence**: `QuantisedBert::forward(ids, type_ids, attention_mask: Option<&Tensor>)` takes a
`(batch, seq)` input and builds the additive mask (`quantised_bert.rs:161–183`); attention is
shaped `(batch, seq, heads, head_dim)` (line 199). Today's `Reranker::rerank` calls
`budget::rerank_with` one pair at a time (`scorer.rs:296`). The `Reranker` trait is unchanged.

**Consequence recorded, not fixed**: padding changes the arithmetic of the padded rows' softmax
(masked positions add `f32::MIN`), so batched and unbatched scores are compared (spec Edge
Cases), not assumed equal.

## D8 — Labels and digests in the existing harnesses

**Decision**: each harness records `computePath` (`cpu` / `accelerate` / `metal`) and
`rerankBatch` (`false` / `true`) and a `hitsDigest`: SHA-256 over every (query, depth, hit rank,
external id, score bits, re-rank score bits) of the run, in order. Two runs are **identical**
exactly when their digests are equal (FR-007).

**Where the label comes from**: the build that selected the features writes it — the iOS
packager's spike flag stages `XtrieverData/compute-path.json`, and the host script passes the
same values to `wikidemo measure`. One command sets both, so they cannot disagree. A Rust test
additionally pins `xtriever_dense::spike::COMPUTE_PATH` to the active feature.

**Harnesses** (existing; the spike adds the two labels and the digest, nothing else):
- the iOS demo's `DemoMeasurementTests` — the phone's app-level latency, comparable with the
  reference table (fused 200.5 ms, re-ranked 1,213 ms, 335.4 MB);
- the Swift package's `DeviceMeasurementTests` with `XTRIEVER_CORPUS=wikipedia` — depths
  0/5/10/20, parity against the host goldens (max dense and re-rank differences, lexical bits,
  fused order), footprint; run on the harness app (`swift/XtrieverHarnessApp`);
- the Python demo's `wikidemo measure` on the host — per-depth latency, parity 800 hits.

The Swift package harness has not been run on Wikipedia since Feature 026's eight-bit models, so
its CPU run is the spike's first measurement and the reference for the others.

## D9 — Frameworks at link time on iOS

**Decision**: the spike's `xcodebuild` commands pass `OTHER_LDFLAGS="-framework Accelerate"` or
`"-framework Metal -framework Foundation -framework CoreGraphics"`; `Package.swift` is not
changed.

**Evidence**: a Rust static library does not carry `rustc-link-lib=framework=…` into the Xcode
link; `accelerate-src`'s `build.rs` requests Accelerate; `objc2-metal-0.3.2/src/lib.rs:18`
documents linking CoreGraphics beside Metal. Changing `Package.swift` would change the default
build (FR-011).

## D10 — Host build throughput and the mixed case, through the harness's cache

**Decision**: each path runs `beir run --dataset scifact --config hybrid-rerank-v3` with its own
`--cache-dir target/spike-028/<path>` (fresh), timed; the mixed case re-runs the **CPU** path
against the Metal path's cache directory.

**Evidence**: the harness caches the embedder's floats per dataset, keyed on the embedder
fingerprint (`xtriever-eval/examples/beir.rs`, `vectors.f32.bin` beside `cache.json`). D6 keeps
the fingerprint, so a CPU run finds the Metal-built vectors valid and builds its index from them
while embedding queries on the CPU — exactly the mixed case, with no new code. Separate cache
directories keep the canonical CPU cache clean. A dev-dependency's feature can be switched on
for an example (`cargo run -p xtriever-eval --example beir --features xtriever-dense/spike-metal`):
verified on a probe crate (`--features candle-core/metal` reached a dev-dependency).

## D11 — Dependency policy holds

**Decision**: no `deny.toml` change is needed.

**Evidence**: `deny.toml` sets `[graph] all-features = true`, so feature-gated dependencies are
checked. `cargo deny check licenses bans sources` with the repository's `deny.toml` on
`probe028` (both features): bans ok, sources ok, licenses ok for every candle, Metal and
Accelerate dependency (the one error was the probe crate's own missing licence field);
`cargo deny check advisories`: ok. The Metal bindings (`objc2-*`) and `accelerate-src` are
system-framework bindings, not C/C++ build dependencies, and live only behind the model crates'
non-default features (Principle III as written).

## D12 — What the spike does not build

Fallback from GPU to CPU (spec: stated, not built); an identity change (D6); Android, CUDA, the
Neural Engine, half precision (spec Assumptions); a `criterion` bench (the harness records are
the measurement, as in Features 008, 009, 026; a follow-up that ships a path adds the bench).
