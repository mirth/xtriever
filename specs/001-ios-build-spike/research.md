# Phase 0 Research: iOS Build Spike

**Feature**: `001-ios-build-spike` | **Date**: 2026-09-10 | **Spec**: [spec.md](./spec.md)

## How this was produced

Two kinds of evidence appear below, and they are labelled:

- **MEASURED** — resolved and compiled locally on this machine (macOS, Xcode 26.6, rustup toolchain
  1.91.1 pinned by `rust-toolchain.toml`) in throwaway crates under `/tmp`, outside the repository.
  Nothing was added to the Xtriever workspace during planning.
- **CITED** — read from crate documentation, crate source in the local registry, official Apple
  documentation, or the Hugging Face model repository. URLs or file paths are given.

No API item below is recalled from memory (Agent Operating Rule 1). Where an item could not be
verified it is listed under **Open risks** rather than assumed.

**No build of the spike itself was attempted.** Proving the crates build for iOS *is* User Story 1,
the spike's own deliverable. What follows resolves the design unknowns needed to order the work; the
verdicts recorded in the spike report supersede anything here.

---

## D1 — tantivy feature set

**Decision**: `tantivy = { version = "0.26.2", default-features = false, features = ["mmap", "stopwords", "lz4-compression", "stemmer"] }`

**Rationale**: MEASURED — tantivy 0.26.2's default feature set **does** pull a C dependency, so the
spec's "default features minus anything pulling C deps" is not a no-op. `cargo add tantivy` enables
`columnar-zstd-compression`, and:

```
zstd-sys v2.1.0+zstd.1.5.7   (built with cc v1.4.5)
└── zstd-safe v7.3.0 └── zstd v0.13.3 └── tantivy-sstable v0.7.0
    └── tantivy-columnar v0.7.0 └── tantivy v0.26.2
```

