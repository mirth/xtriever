# Upstream issue draft — candle's non-optional `tokenizers`/`onig` dependency

Follow-up action from [ADR-0001](../adr/0001-pin-candle-0-9-2.md). **Not filed** — `gh` is not installed
here, and filing to a third-party repository is a decision for a human to make and sign. Paste as-is
into <https://github.com/huggingface/candle/issues> if you want it raised.

Only defect (a) needs an issue. Defect (b) — the nightly-only `stdarch_neon_f16` — is already fixed
upstream by PR #3845 and just needs a release; track that instead of reporting it.

---

**Title**: `candle-core` unconditionally pulls `tokenizers` with the `onig` feature, forcing a C toolchain

**Body**:

`candle-core` declares, for every non-wasm32 target:

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
tokenizers = { workspace = true, features = ["onig"] }
```

The dependency is **normal and non-optional**, and it hard-codes `onig`. The resulting chain is:

```
onig_sys (C, built with cc)
└── onig
    └── tokenizers
        └── candle-core
```

There is no feature flag to disable it, so every consumer of `candle-core` on a non-wasm target
inherits a C build dependency.

**Why this is a problem for us.** We are a pure-Rust retrieval engine targeting iOS and Android, with
a policy of no C/C++ build dependencies outside explicitly named leaf crates. We already depend on
`tokenizers` directly with `default-features = false, features = ["fancy-regex"]` precisely to avoid
Oniguruma. `candle-core` reintroduces it transitively and unconditionally, and also pins a second
copy of `tokenizers` (0.22.x alongside our 0.23.x).

Cross-compiling `onig_sys` works, so this is not a hard blocker — it is a policy and
supply-chain-surface problem, plus a duplicate-version one.

**What it appears to be needed for.** As far as we can tell the dependency exists only for
`candle-core/src/quantized/tokenizer.rs`, which builds a BPE tokenizer from GGUF metadata. Consumers
who never touch GGUF still pay for it.

**Suggested fix**, in order of preference:

1. Make the dependency **optional** behind a feature (`gguf-tokenizer`, say), defaulting off.
2. Failing that, depend on `tokenizers` with `default-features = false` and let the consumer choose
   the regex engine — `fancy-regex` is pure Rust and covers the same ground for this use.
3. Move the GGUF tokenizer helper into `candle-transformers`, which already depends on
   `fancy-regex` rather than `onig`.

**Versions**: introduced in `candle-core` 0.10.x; 0.9.2 and earlier have no `tokenizers` dependency
at all. We are pinned to 0.9.2 for this reason.

---

## Also worth watching, not worth filing

**PR #3845, "Avoid unstable AArch64 FP16 vector type"** (merged 2026-08-13) fixes the other defect
that pins us: `candle-core` 0.11.0 uses the nightly-only `stdarch_neon_f16` and therefore fails to
build on stable Rust for any target with `target_feature = "fp16"` — which includes Apple-silicon
macOS and the iOS simulator. The fix is on `main` and in no released version at time of writing.

**Both** must be resolved before we can move off the 0.9.2 pin. The FP16 fix alone is not enough,
because the `onig` dependency would still be there — see ADR-0001's review triggers.
