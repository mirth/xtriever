# Spike Report: 001 — iOS Build Spike

**Status**: measured on device. Peak footprint **238.1 MB against a 300 MB ceiling — PASS**, twice, on an iPhone 16e. | **Started**: 2026-09-10 | **Updated**: 2026-09-11

Evaluated against constitution **v1.1.0** (Principle VII's unsafe clause expanded 2026-09-11).

Close-out artifact for Feature 001 (FR-024 – FR-027, SC-008 – SC-010). Every item in FR-001–FR-022
appears below with a verdict of `pass`, `fail`, or `untested`; **no item may be left blank**. Every
`fail` needs a Finding with a smallest reproduction, and every design-constraining Finding needs an
ADR.

A reviewer without an iPhone should be able to read this and reach the same conclusions as its
author, with no measurement taken on trust (SC-010).

---

## Environment

| | |
|---|---|
| Host | macOS, Apple silicon, Xcode 26.6 (iPhoneOS26.5.sdk) |
| Toolchain | 1.91.1 (rustup), pinned by `rust-toolchain.toml` |
| Provenance guard | `scripts/check-toolchain.sh` — PASS |
| `CANDLE_NUM_THREADS` | not yet applicable (no inference in PR 1a) |

`scripts/check-toolchain.sh` output for this run:

```
check-toolchain: PASS — cargo 1.91.1 (ea2d97820 2025-10-10) | rustc 1.91.1 (ed61e7d7e 2025-11-07)
                 | active=1.91.1 | targets=aarch64-apple-ios,aarch64-apple-ios-sim | sdk=iPhoneOS26.5.sdk
```

**Every cross-target verdict below was recorded with that guard passing.** See Finding F-001: at the
time those verdicts were taken the guard did *not* pass in a default shell, and the resulting
failure is indistinguishable from a genuine iOS portability failure. That environment issue is now
resolved, and the verdicts were recorded with the correct toolchain either way.

## Resolved dependency versions (FR-004)

Read from `Cargo.lock`, not from `Cargo.toml` requirements.

| crate | resolved | features |
|---|---|---|
| `tantivy` | 0.26.2 | `mmap`, `stopwords`, `lz4-compression`, `stemmer` (default **minus** `columnar-zstd-compression`) |
| `tokenizers` | 0.23.2 | `fancy-regex` only (`onig`, `esaxx_fast` off) |
| `candle-core` | 0.9.2 | default (`[]`) — pinned by [ADR-0001](../../docs/adr/0001-pin-candle-0-9-2.md) |
| `candle-nn` | 0.9.2 | default |
| `candle-transformers` | 0.9.2 | default |
| `uniffi` | 0.32.1 | default |

## Feature-set matrix (FR-001, FR-002, SC-001)

`cargo check -p xtriever-ffi --features spike --target <triple>`. Verdicts are **per crate per
target**; a pass on the simulator is never recorded as a pass on the device.

| crate | `aarch64-apple-ios` | `aarch64-apple-ios-sim` |
|---|---|---|
| `tantivy` 0.26.2 | **pass** | **pass** |
| `tokenizers` 0.23.2 | **pass** | **pass** |
| `candle-core` 0.9.2 | **pass** | **pass** |
| `candle-transformers` 0.9.2 | **pass** | **pass** |

**8 of 8 verdicts pass** (SC-001). `uniffi` 0.32.1 and `candle-nn` 0.9.2 also compile on both, giving
12 of 12 across the full pinned set.

### Features disabled, and the native dependency each would have pulled (FR-002)

| crate | feature disabled | would have pulled |
|---|---|---|
| `tantivy` | `columnar-zstd-compression` (**on by default**) | `zstd` → `zstd-safe` → **`zstd-sys`** (C, via `cc`) |
| `tokenizers` | `onig` | `onig` → **`onig_sys`** (C, Oniguruma, via `cc`) — banned outright by `deny.toml` |
| `tokenizers` | `esaxx_fast` | `esaxx-rs/cpp` (**C++**, via `cc`) |
| `candle-core` | all of `accelerate`, `mkl`, `cuda`, `cudnn`, `metal`, `nccl` | system/C libraries; default is `[]` so none is on |

"tantivy default features minus anything pulling C deps" was **not** a no-op: the default set does
pull C, and the C-free set is exactly default minus `columnar-zstd-compression`.

### C/C++ freeness (FR-003)

Per target, `cargo tree -e normal,build` matched **none** of `onig|zstd|-sys v|cc v[0-9]`:

| target | native-code indicators |
|---|---|
| `aarch64-apple-ios` | none — no `cc`, no `-sys`, no `onig`, no `zstd` |
| `aarch64-apple-ios-sim` | none |

`esaxx-rs v0.1.10` **is** present with an **empty feature set**. That is expected and is not a
violation: its C++ build is gated behind its own `cpp` feature (only in *its* default), and
`tokenizers` declares it with `default-features = false`. The decisive signal is that `cc` is absent
from the graph entirely, so nothing compiles C or C++.

No C dependency is reachable from any pure crate:

| crate | result |
|---|---|
| `xtriever-core` | clean |
| `xtriever-analysis` | clean |
| `xtriever-pipeline` | clean |
| `xtriever-ltr` | clean |
| `xtriever-eval` | clean |

### Link-and-codegen verification (T025)

`cargo check` performs no codegen and no linking, so it cannot complete US1 on its own.

```
cargo build -p xtriever-ffi --features spike --release --target aarch64-apple-ios
    Finished `release` profile [optimized] target(s) in 1m 53s

lipo -info target/aarch64-apple-ios/release/libxtriever_ffi.a
    Non-fat file: ... is architecture: arm64
```

- `libxtriever_ffi.a` — **16 MB**, arm64, iOS. (Skeleton only; no fixtures, no model, no harness.)
- 54 exported `uniffi` symbols, including all three operations:
  `_uniffi_xtriever_ffi_fn_func_spike_index`, `_uniffi_xtriever_ffi_fn_func_spike_query`,
  `_uniffi_xtriever_ffi_fn_func_spike_embed`.

The three stacks therefore **compile, codegen and link** for a real iOS device target.

---

## Golden fixtures (FR-011 – FR-016, PR 1b)

Generated by `reference/gen_001_fixtures.py` inside the pinned venv (Python 3.12 — the system
interpreter is 3.14, which torch publishes no wheel for). The generator refuses to run outside that
venv, for the same reason `check-toolchain.sh` exists: a run under a stray interpreter with a
different torch would silently produce a *different golden* and nothing would complain.

| fixture | bytes | what it pins |
|---|---:|---|
| `corpus.json` | 253,810 | 1,000 ASCII documents, the query, the sentence, seed 1 |
| `bm25_reference.json` | 2,887 | independent Python BM25 — the Principle II parity oracle |
| `tokens.json` | 5,707 | reference tokenization at **256** tokens |
| `embedding.json` | 10,234 | 384-dim reference vector, L2-normalized |
| `model.json` | 622 | the pinned weights' identity |
| `ranking.json` | 295 | placeholder — host↔device oracle, minted in PR 2 |
| `manifest.json` | 710 | SHA-256 per fixture, making the goldens tamper-evident |

**Model verified three ways** (FR-016, FR-033), not assumed: exactly **90,868,376 bytes**,
**103 F32 tensors** read from the safetensors header, SHA-256
`53aa51172d142c89d9012cce15ae4d6c…`, at revision `1110a243fdf4706b3f48f1d95db1a4f5529b4d41`. The 87.1 MiB of weights live in
gitignored `reference/models/001/` and are **not** committed.

**The BM25 parity fixture is defensible by construction.** 72
documents have a lossy fieldnorm, so the u8 quantization the transcription has to reproduce is
genuinely exercised; and the smallest gap between adjacent top-k scores is
**0.004664**, far above the `1e-05` comparison tolerance —
so the ordering it asserts is a real signal rather than float noise. The generator *refuses to emit
the fixture* if either condition fails, which is how the first attempt was caught: a single-term
query over uniform-length documents produced a top-10 with a zero score gap.

## Tests committed failing (FR-013, Agent Operating Rule 4)

```
cargo nextest run -p xtriever-ffi --features spike
  Summary  14 tests run: 7 passed, 7 failed, 0 skipped
```

The 7 passing are `fixtures_valid.rs`, which is deliberately **not** spike-gated. That split is what
makes "fails for the right reason" mechanical instead of a judgement call: fixtures green + oracles
red means the implementation is missing; fixtures red would mean the goldens are broken and any
other red is uninterpretable.

Every one of the 7 failures is attributable to missing implementation — **zero** to a broken
fixture:

| test | failure |
|---|---|
| `index_query::indexing_the_corpus_produces_one_segment` | `NotImplemented { operation: "spike_index" }` |
| `index_query::query_matches_the_golden_ranking_exactly` | ranking.json is still a placeholder (mint in PR 2) |
| `bm25_parity::bm25_scores_match_the_independent_python_reference` | `NotImplemented { "spike_index" }` |
| `determinism::repeated_indexing_yields_identical_rankings` | `NotImplemented { "spike_index" }` |
| `tokenize::tokenization_matches_the_python_reference_exactly` | `NotImplemented { "tokenize" }` |
| `embed::embedding_matches_the_python_reference_within_tolerance` | `NotImplemented { "spike_embed" }` |
| `load_paths::buffered_and_mmapped_loaders_agree_bit_for_bit` | `NotImplemented { "spike_embed" }` |

The stubs contain **no** backend code — `grep` for `tantivy`/`candle`/`tokenizers` in
`src/spike/*.rs` matches only doc comments — so the red state is genuine TDD, verifiable from the
diff rather than taken on trust.

CI stays green throughout: it runs `cargo nextest run --workspace` **without** `--all-features`, so
the spike tests compile to nothing there (9 passed, 0 skipped).

## The three operations run, and the oracles hold (PR 2a)

`spike_index`, `spike_query` and `spike_embed` are implemented. The suite that was committed red in
PR 1b is now **14 of 14 green**, which is what actually validates the oracle apparatus — until
something ran against them, the fixtures were only an assertion.

Three risks named in the plan are retired, each by a passing test rather than an argument:

| risk | result |
|---|---|
| Does the 256-token override reproduce `tokens.json` **exactly**? (research D6) | **yes** — `tokenize` passes on exact equality |
| Does candle 0.9.2's BERT match torch 2.14 within cosine 0.9999 / 1e-3? | **yes** — `embed` passes |
| Does the single-threaded writer give `segment_count == 1`, and do tantivy's f32 scores match the Python transcription within 1e-5? | **yes** — one segment; worst relative difference **8.3e-08**, ~120× inside tolerance |

`ranking.json` is now **minted** from a real host tantivy run
(`documents_indexed: 1000`, `segment_count: 1`), and the
generator refused to write it until its own independent BM25 agreed on both order and scores. The
fixture records each score's **f32 bit pattern** as well as its decimal, so the host↔device oracle
compares bits rather than a JSON round-trip that can silently lose a ULP.

**ADR-0002 condition 4 is satisfied**: `load_paths` shows the safe buffered loader and the `unsafe`
mmap loader produce bit-identical embeddings. The mmap path therefore survives to be measured on
device, which is the whole reason the exemption was granted.

### Correction to research D15

D15 said candle reads `CANDLE_NUM_THREADS`. In the pinned **0.9.2**, `candle_core::utils::get_num_threads`
reads **`RAYON_NUM_THREADS`** only, and there is no setter. The crate therefore does not pin the
thread count itself — a library mutating process-global environment is wrong, and in edition 2024
`std::env::set_var` is `unsafe`, which would have spent an exemption ADR-0002 reserves for the mmap
loader. Instead the **caller** exports `RAYON_NUM_THREADS=1` and `spike::embed::thread_count()`
reports what was actually in effect, for the device run to record.

## User Story 2 — the boundary works (PR 2b)

`xcodebuild test` on the **iOS 26.5 simulator**, 4 of 4 passing:

| test | proves |
|---|---|
| `testIndexAndQueryCrossTheBoundary` | 1,000 documents indexed into **one segment**; a 10-hit ranking returns in strictly descending score order |
| `testEmbeddingCrossesAsFloatArray` | candle runs BERT **on an iOS target** and a 384-dim L2-normalized vector crosses as `[Float]` |
| `testRustErrorsArriveAsCaughtSwiftErrors` | a missing model directory throws a caught `SpikeError.Model` — **not** a process abort |
| `testQueryErrorsAreTypedNotFatal` | an unopened index throws `IndexIo`; a query matching nothing throws `QueryParse` rather than passing vacuously |

FR-006, FR-007 and FR-008 are therefore verified, not merely declared. The XCFramework carries both
`ios-arm64` and `ios-arm64-simulator` slices and is reproducible from
`scripts/build-ios-harness.sh`; nothing binary is committed.

Fixtures and weights reach the Simulator by environment variable (`TEST_RUNNER_XTRIEVER_*`) rather
than being bundled, so the Swift tests read exactly the same bytes the Rust tests do. A device run
cannot do that and must bundle both — which is why installed binary size (FR-018) is a PR 3
measurement.

## Device measurement (User Story 3, FR-017 – FR-022)

**iPhone 16e (`iPhone17,5`), iOS 26.6.1, Release, `RAYON_NUM_THREADS=1`, thermal state nominal.**
Two runs on the same commit and device. Raw records committed verbatim in
[`runs/`](./runs/) so no number here has to be taken on trust (SC-010).

### The verdict (FR-020, SC-004)

| | run 1 | run 2 |
|---|---:|---:|
| baseline footprint | 26.3 MiB | 26.4 MiB |
| **peak footprint** | **238.1 MB** | **238.3 MB** |
| ceiling | 300 MB | 300 MB |
| headroom | 61.9 MB (21%) | 61.7 MB (21%) |
| **verdict** | **PASS** | **PASS** |

The two peaks differ by **0.055%**, so the memory result is solidly reproducible.

The device's *own* per-app limit was **3.54 GB** — about 11.8× the constitutional ceiling. These are
two different thresholds and the report keeps them apart deliberately: the spike passed the budget
Xtriever imposes on itself, nowhere near the one the hardware imposes.

### The headline: memory-mapped weights cut footprint ~40× (ADR-0002)

This is what [ADR-0002](../../docs/adr/0002-unsafe-mmap-safetensors-measurement.md) was commissioned
to find out, and the answer is unambiguous:

| load path | Δfootprint run 1 | Δfootprint run 2 | wall run 1 | wall run 2 |
|---|---:|---:|---:|---:|
| `buffered` (safe) | 101.14 MB | 101.17 MB | 214.5 ms | 142.7 ms |
| `mmapped` (`unsafe`) | **2.56 MB** | **2.56 MB** | 122.0 ms | 117.0 ms |

**~99 MB saved, a 39.6× reduction**, identical to three significant figures across both runs. The
90.9 MB of fp32 weights essentially do not count toward `phys_footprint` when mapped — exactly the
hypothesis the `unsafe` exemption was granted to test. ADR-0002 condition 4 also holds: both paths
produced **bit-identical** embeddings, so the mmap path stays.

**One caveat, stated because it limits the claim.** Both paths run in the *same process*, buffered
first. The mmap load is therefore measured against an allocator that has just freed ~101 MB, so
`2.56 MB` is a marginal cost, not a from-cold one. The 40× gap is far too large for ordering to
explain it away, but a rigorous comparison would run each path in a fresh process. Recorded as a
follow-up rather than asserted away.

Note also that peak stays at 238 MB in both runs *because* the buffered path ran first — peak is a
process-lifetime high-water mark. A mmap-only configuration would very likely peak far lower, but
that number was not measured and is not claimed.

### Determinism holds across platforms (FR-014, Principle VI)

The on-device ranking was **bit-identical** to the host-minted golden — compared as `f32` bit
patterns, not decimals. tantivy produces the same BM25 scores on an iPhone as on an Apple-silicon
Mac, down to the last bit, and the top hits match the independent Python BM25 as well:

```
doc-000394@7.622925   doc-000362@7.4664707   doc-000361@7.2195134
```

The embedding matched the Python/torch reference within the stated tolerance (cosine ≥ 0.9999,
max abs diff ≤ 1e-3), and `segment_count == 1` in both runs, so the comparison was meaningful.

### Wall times, and a reproducibility failure (FR-022, SC-007)

| operation | run 1 | run 2 | spread |
|---|---:|---:|---:|
| index (1,000 docs) | 132.2 ms | 95.6 ms | **38.3%** |
| query | 3.23 ms | 0.94 ms | **242%** |
| embed (buffered) | 214.5 ms | 142.7 ms | **50.3%** |
| embed (mmapped) | 122.0 ms | 117.0 ms | 4.3% |

**Three of four operations exceed the ±20% reproducibility band this spec states** (spec
Assumptions). That is a **FAIL** on SC-007 for wall time, and it is recorded as such rather than
fixed by widening the band — the band is an oracle and FR-028 forbids relaxing one to obtain a pass.
See finding F-009.

Memory reproduced at 0.055%; only *timing* is affected.

## Item verdicts

`untested` means not yet attempted — never inferred from a related result (FR-024, spec US4
scenario 4).

| FR | Item | Verdict |
|---|---|---|
| FR-001 | Per-crate/per-target build verdicts | **pass** (8/8, plus 4 more) |
| FR-002 | Feature sets and avoided native deps recorded | **pass** |
| FR-003 | No C/C++ reachable from the pure crates; confined behind a non-default feature | **pass** |
| FR-004 | Deps added via `cargo add`, resolved versions recorded | **pass** |
| — | `cargo deny check` green (T006) | **pass** — all four sections ok, after ADR-0004 |
| FR-005 | No vendoring, patching or forking | **pass** — none applied |
| FR-006 | Exactly three operations exposed | **pass** — three, and no more |
| FR-007 | Corpus in; ranked hits and a float vector out | **pass** — behaviour verified against the goldens |
| FR-008 | Rust errors reach Swift as typed errors | **pass** — verified on the simulator |
| FR-009 | No `xtriever-core` trait modified | **pass** — `git diff` on the crate is empty |
| FR-010 | No timing/async/threads/C in the pure crates | **pass** — all spike code is in `xtriever-ffi` |
| FR-011 – FR-016 | Fixtures and oracles | **pass** — committed, verified, and tamper-evident (PR 1b) |
| FR-017 | Binary size, footprint, per-operation wall time recorded | **pass** — footprint and timing; binary size outstanding (F-010) |
| FR-018 | Installed size broken down | **fail** — not measured; needs an archive + App Thinning Size Report (F-010) |
| FR-019 | Device model, iOS version, build config, thermal state recorded | **pass** — iPhone17,5 / 26.6.1 / Release / nominal |
| FR-020 | Peak footprint vs the 300 MB ceiling | **pass** — 238.1 MB, 21% headroom, twice |
| FR-021 | 1% caveat stated | **pass** — see "What this does not tell us" |
| FR-022 | Repeated runs, reproducibility measured | **pass** (runs done) / **fail** on the timing band — see F-009 |
| FR-023 | No performance budgets set | **pass** — none set; see "Baseline, not budget" |
| FR-024 – FR-027 | Findings discipline | **in progress** — this document |
| FR-028 | No oracle weakened | **pass** — see F-002, reported rather than worked around |
| FR-029 | No retrieval stage implemented beyond the minimum | **pass** — stubs contain no backend code |
| FR-030 | Android and wasm32 recorded as untested | **pass** — see below |
| FR-031 – FR-033 | Provisional binding; pinned fp32 weights | **pass** — weights verified by size, hash and header dtype |

### Android and wasm32 (FR-030)

**untested.** Neither was attempted, and neither is inferred from the iOS result. The one measured
data point: `wasm32-unknown-unknown` fails at `getrandom` 0.3.4, which requires the `wasm_js` backend
flag — a transitive-dependency configuration issue, not anything about tantivy, candle or tokenizers.
Principle III makes wasm32 best-effort and tracked.

### Baseline, not budget (FR-023)

No performance budget was set for this spike, deliberately. The numbers it produces are the
**baseline** that later specs set budgets against. The one figure available so far — a 16 MB
skeleton staticlib — excludes the model weights, fixtures and harness, and must not be quoted as an
app size.

### The 1% caveat (FR-021)

Not yet applicable (no measurement taken), but binding on PR 3: **1,000 documents is 1% of the
100,000-chunk configuration the 300 MB ceiling is written against.** No claim about the ceiling at
100k may be made from a 1k measurement.

---

## What this does *not* tell us (FR-021, FR-023)

Three limits on the result above, stated because the numbers are attractive enough to be
over-read.

**1,000 documents is 1% of the configuration the ceiling is written against.** The 300 MB ceiling in
Principle III is specified for a **100,000-chunk** index. Indexing cost **12.13 MB of footprint for
1,000 documents**, averaged across runs. A naive ×100 extrapolation gives **≈1.2 GB** — four times
the ceiling.

That number is an **extrapolation, not a measurement**, and linear scaling is the wrong model for an
inverted index: postings compress, the dictionary sublinearly, and the writer's arena is a fixed
cost partly included in the 12.13 MB. But it is the right shape of the risk, and it points
somewhere specific: **the model weights are a fixed cost the spike has now solved (2.56 MB mapped),
while index memory is the term that grows.** FR-021 forbids claiming the ceiling holds at 100k on
this evidence, and it does not hold on this evidence. Measuring the real curve is the obvious next
spec.

**No budget was set, and none is implied.** Per FR-023 these are baseline numbers for later specs to
set budgets against — 132/96 ms to index 1,000 documents is a starting point, not a target anyone
has agreed to.

**One device, one OS, two runs.** iPhone 16e on iOS 26.6.1. Apple publishes no per-device memory
limits, and this device reported a 3.54 GB per-app limit; a smaller device would report less. None
of this generalises across device classes, and the report records the hardware precisely so nobody
tries.

## Findings

### F-001 — Homebrew `rustc` shadows rustup, producing a false iOS build failure — **RESOLVED**

**Verdict impact**: none (guard in place). **Severity**: high — it silently faked the exact failure
this spike exists to detect. **Resolved 2026-09-11**: the Homebrew Rust install was removed;
`cargo` and `rustc` both resolve under `~/.cargo/bin` and `scripts/check-toolchain.sh` passes with
no `PATH` manipulation.

`/opt/homebrew/bin/rustc` precedes `~/.cargo/bin/rustc` on `PATH`. The Homebrew toolchain ignores
`rust-toolchain.toml` and ships only the host `std`, so cargo — even rustup's cargo — invokes it and
every cross-target build fails with `error[E0463]: can't find crate for 'std'`. That is
indistinguishable from a genuine portability failure.

Smallest reproduction:

```sh
cd $(mktemp -d) && cargo new --lib p -q && cd p
cp /path/to/xtriever/rust-toolchain.toml .
cargo check --target aarch64-apple-ios          # E0463
PATH="$HOME/.cargo/bin:$PATH" cargo check --target aarch64-apple-ios   # Finished
```

Note the trap is **partial**: removing Homebrew's `cargo` alone is not sufficient, because `rustc` is
resolved independently and can still be Homebrew's. `scripts/check-toolchain.sh` checks both, plus
the active toolchain and the iOS SDK.

**Resolution**: the underlying `PATH` was fixed on 2026-09-11 by removing the Homebrew Rust
install; the guard now passes in a fresh shell with no `PATH` manipulation. The guard itself
stays, for two reasons: CI runners and other developer machines can still hit this, and the trap
**changed shape** mid-investigation — removing Homebrew's `cargo` alone was not sufficient,
because its `rustc` shadowed rustup's independently. A guard checking only `cargo` would have
reported PASS while cross-target builds still failed.

### F-002 — `cargo deny check` fails on an unmaintained transitive crate — **RESOLVED**

**Verdict impact**: T006 was blocked; **now unblocked**. `cargo deny check` is a blocking CI gate.

```
advisories FAILED, bans ok, licenses ok, sources ok
error[unmaintained]: paste - no longer maintained
  ID: RUSTSEC-2024-0436
```

Path: `candle-core 0.9.2 → gemm 0.19.0 → paste 1.0.15` (also via `gemm-c32`, `gemm-c64`, `pulp`).

Measured before/after, so this is unambiguous:

| | advisories | bans | licenses | sources |
|---|---|---|---|---|
| before adding the spike deps | ok | ok | ok | ok |
| after | **FAILED** | ok | ok | ok |

Two things worth separating. First, **ADR-0001's pin did its job**: `bans` passes, meaning no
`onig_sys`, which is what the candle version pin was for. Second, this is an *unmaintained* advisory,
not a vulnerability — `paste` is a proc-macro with no known CVE — and it is unavoidable while using
candle, since `gemm` is candle's CPU matmul backend.

**Resolution (authorized 2026-09-11).** `deny.toml` gained `ignore = ["RUSTSEC-2024-0436"]` —
**one advisory id, deliberately not** a blanket `unmaintained = "allow"`, so the next unmaintained
crate still fails the gate. Rationale and review triggers in
[ADR-0004](../../docs/adr/0004-ignore-rustsec-2024-0436.md). `[bans]` is untouched, so `onig_sys`
remains denied outright and ADR-0001's tripwire still fires if anyone upgrades candle past 0.9.x.

Post-change: `advisories ok, bans ok, licenses ok, sources ok`.

This is honestly a gate *relaxation*, not a fix — the advisory is still true, we have decided it is
acceptable. It was escalated rather than applied unilaterally (Rules 2 and 6), and it carries review
triggers so it does not rot.

Smallest reproduction: `cargo deny check advisories` at this commit.

---

### F-003 — golden fixtures were not portable: Git rewrote them on Windows — **RESOLVED**

**Verdict impact**: CI red on Windows, green on macOS and Linux. **Severity**: medium — the failure
message accused the fixture of being hand-edited, which is the wrong place to look.

`cargo nextest run --workspace` failed on the Windows runner:

```
manifest_hashes_match_every_fixture
  bm25_reference.json does not match its recorded hash
    left:  b32d61fda53349897758708d38e5232150afaf81108fcd65d324c3fce87b880b
    right: a4dbc27daf0b71359c793a0dbaf7e1d3e8032b89e7777c7499b5a2d082eaf6ba
```

**Cause, proven rather than inferred.** The repository had no `.gitattributes`, so Git on Windows
(`core.autocrlf=true` by default) rewrote LF to CRLF on checkout. Converting the local fixture to
CRLF and hashing it reproduces the Windows value exactly:

| bytes | SHA-256 |
|---|---|
| as committed (LF) | `a4dbc27d…` — matches the manifest |
| same file, CRLF | `b32d61fd…` — **matches what Windows computed** |

Only `bm25_reference.json` was named because `serde_json::Map` is a `BTreeMap` and it sorts first;
**every** JSON fixture was affected.

**Smallest reproduction** (on any platform):

```sh
python3 -c "d=open('reference/fixtures/001/bm25_reference.json','rb').read(); \
            open('reference/fixtures/001/bm25_reference.json','wb').write(d.replace(b'\n', b'\r\n'))"
cargo nextest run -p xtriever-ffi --test fixtures_valid manifest_hashes
```

**Resolution.** Added `.gitattributes`: a general `* text=auto eol=lf`, then
`reference/fixtures/** -text` so the goldens are never translated at all. `-text` rather than
`binary` keeps fixture diffs readable, which matters because a change to a golden is exactly what
must be visible in review.

Two things worth carrying forward:

- **Order is load-bearing and was wrong on the first attempt.** In `.gitattributes` the *last*
  matching pattern wins, so the specific rule must follow the general one. Written the other way
  round — which is how it reads more naturally — the `*` line silently re-enables translation for
  the fixtures. Caught only by checking `git check-attr`, not by reading the file.
- **The oracle was not weakened to fix this.** Hashing normalized content would have made the test
  pass everywhere while destroying its tamper-evidence (FR-028). The defect was in the repository
  configuration, and that is where it was fixed. The test now *detects* the line-ending case and
  says so, instead of blaming the fixture:

  > `bm25_reference.json` differs from its recorded hash ONLY by line endings … This is Git
  > translating on checkout, not a bad fixture.

**Not yet confirmed on Windows.** The cause is proven and the fix verified locally by simulating a
CRLF checkout, but the Windows runner is the only place the real checkout path executes. The next CI
run is the confirmation.

### F-004 — `xcodebuild` could not load its own simulator plug-in — **RESOLVED**

`xcodebuild -create-xcframework` failed before doing any work:

```
The plug-in "com.apple.dt.IDESimulatorFoundation" ... could not be loaded.
Symbol not found: _$s12DVTDownloads21DownloadableAssetTypeO...
```

`/Library/Developer/PrivateFrameworks/DVTDownloads.framework` was at bundle version **17.0** while
Xcode is **26.6 (17F113)** — stale system components after an Xcode upgrade. `xcodebuild -showsdks`
and `xcrun simctl` were unaffected, so only calls needing the simulator plug-in failed.

**Resolution**: `xcodebuild -runFirstLaunch`, after which the XCFramework built first try.

### F-005 — Xcode's iOS platform component was not installed — **RESOLVED**

**Blocks**: T036, T037, and all of PR 3.

```
{ platform:iOS, name:Any iOS Device,
  error:iOS 26.5 is not installed. Please download and install the platform from
        Xcode > Settings > Components. }
```

`xcodebuild` offers **no iOS destination at all**, so nothing can be run on a simulator or a device.

What makes this confusing, and worth writing down: the pieces that *look* like iOS support are all
present. The SDKs are installed (`iphoneos26.5`, `iphonesimulator26.5` — which is exactly why
`cargo check --target aarch64-apple-ios` and the release staticlib builds succeed), the
`.platform` directories exist, and simulator runtimes for iOS 17.2, 17.5, 18.2 and 26.0 are
installed. The missing piece is the separately-downloaded iOS *platform component* for 26.5.

**Confirmed environmental, not ours**: a trivial SwiftPM package containing none of this project's
configuration — no binary target, no XCFramework — reports the identical error.

**Smallest reproduction**:

```sh
mkdir -p /tmp/p/Sources/P && cd /tmp/p
printf 'public func f() {}' > Sources/P/F.swift
cat > Package.swift <<'EOF'
// swift-tools-version: 5.9
import PackageDescription
let package = Package(name: "P", platforms: [.iOS(.v16)],
    products: [.library(name: "P", targets: ["P"])], targets: [.target(name: "P")])
EOF
xcodebuild -showdestinations -scheme P     # no iOS destinations
```

**Resolution 2026-09-11**: the platform component was installed. `xcodebuild` now offers iOS destinations and the iOS 26.5 simulator runtime is present, and the harness runs.

### F-006 — the XCFramework modulemap needs an explicit `--module-name` — **RESOLVED**

Exactly the undocumented territory research risk R2 predicted, though not in the shape predicted.

uniffi's generated Swift opens with `#if canImport(xtriever_ffiFFI)`. Generating the modulemap
without `--module-name` declares the module as `xtriever_ffi`, so `canImport` is quietly **false**,
the import is skipped, and every FFI type (`RustBuffer`, `ForeignBytes`, …) reports "cannot find
type ... in scope" — a wall of errors a long way from the one-word cause.

Nothing in the UniFFI guide connects those two names. Found by type-checking the generated bindings
directly with `swiftc` rather than by reading documentation.

There were **two** independent causes, and fixing only the first left the identical error:

1. **Module name.** Without `--module-name xtriever_ffiFFI` the modulemap declares `xtriever_ffi`.
2. **`--xcframework` is the wrong flag here, despite its name.** It emits `framework module`, which
   requires a real `.framework` layout. Our slices are static libraries (`libxtriever_ffi.a`), so
   Clang never matches the module — same wall of errors, different reason. The UniFFI guide
   recommends `--xcframework` for XCFramework use without noting that it assumes a framework rather
   than a static library.

**Resolution**: generate a *plain* modulemap —
`--modulemap --module-name xtriever_ffiFFI --modulemap-filename module.modulemap`, deliberately
**without** `--xcframework`. `scripts/build-ios-harness.sh` carries a comment at the call site
recording why each flag is there and why that one is not.

**Also fixed while here**: `spike_embed` tokenized before verifying the model directory, so a missing
or unbundled model reported `Tokenize` — "tokenization failed" when in fact nothing was there. It now
checks `config.json`, `tokenizer.json` and `model.safetensors` up front and returns `Model` naming the
first missing file. A model that did not make it into the app bundle is the likeliest device-side
failure, and it should say so.

### F-007 — a hostless XCTest bundle cannot run on a device — **RESOLVED**

**My error, not the environment's.** PR 2b chose a SwiftPM package with an XCTest target over the
"three buttons" app T036 specified, and `harness/ios/README.md` justified it partly with *"XCTest
runs on a physical device too, so the same harness serves PR 3 unchanged."* That is wrong. The first
device attempt failed:

```
Cannot test target "XtrieverSpikeAppTests" on iPhone:
Tool-hosted testing is unavailable on device destinations.
Select a host application for the test target, or use a simulator destination instead.
```

Hostless ("tool-hosted") test bundles run only on simulators and macOS. A device needs an app to
host them — which is what T036 asked for, and the reason it asked is now clear.

**Resolution**: `harness/ios/XtrieverSpikeApp/`, a minimal generated iOS app that exists solely to
host the tests. Its test target's sources are the *same* files the SwiftPM package uses, referenced
in place, so there is one copy of the tests running on both destinations. The `.xcodeproj` is
generated by `xcodegen` from a 60-line `project.yml` and gitignored — derived, not authored.

The deviation from "three buttons" stands and still earns its place: the app is inert, and the
measurements run under `xcodebuild test` so the two runs FR-022 requires are one command each rather
than a tapping ritual. What was wrong was the claim that no app was needed at all.

### F-008 — arm64-only slices versus a Release build's default architectures — **RESOLVED**

`ld: symbol(s) not found for architecture x86_64`, which reads like a missing symbol and is really a
missing slice. The Rust staticlibs are arm64-only by design — Principle III names
`aarch64-apple-ios` and `aarch64-apple-ios-sim`, and x86_64 was never in scope — but a Release build
does not restrict itself to the active architecture.

**Resolution**: `ARCHS=arm64` on the **command line**. Worth recording precisely, because the
obvious fix does not work: the same setting in the generated project's build configuration does
*not* reach the SwiftPM package target, and a clean build with only the project setting still fails.
Verified both ways from a cleared DerivedData.

### F-009 — wall-time reproducibility misses the stated ±20% band ⚠️ OPEN

**Verdict: SC-007 FAILS for wall time.** Memory reproduced at 0.055%; timing did not.

| operation | run 1 | run 2 | spread | within ±20%? |
|---|---:|---:|---:|---|
| index | 132.2 ms | 95.6 ms | 38.3% | **no** |
| query | 3.23 ms | 0.94 ms | 242% | **no** |
| embed (buffered) | 214.5 ms | 142.7 ms | 50.3% | **no** |
| embed (mmapped) | 122.0 ms | 117.0 ms | 4.3% | yes |

Run 1 was slower than run 2 in every case, which points at first-run effects — page cache, first
touch of the mapped weights, lazily spawned thread pools — rather than random noise. Thermal state
was `nominal` in both runs, so throttling is not the explanation.

**Not fixed by widening the band.** The ±20% figure is an oracle stated in the spec, and FR-028 plus
Agent Operating Rule 6 forbid relaxing an oracle to obtain a pass. It is recorded as a failure and
carried forward.

What a follow-up should do instead, in rough order of value:

1. **More than two runs.** Two samples cannot separate a cold-start effect from variance; FR-022's
   "at least twice" is a floor, and this is evidence it is too low a floor for timing.
2. **Discard a warm-up run**, which is standard benchmarking practice and what Apple's own
   performance-test tooling does.
3. **Set the band from measured data** rather than from the plausible-looking ±20% guess this spec
   made before any measurement existed. That is precisely what a baseline spike is for — and
   note the *memory* numbers argue the spike's methodology is sound, so this is a band problem, not
   a harness problem.

Nothing here changes the memory verdict, which is what the spike was primarily commissioned to
answer.

## Deviations (FR-027)

### D-001 — the BM25 golden cannot come from Python alone

Task T011 specified that `reference/gen_001_fixtures.py` emit `ExpectedRanking` "by building the host
tantivy index". **Python cannot build a tantivy index**, and the task additionally conflated two
oracles that cannot share one fixture:

| oracle | question | reference | comparison |
|---|---|---|---|
| BM25 parity (Principle II) | is our BM25 the BM25? | an *independent* implementation | scores within `1e-5` |
| Determinism (Principle VI, FR-014) | does iOS produce the same bits as macOS? | the host Rust run itself | bit-exact |

**Resolution** (approved 2026-09-10): two fixtures. `bm25_reference.json` comes from an independent
Python transcription of tantivy's documented BM25 — `k1=1.2`, `b=0.75`, the Lucene IDF, and the
`FIELD_NORMS_TABLE` u8 fieldnorm quantization, all public in tantivy 0.26.2 — compared ids-and-order
exact with scores within `score_rel_tol = 1e-5`. `ranking.json` is minted by a Rust example and
cross-checked by the Python script at mint time, and is the bit-exact host↔device oracle.

**Cost: none.** Principle II's cross-implementation clause is satisfied *more* strongly than T011
would have satisfied it, because the Python side is genuinely independent rather than the same engine
compiled twice. `score_rel_tol` is now stated in the spec, as Principle II requires.

### D-002 — uniffi forces a lint relaxation Principle VII did not anticipate — **no longer a deviation**

`unsafe_code = "deny"` cannot coexist with uniffi: its macros emit `unsafe` at 38 sites and suppress
only `missing_docs` and `clippy::missing_safety_doc`. Contained per
[ADR-0003](../../docs/adr/0003-uniffi-scaffolding-requires-unsafe-allow.md): `src/ffi/` allows it for
generated code, `src/spike/mod.rs` re-denies it for ours.

Verified invariants at this commit:

```sh
grep -rn 'allow(unsafe_code)' crates/xtriever-ffi/src/   # lib.rs + ffi/{mod,types,error}.rs only
grep -rn 'unsafe' crates/xtriever-ffi/src/spike/*.rs     # NONE (one // SAFETY: block arrives in PR 2)
```

This amends ADR-0002 condition 5, which had assumed the mmap call would be the crate's only `unsafe`.

**Superseded as a deviation on 2026-09-11**: rather than carry a standing exception, Principle VII
was expanded to admit this case and the constitution went to **v1.1.0** (Governance: expanded
principle → MINOR bump, PR + ADR-0003 + human approval). The containment design above is now the
rule, not a departure from it, and the Principle VII gate row passes cleanly.

---

## Open decisions

None. Every blocker raised so far is resolved: the advisory ignore (ADR-0004) and
the Principle VII amendment (constitution v1.1.0, on ADR-0003's evidence).

## Untested, and why

| Item | Reason |
|---|---|
| FR-011 – FR-016 (fixtures, oracles) | PR 1b — needs a Python 3.12 venv, torch, and the 87.1 MiB weights download |
| FR-008 (typed errors in Swift) | PR 2 — needs the simulator |
| FR-017 – FR-022 (all measurements) | PR 3 — needs a physical iPhone and provisioning |
| Android, wasm32 | out of scope (FR-030) |

## Known issue not fixed here

`crates/xtriever-core/src/traits.rs:21` cites "ADR-0001" for the deferred writer/reader split, but
`docs/adr/0001` is the candle pin — a dangling reference created when that ADR was numbered. Fixing
it means editing `xtriever-core`, which FR-009 and Rule 2 forbid uninvited. Recommend either
renumbering or writing the missing ADR under a separate change.