CITED — the declared default set is
`default = ["mmap", "stopwords", "lz4-compression", "columnar-zstd-compression", "stemmer"]`
(<https://docs.rs/crate/tantivy/0.26.2/source/Cargo.toml>). The chosen set is therefore exactly
*default minus `columnar-zstd-compression`*. MEASURED — with that set, the graph contains no `zstd`,
no `-sys` crate and no `cc`. `lz4-compression` is retained because it maps to `lz4_flex`, which is
pure Rust.

`mmap` is kept because CITED it is the feature gating `MmapDirectory`, `Index::create_in_dir`,
`Index::open_in_dir` and `Index::create_from_tempdir`
(<https://docs.rs/tantivy/0.26.2/src/tantivy/directory/mod.rs.html>,
<https://docs.rs/tantivy/0.26.2/src/tantivy/index/index.rs.html>). Without it only `RamDirectory`
survives, and an in-RAM index would not exercise the iOS application sandbox, which the spec's edge
cases call out as a distinct risk.

**Alternatives rejected**: keeping `columnar-zstd-compression` and permitting `zstd-sys` — rejected
because it compiles C for no benefit at 1,000 documents. Dropping `mmap` — rejected because it
removes the on-disk path the spike is meant to test.

---

## D2 — tokenizers feature set

**Decision**: `tokenizers = { version = "0.23.2", default-features = false, features = ["fancy-regex"] }`

**Rationale**: MEASURED — this leaves `onig` and `esaxx_fast` off. Both are native: `onig` →
`onig_sys` (C, via `cc`), `esaxx_fast` → `esaxx-rs/cpp` (C++). The spec's stated feature choice is
correct.

One subtlety worth recording, because it looks alarming in a dependency tree and is not: `esaxx-rs`
**is a non-optional dependency of tokenizers** and so appears in the graph regardless of features.
It compiles no C++ here. MEASURED — tokenizers declares it with `default_feat=False, feats=[]`, and
CITED — `esaxx-rs` 0.1.10's own `build.rs` is
`#[cfg(not(feature = "cpp"))] fn main() {}`, with C++ compilation gated behind its `cpp` feature
(which lives only in its `default`). MEASURED — its active feature set in our graph is empty, and
`cc` is absent from the whole graph, which independently proves nothing compiles C or C++.

**Alternatives rejected**: `onig` — a C dependency that `deny.toml` already bans by name.
`esaxx_fast` — C++, and only a BPE-training speedup that inference does not need.

---

## D3 — candle version: pin 0.9.2, not the current 0.11.0 ⚠️

**Decision**: `candle-core`, `candle-nn`, `candle-transformers` all at **0.9.2**, with default
features (`default = []`).

This is a deliberate departure from Principle VII's "dependencies added with `cargo add` at current
versions". It is justified by measurement, and it needs an ADR. It is the single most consequential
finding of Phase 0.

**Rationale — two independent, measured defects in the current versions:**

**(a) candle-core 0.10.1 and later force a C dependency that `deny.toml` bans.** MEASURED:

```
candle-core 0.11.0 -> tokenizers req=^0.22.0 kind=normal optional=False
                      default_features=False features=['onig']
```

The dependency is **normal and non-optional**, and it **hard-codes the `onig` feature**. No candle
feature flag can switch it off, so `onig` → `onig_sys` (C, via `cc`) enters the graph
unconditionally, and `deny.toml` denies `onig_sys` outright. It also pins a second copy of
tokenizers (0.22.2 beside our 0.23.2), which trips `multiple-versions`. MEASURED via the crates.io
dependency API, the boundary lies between the 0.9 and 0.10 series: 0.11.0 and 0.10.1 declare it;
**0.9.1, 0.8.4, 0.7.2 and 0.6.0 have no `tokenizers` dependency at all.**

CITED — the declaration in `candle-core/Cargo.toml` is target-scoped, which is why it is invisible
to a casual reading of the `[features]` table:

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
tokenizers = { workspace = true, features = ["onig"] }
```

It exists to support a GGUF-metadata BPE tokenizer helper in `candle-core/src/quantized/tokenizer.rs`
— nothing to do with BERT, and nothing this project needs.

**(b) candle-core 0.11.0 does not compile on stable Rust at all on this machine — including the host
target.** MEASURED, `cargo check` on toolchain 1.91.1:

| target | candle-core 0.11.0 | candle-core 0.9.2 |
|---|---|---|
| `aarch64-apple-darwin` (host) | **FAIL** | PASS |
| `aarch64-apple-ios` (device) | PASS | PASS |
| `aarch64-apple-ios-sim` | **FAIL** | PASS |

The failure is `error[E0658]: use of unstable library feature 'stdarch_neon_f16'` at
`candle-core-0.11.0/src/cpu/neon.rs:160,188,189` (`float16x8_t`), i.e. it requires nightly
(rust-lang/rust#136306).

CITED — the mechanism, from the local registry copy of `candle-core-0.11.0/src/cpu/neon.rs`: line 71
opens `mod fp16`, line 81 is the portable path under `#[cfg(not(target_feature = "fp16"))]`, and
line 153 is the `float16x8_t` path under `#[cfg(target_feature = "fp16")]`. MEASURED —
`rustc --print cfg` shows why the device is the odd one out:

| target | `neon` | `fp16` |
|---|---|---|
| `aarch64-apple-ios` | yes | **no** |
| `aarch64-apple-ios-sim` | yes | **yes** |
| `aarch64-apple-darwin` | yes | **yes** |

So the device target compiles only because `fp16` happens to be absent there. Anywhere `fp16` is
enabled — the simulator, and every Apple-silicon developer machine and macOS CI runner — candle
0.11.0 needs nightly. That breaks the constitution's blocking `nextest on macOS` gate, not merely
iOS, and it is incompatible with Principle VII's pinned stable toolchain.

**MEASURED — candle 0.9.2 has neither defect**: it passes `cargo check` on host, device and
simulator, and its graph contains no `onig`, no `zstd`, no `-sys` crate and no `cc`. CITED —
`candle-core-0.9.2/Cargo.toml` declares `default = []`, with `accelerate`, `mkl`, `cuda`, `cudnn`,
`metal` and `nccl` all opt-in, so the default build pulls no system or C dependency. Independently
corroborated: 0.9.2 also builds for `aarch64-linux-android`, and the BERT API used below compiles
identically against 0.9.2 and 0.11.0, so the downgrade costs nothing this spike needs.

**There is an upstream fix for defect (b), but it is unreleased.** CITED — candle PR #3845, "Avoid
unstable AArch64 FP16 vector type", merged **2026-08-13**, stores FP16 lane bits in the stable
`uint16x8_t` and supplies float semantics via inline assembly, specifically to "avoid
`stdarch_neon_f16`, which fails on stable Rust when `target-feature=+fp16` is enabled". Release
0.11.0 shipped 2026-06-26, **before** that merge, so the fix is on `main` and in no published
version. This makes the 0.9.2 pin explicitly temporary with a concrete unblock condition — the next
candle release after 0.11.0 — rather than an open-ended downgrade. Recorded as ADR-0001's primary
review trigger.

**Alternatives rejected**:
- *candle 0.11.0 + nightly toolchain* — violates Principle VII's pinned stable toolchain, for a
  spike whose entire purpose is to measure the boring path.
- *candle 0.11.0 + a `deny.toml` exception for `onig_sys`* — Agent Operating Rule 2 forbids touching
  `deny.toml` unless the spec says so, and it would weaken a gate to accommodate a dependency defect
  rather than avoid it. Note also that `deny.toml` sets `[graph] all-features = true`, so
  feature-gating would **not** hide a banned crate from `cargo deny` anyway.
- *Vendoring or patching candle* — forbidden outright by FR-005.
- *Switching the embedder to ONNX Runtime* — `ort-sys` is already anticipated in `deny.toml` under
  `xtriever-dense`/`-rerank` wrappers, and the model repository ships ONNX variants. But Principle I
  names `candle` as the default ML inference engine, so this would change the *spec*, not the plan.
  Recorded as the fallback if candle 0.9.2 fails on device.
- *Waiting for an upstream fix* — tracked as an open risk, not a plan.

---

## D4 — uniffi and the binding shape

**Decision**: `uniffi = "0.32.1"`, proc-macro scaffolding, `crate-type = ["cdylib", "staticlib"]`,
bindings generated by a dedicated workspace binary crate.

**Rationale**: CITED and MEASURED — uniffi 0.32.1 is current, and its library dependency graph
contains no `cc`, no `cmake`, no `-sys` crate and no `build.rs` in any `uniffi*` crate, so it is pure
Rust and safe under Principle III even though `xtriever-ffi` is a leaf crate where C would be
tolerated.

CITED, from the UniFFI guide (<https://mozilla.github.io/uniffi-rs/latest/>):

- `uniffi::setup_scaffolding!()` at the top of `lib.rs`, `#[uniffi::export]` on the three functions,
  `#[derive(uniffi::Record)]` for the result structs, `#[derive(uniffi::Error)]` for the error enum.
  The proc-macro path needs no `build.rs`
  (<https://mozilla.github.io/uniffi-rs/latest/tutorial/Rust_scaffolding.html>).
- iOS requires `staticlib` in addition to `cdylib`
  (<https://mozilla.github.io/uniffi-rs/latest/tutorial/Prerequisites.html>).
- In a multi-crate workspace, bindings are generated by a dedicated crate exposing a
  `[[bin]]` whose `main` calls `uniffi::uniffi_bindgen_swift()`, invoked as `cargo run -p …`
  (<https://mozilla.github.io/uniffi-rs/latest/tutorial/foreign_language_bindings.html>).
  `uniffi-bindgen-swift` is the Swift-specific driver and is the one with
  `--xcframework`/`--modulemap` support
  (<https://mozilla.github.io/uniffi-rs/latest/swift/uniffi-bindgen-swift.html>).
- Errors reach Swift as typed `throws` when the exported function returns `Result<T, E>` where `E`
  is an `enum` implementing `std::error::Error`
  (<https://mozilla.github.io/uniffi-rs/latest/types/errors.html>) — `thiserror` satisfies this, and
  it is what Principle VII already mandates. **A Rust panic inside a non-`throws` Swift function is
  an uncatchable fatal error** (<https://mozilla.github.io/uniffi-rs/latest/swift/overview.html>),
  so FR-008 is satisfied by declaring `Result` on all three operations, not by a catch-all.
- `Vec<f32>` crosses natively to Swift `[Float]`, verified in UniFFI's own CI
  (`bindgen-tests/lib/src/collections.rs` `roundtrip_vec_f32`, asserted in
  `bindgen-tests/swift/tests/collections.swift`). This is what carries the embedding out.

**Cost to be aware of when reading the wall-time numbers**: CITED — `uniffi_core`'s
`Lower for Vec<T>` serializes vectors element-by-element into a `RustBuffer` rather than passing a
pointer (`uniffi_core/src/ffi_converter_impls.rs`, "Vectors are currently always passed by
serializing to a buffer"). The 1,000-document corpus going *in* and the 384-float embedding coming
*out* both pay that cost, and it is inside the measured wall times. FR-017's per-operation split
keeps it attributable; the report must not present FFI serialization as engine time.

**Alternatives rejected**: the UDL-file path — CITED, single-UDL generation is described as
deprecated in the guide, and the proc-macro path removes a build script.

---

## D5 — determinism of the on-device ranking ⚠️

**Decision**: build the index with **one worker thread** —
`Index::writer_with_num_threads(1, memory_budget)` — for both the host golden fixture and the device
run.

**Rationale**: this is a correctness prerequisite for FR-014, which requires the on-device ranking to
be *identical* to the host golden ranking, and for Principle VI's determinism guarantee.

CITED — two facts from tantivy 0.26.2 that together make the naive approach flaky:

1. `TopDocs` breaks score ties by **ascending `DocAddress`**, not ascending `DocId`:
   "In case of a tie on the sort key, documents are always sorted by ascending `DocAddress`"
   (<https://docs.rs/tantivy/0.26.2/tantivy/collector/struct.TopDocs.html>). `DocAddress` is
   `{ segment_ord: SegmentOrdinal, doc_id: DocId }`, so the ordering is segment-ordinal-major.
2. `Index::writer_for_tests` is documented as using "a single thread [which] gives us a deterministic
   allocation of `DocId`" (<https://docs.rs/tantivy/0.26.2/src/tantivy/index/index.rs.html>) —
   implying the **multi-threaded writer does not** allocate `DocId`s or segments reproducibly, since
   each indexing thread builds its own independent segment.

`Index::writer` defaults to `min(available_parallelism(), 8)` threads, which differs between a
developer Mac and an iPhone. Left alone, the same corpus would produce different `DocId`/segment
assignments on host and device, and any score tie would then order differently — a spurious FR-014
failure that looks like an iOS bug. Fixing the writer to one thread removes the variable.

Note the constitution's own wording ("ties broken by ascending `DocId`") does not match tantivy's
tie-break (ascending `DocAddress`). At one segment they coincide. This mismatch is worth an ADR
before any multi-segment index exists; it is out of scope here and recorded as an open risk.

**Alternatives rejected**: comparing only the top-k *set* rather than the ordered list — rejected as
weakening the oracle (FR-028). Comparing scores with a tolerance — rejected for the same reason;
host-vs-device float equality is the property Principle VI actually promises.

---

## D6 — tokenization parity ⚠️

**Decision**: after `Tokenizer::from_file`, explicitly override truncation and padding to a maximum
sequence length of **256**, and assert the reference token sequence before comparing embeddings.

**Rationale**: CITED — the model repository's `tokenizer.json` has truncation and padding **baked
in at 128**: `truncation: {"max_length": 128, …}` and
`padding: {"strategy": {"Fixed": 128}, …}`. Python's `AutoTokenizer` overrides these from
`tokenizer_config.json`, so a naive Rust `Tokenizer::from_file` and the Python reference **disagree
by construction**, with no iOS involvement at all.

The repository states three different lengths — 128 in `tokenizer.json`, 256 in
`sentence_bert_config.json` (`{"max_seq_length": 256, …}`) and the model card ("input text longer
than 256 word pieces is truncated"), and 512 as the architectural ceiling
(`max_position_embeddings`). **256** is the number that reproduces reference sentence-transformers
behaviour.

This is exactly the failure the spec's edge cases anticipated: a tokenization difference masquerading
as an embedding failure. FR-015's separate token-sequence assertion is what keeps them apart, and it
must be checked *first*.

---

## D7 — model artifacts and pinning

**Decision**: pin the Hugging Face revision
**`1110a243fdf4706b3f48f1d95db1a4f5529b4d41`** and bundle exactly three files.

CITED, from <https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2> (sizes from the
`?blobs=true` model API, revision confirmed via `git ls-remote`):

| file | bytes | role |
|---|---:|---|
| `model.safetensors` | 90,868,376 | weights, **all F32** (safetensors header: 103 × F32 tensors) |
| `tokenizer.json` | 466,247 | fast-tokenizer definition |
| `config.json` | 612 | architecture, deserializes into candle's `Config` |
| **total** | **91,335,235** (87.1 MiB) | |

Architecture: `model_type` `bert`, hidden 384, 6 layers, 12 heads, vocab 30,522,
`max_position_embeddings` 512, `intermediate_size` 1,536, `hidden_act` gelu,
`layer_norm_eps` 1e-12. License Apache-2.0 — inside `deny.toml`'s allow-list.

Two loading details that matter:

- CITED — safetensors keys carry **no `bert.` prefix** (`embeddings.*`, `encoder.layer.N.*`,
  `pooler.dense.*`), which matches candle's `BertModel::load` calling `vb.pp("embeddings")` and
  `vb.pp("encoder")` at the root. So the `VarBuilder` root prefix is `""`. `pooler.dense.*` is
  present and unused by mean pooling.
- FR-016's "pinned by content hash" is satisfied by the revision SHA plus the exact byte size above;
  the spike additionally records the SHA-256 of the downloaded `model.safetensors`.

**87.1 MiB of the 300 MB ceiling is therefore accounted for before a single document is indexed** —
the spec's assumption of "roughly 90 MB" is confirmed, leaving on the order of 200 MB for weights
resident in memory, the index, the runtime and the application.

---

## D8 — sentence embedding definition

**Decision**: attention-mask-weighted **mean pooling** over the last hidden state, then **L2
normalization**, giving a **384**-dimensional vector.

**Rationale**: CITED — `1_Pooling/config.json` is
`{"word_embedding_dimension": 384, "pooling_mode_mean_tokens": true, "pooling_mode_cls_token": false, …}`,
and `modules.json` lists three modules: `Transformer`, `Pooling`, then `Normalize`. The model card's
reference snippet applies `F.normalize(..., p=2, dim=1)`.

CITED — the candle side, read from the local registry copy of
`candle-transformers-0.9.2/src/models/bert.rs` (authoritative for the pinned version, in preference
to docs for a different one):

```rust
pub const DTYPE: DType = DType::F32;                                    // line 15
impl BertModel {
    pub fn load(vb: VarBuilder, config: &Config) -> Result<Self>        // line 466
    pub fn forward(&self, input_ids: &Tensor, token_type_ids: &Tensor,
                   attention_mask: Option<&Tensor>) -> Result<Tensor>   // line 495
}
```

`Config` (line 57) is serde-deserializable and its field names line up with the model's
`config.json`. `token_type_ids` is **required, not optional** — a zeros tensor for a single-segment
sentence.

Weights are loaded through `candle_nn::VarBuilder`. CITED —
`candle-nn-0.9.2/src/var_builder.rs` offers both:

- `pub unsafe fn from_mmaped_safetensors(...)` — line 642, **`unsafe`**
- `pub fn from_buffered_safetensors(data: Vec<u8>, dtype, dev) -> Result<VarBuilder>` — line 652, safe

See D12; this choice is a Principle VII question with a measurable memory consequence.

---

## D9 — how memory is measured on device

**Decision**: report **`ri_lifetime_max_phys_footprint`** as the peak, sample `phys_footprint`
during the run, and additionally record the device's own limit.

**Rationale**: CITED, Apple documentation:

- Footprint, not RSS, is the enforced metric: "Memory footprint … is used for memory limit
  enforcement" (WWDC22 "Profile and optimize your game's memory",
  <https://developer.apple.com/videos/play/wwdc2022/10106/>). Jetsam's `per-process-limit` reason is
  "The process crossed the resident memory limit imposed by the system on all apps"
  (<https://developer.apple.com/documentation/xcode/identifying-high-memory-use-with-jetsam-event-reports>).
- Instantaneous value: `task_info(mach_task_self_, TASK_VM_INFO, …)` into `task_vm_info_data_t`,
  reading `phys_footprint`; check the returned count against `TASK_VM_INFO_REV1_COUNT` first, since
  REV0 predates the field (`mach/task_info.h` in the iOS SDK).
- **Lifetime peak without sampling**: `proc_pid_rusage(getpid(), RUSAGE_INFO_CURRENT, …)` then
  `ri_lifetime_max_phys_footprint` (WWDC22 10106). This is what satisfies FR-020's "peak across the
  whole run" — see the open risk about its header availability.
- The device's actual limit: `os_proc_available_memory()` returns "the number of bytes remaining …
  before the current process will hit its current dirty memory limit" (`os/proc.h`, iOS 13+),
  equivalently `task_vm_info.limit_bytes_remaining`. Apple documents that a per-app limit exists and
  varies by device but **publishes no numbers**, so the spike records the limit it actually observed
  alongside the 300 MB constitutional ceiling. Those are two different thresholds and the report must
  not conflate them.

CITED — this also independently validates the spec's refusal to substitute simulator numbers: for the
Simulator, Apple states "the memory gauge always stays in the green (safe) region because macOS
doesn't issue memory warnings or out-of-memory terminations"
(<https://developer.apple.com/documentation/xcode/gathering-information-about-memory-use>).

Apple explicitly declines to guarantee that any API matches the Xcode debug gauge, so the report
quotes the API value as authoritative and the gauge only as a cross-check.

---

## D10 — how binary size is measured

**Decision**: archive, export with thinning for all variants, and report the **uncompressed** figure
from `App Thinning Size Report.txt`; attribute bytes to the Rust static library with a **link map**.

**Rationale**: CITED, "Reducing your app's size"
(<https://developer.apple.com/documentation/xcode/reducing-your-app-s-size>) is unusually blunt:
"none of the binaries that you create for debugging or that you upload to the App Store from within
Xcode are suitable for measuring your app's size". The supported route is archive → export with
"All compatible device variants" → `App Thinning Size Report.txt`, where "**The uncompressed size is
equivalent to the size of the installed app on the device**, and the compressed size is the download
size". Automatable via `xcodebuild -exportArchive` with `thinning` set to `<thin-for-all-variants>`.

This settles the spec's binary-size ambiguity precisely: the `.app`, the `.xcarchive` and the `.ipa`
are all the wrong number. FR-018's code-versus-weights split comes from the link map
(`LD_GENERATE_MAP_FILE` / `LD_MAP_FILE_PATH`, or `-map`), whose per-symbol `size` and object-file
index let bytes be summed per contributing `.a`
(<https://developer.apple.com/forums/thread/733475>), cross-checked with `size -m` on the thinned
executable. The 87.1 MiB of bundled weights is a resource, not part of the executable, so the two
components separate cleanly.

---

## D11 — how wall time is measured

**Decision**: `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)`, with `ProcessInfo.thermalState` recorded
alongside every measurement.

**Rationale**: CITED — Apple's own `mach_absolute_time` reference says "Prefer to use the equivalent
`clock_gettime_nsec_np(CLOCK_UPTIME_RAW)` in nanoseconds"
(<https://developer.apple.com/documentation/driverkit/mach_absolute_time>), and `clock_gettime(3)`
documents `CLOCK_UPTIME_RAW` as monotonic, not incrementing while asleep, and identical to
`mach_absolute_time` after timebase conversion. Swift's `ContinuousClock` is an alternative but keeps
counting while the system sleeps, which is wrong for attributing compute time.

CITED — harness hygiene, from "Writing and running performance tests"
(<https://developer.apple.com/documentation/xcode/writing-and-running-performance-tests>): build for
testing with the **Release** configuration, turn off "Debug executable", and disable code coverage
and the runtime sanitizers. FR-019's "build configuration" field exists to record that this was done.
`ProcessInfo.thermalState` (iOS 11+) supplies FR-019's device-state field.

**Note**: this timing lives in the Swift harness, satisfying FR-010 and keeping
`std::time::Instant` out of every Rust crate — pure or leaf.

---

## D12 — safetensors loading: safe buffer vs. unsafe mmap ⚠️

**Decision**: implement the **safe** `from_buffered_safetensors` path as the primary, and measure the
`unsafe` mmap path as a second data point.

**Rationale**: this is a genuine Principle VII conflict with a measurable payoff, so the spike should
produce the number rather than the plan guessing.

- The mmap constructor is `unsafe` (D8). Principle VII allows `unsafe` **only** in `xtriever-dense`
  SIMD kernels, each block preceded by `// SAFETY:` and tested against the safe path. A
  `VarBuilder` call is not a SIMD kernel, and `Cargo.toml` sets `unsafe_code = "deny"`
  workspace-wide, so this path needs an explicit `#[allow(unsafe_code)]` and, read strictly, an ADR.
- The safe constructor takes `Vec<u8>`, so it reads all 87.1 MiB into the heap — memory that
  certainly counts toward `phys_footprint` and therefore against both the 300 MB ceiling and the
  device's own limit.
- mmap'd file-backed pages are clean and may not count the same way, which could matter a great deal
  at a 300 MB budget.

Measuring both costs one extra harness call and turns a Principle VII argument into a number.
"Tested against the safe path" is then satisfied literally: the two paths must produce the same
embedding.

**Alternatives rejected**: unsafe mmap only — undermines the Principle VII gate with no evidence that
it is needed. Safe path only — leaves the most promising memory optimisation unmeasured in the one
spike commissioned to measure memory.

---

## D13 — wasm32 status

**Finding**: MEASURED — the proposed stack fails `cargo check` for `wasm32-unknown-unknown`, at
`getrandom` 0.3.4: "The wasm32-unknown-unknown targets are not supported by default; you may need to
enable the `wasm_js` configuration flag" (`getrandom-0.3.4/src/backends.rs:194`). It is the only
crate that fails.

Principle III makes wasm32 explicitly best-effort and tracked, and FR-030 puts it out of scope, so
this is recorded, not fixed. The cause is a `RUSTFLAGS`-level configuration flag on a transitive
dependency, not anything about tantivy, candle or tokenizers.

Corroborated as **actually fixable**: candle 0.9.2 builds for `wasm32-unknown-unknown` with
`RUSTFLAGS='--cfg getrandom_backend="wasm_js"'` plus a target-scoped
`getrandom = { version = "0.3", features = ["wasm_js"] }`. That is a promising signal for the
constitution's tracked wasm32 line, but it is out of scope here (FR-030) and remains **untested for
the full stack including tantivy and uniffi** — uniffi's own wasm support is documented as unstable.
Recorded so a later spec knows where to start, not claimed as a result.

---

## D14 — toolchain shadowing in this environment ⚠️

**Finding**: MEASURED — `/opt/homebrew/bin/cargo` precedes `~/.cargo/bin/cargo` on `PATH`, and
`rustc --version` reports `1.91.1 (Homebrew)`. The Homebrew cargo does **not** honour
`rust-toolchain.toml` or rustup's installed targets: `cargo build --target aarch64-apple-ios` failed
with "can't find crate for `std` … the `aarch64-apple-ios` target may not be installed", and
succeeded immediately once `~/.cargo/bin` was put first.

Principle VII requires the toolchain to be pinned by `rust-toolchain.toml`. In this shell it was not,
which would silently invalidate any cross-target verdict the spike produced. Not a code defect, but it
would have produced a wrong FAIL in User Story 1.

**RESOLVED 2026-09-11.** The Homebrew Rust install was removed; `cargo` and `rustc` both now resolve
under `~/.cargo/bin`, and `scripts/check-toolchain.sh` passes with no `PATH` manipulation. The guard
stays — CI runners and other developer machines can still hit this, and the shape of the trap changed
once mid-investigation (removing Homebrew's `cargo` was not sufficient; its `rustc` still shadowed
rustup's independently), which is exactly why the guard checks both binaries plus the active
toolchain.

---

---

## D15 — candle spawns threads unconditionally; pin the thread count ⚠️

**Decision**: set `CANDLE_NUM_THREADS=1` (and `RAYON_NUM_THREADS=1`) for both the host golden
generation and the device run, and record the value in every `DeviceRun`.

**Rationale**: CITED — `rayon` is a **mandatory, non-optional** dependency of candle-core and
candle-nn in every version from 0.8.4 through 0.11.0, and it is used unconditionally on the CPU
path (`candle-core/src/cpu_backend/mod.rs`, `cpu_backend/conv2d.rs`, `sort.rs`, `cpu/kernels.rs`).
candle-core additionally spawns raw `std::thread`s through a `BarrierPool`
(`candle-core/src/utils.rs`, using `std::thread::Builder` and `std::thread::park`) and keeps a
private `OnceLock<rayon::ThreadPool>`. Thread creation is lazy, and the count comes from
`CANDLE_NUM_THREADS`, else `RAYON_NUM_THREADS`, else the physical core count.

Three consequences:

1. **Principle III is satisfied but only by placement.** "No unconditional threads" binds the five
   pure crates; candle lives behind a non-default feature in `xtriever-ffi`, a named leaf crate. The
   threads are unavoidable in candle at any version, so any future attempt to use candle from a pure
   crate is blocked outright — worth knowing now.
2. **Reproducibility.** A parallel float reduction whose thread count differs between a developer Mac
   and an iPhone can change summation order and therefore low-order bits. The embedding is compared
   under tolerance (FR-015) so this is unlikely to bite, but pinning the count costs nothing and
   removes the variable — the same reasoning as D5 for the index.
3. **Memory.** Thread stacks count toward `phys_footprint`. At a 300 MB ceiling, an unpinned pool
   sized to the device's core count is an uncontrolled term in the measurement.

CITED — `std::time::Instant` is **not** used anywhere in candle-core, candle-nn or
candle-transformers, so nothing here endangers FR-010 or the wasm32 constraint.

---

## Measured build matrix (planning-time, indicative only)

MEASURED on the proposed pinned stack — tantivy 0.26.2 (D1) + tokenizers 0.23.2 (D2) + candle 0.9.2
(D3) + uniffi 0.32.1 (D4), `cargo check` only:

| target | result |
|---|---|
| `aarch64-apple-darwin` | PASS |
| `aarch64-apple-ios` | PASS |
| `aarch64-apple-ios-sim` | PASS |
| `wasm32-unknown-unknown` | FAIL — `getrandom` (D13), best-effort |

C/C++ compilation in the graph: **none** — no `cc`, no `-sys` crate, no `zstd`, no `onig`; `esaxx-rs`
present with no features (D2). Single `tokenizers` version, so no duplicate-version warning.

**These are not User Story 1's verdicts.** `cargo check` performs no codegen and no linking, the
scratch crate merely depends on the four crates rather than calling them, and nothing was built for
the device as a `staticlib` or linked into an app. US1 must still run the real builds and record its
own verdicts. What this matrix does establish is that the *plan* is not built on a guess: the
dependency-level portability question has a measured answer, and the one blocking defect (D3) was
found before any code was written.

---

## Open risks

Carried into the plan; each is a candidate finding under User Story 4.

| # | Risk | Evidence status | Consequence if it bites |
|---|---|---|---|
| R1 | `proc_pid_rusage` is declared in no public iOS SDK header (there is no `libproc.h`); it appears only in a WWDC22 slide with a hand-written prototype, though the symbol is exported by `libSystem.tbd`. | UNVERIFIED for supported iOS availability | Peak footprint must fall back to sampling `phys_footprint`, which can miss a transient peak. Record which method was used. |
| R2 | End-to-end XCFramework assembly for device + simulator is **not** documented in the UniFFI guide; `--xcframework` and the `module.modulemap` rename are, but `xcodebuild -create-xcframework` appears nowhere in the v0.32.1 tree. | CITED absence | Story 2 packaging may cost more than planned. Third-party recipes exist but are unreviewed. |
| R3 | `#[cfg()]` does not work inside `#[uniffi::export]` blocks — scaffolding is generated regardless. | CITED (UniFFI proc-macro docs) | Feature-gating the FFI surface for FR-003 needs `#[cfg()]` *outside* separate export blocks. |
| R4 | The constitution says ties break by ascending `DocId`; tantivy breaks them by ascending `DocAddress`. Identical at one segment, divergent beyond. | CITED | Needs an ADR before any multi-segment index. Not triggered by this spike (D5). |
| R5 | The 0.9.2 pin will age. Upstream PR #3845 fixes the `stdarch_neon_f16` defect but is unreleased; nothing yet fixes the mandatory `tokenizers`/`onig` dependency. | MEASURED defect; fix CITED as merged-but-unreleased | Revisit at the next candle release. Even then, the `onig` C dependency remains a separate blocker — both must be resolved before moving off 0.9.2. Tracked in ADR-0001. |
| R9 | Enabling candle's optional `ug` feature on 0.9.2 risks App Store rejection (`ITMS-90755`, prohibited instructions via `gemm`). The `not(target_os = "ios")` guard was added for 0.11.0 and is **not** in 0.9.2. | CITED (candle issue #2749, PR #3071) | Irrelevant while `ug` stays off, which it is by default — but a future `--all-features` build or a careless feature addition would hit it. Do not enable `ug`. |
| R6 | No tantivy documentation states that identical index + query + config yields bit-identical scores, or fixes float summation order across segments. | CITED absence | FR-014's exact-equality oracle rests on D5 plus single-segment behaviour, not a documented guarantee. If it fails, that is itself a Principle VI finding. |
| R7 | Apple publishes no per-device memory limit values, and does not guarantee `phys_footprint` equals the value jetsam compares against. | CITED | The 300 MB verdict is against the constitutional ceiling; the device's own limit is recorded separately (D9). |
| R8 | `uniffi_core`'s `Lower for Vec<T>` contains `.unwrap()` and caps vectors at `i32::MAX`. | CITED | Inside a dependency, so workspace lints do not fire; irrelevant at 1,000 documents but worth knowing before the production FFI. |

## Unknowns deliberately left to the spike

Not researchable at plan time; these are what the measurements are *for*.

- Whether the stack **links** (not merely checks) into a device app, and whether `cargo check`'s pass
  survives codegen for `staticlib`.
- Peak footprint, per-operation wall times, and installed size — the spike's entire output.
- Whether mmap'd weights actually reduce `phys_footprint` versus a heap buffer (D12).
- Whether candle 0.9.2 produces an embedding within tolerance of the Python reference on device.
- Whether tantivy's `MmapDirectory` behaves inside the iOS application sandbox.
