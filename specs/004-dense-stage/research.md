# Phase 0 Research: The Dense Stage

**Feature**: `004-dense-stage` | **Date**: 2026-09-12 | **Plan**: [plan.md](./plan.md)

Everything below that could be read was read from the pinned crate sources in the local cargo
registry (`candle-core`/`candle-nn`/`candle-transformers` **0.9.2**, `tokenizers` **0.23.2**,
`memmap2` **0.9.11**, all already in `Cargo.lock`), and everything that could be measured was
measured on this host on 2026-09-12 (Agent Operating Rule 1; Principle IV). Items are cited as
`crate-version/path:line`.

---

## D1. Both weight loaders copy the tensors onto the heap — Feature 001's "39.5×" was a measurement artifact

**Finding**: candle 0.9.2 materialises every safetensors tensor as an owned CPU buffer regardless
of how the file bytes were obtained. `candle-core-0.9.2/src/safetensors.rs:115-137`
(`convert_slice`) ends in `Tensor::from_slice(data, shape, device)` on both the aligned and the
unaligned branch, and `Tensor::from_slice` on `Device::Cpu` copies into a `CpuStorage::F32(Vec)`.
`MmapedSafetensors::load` (line 489) and `BufferedSafetensors` (line 542) both go through
`convert_`. So after `BertModel::load` returns, the ~87 MiB of weights live on the heap in **both**
paths; the mapped file (or the `Vec<u8>`) is no longer referenced.

What the two paths differ in is the **transient** during load: the buffered path holds the
90,868,376-byte `Vec<u8>` *and* the tensor copies at once (~174 MiB peak), the mapped path holds
only the copies plus whatever pages the kernel has paged in (clean, evictable). Feature 001's
report measured Δfootprint = 2.56 MB for mmap *after* the buffered path had run and freed ~101 MB
in the same process — the report itself flagged that ordering as a caveat ("a marginal cost, not
a from-cold one"). D1 explains why the marginal cost was near zero: the allocator handed the tensor
copies pages it had just retained.

**Decision**: keep the spec's FR-008 (both paths, one Cargo feature) — the user chose it and it is
still the right shape — but the plan states the benefit honestly: **mmap lowers the peak during
load by roughly the weight-file size and changes nothing about steady state**. FR-023 measures
each path **in a fresh process** (D12), which is the measurement 001 said it owed. ADR-0007 (D3)
carries a condition keyed to that measurement.

**Consequence for the code**: both paths feed the same safe constructor,
`VarBuilder::from_slice_safetensors(&[u8], DType, &Device)`
(`candle-nn-0.9.2/src/var_builder.rs:658`), so the only difference between them is how the byte
slice is obtained — `std::fs::read` or a `memmap2::Mmap`. The `unsafe` surface is therefore
**one** block, `memmap2::MmapOptions::map` (`memmap2-0.9.11/src/lib.rs:429`), and the vector index
(D8) reuses the same function, which is what the spec's "one flag, one ADR" assumption needs.
`from_mmaped_safetensors` (`var_builder.rs:642`, `unsafe fn`) is not used.

## D2. Bit-identity across batches is not free: candle folds the batch into M

**Finding**: `candle-core-0.9.2/src/cpu_backend/mod.rs:1400-1403` — when the right-hand side is
broadcast across the batch (every `Linear` in BERT: `b_skip == 0 && a_skip == m * k`) the CPU
matmul rewrites `(b, m, n, k)` as `(1, b·m, n, k)` and issues **one** `gemm` call with
`M = batch × seq_len`. `gemm` 0.19 selects its kernel and blocking from the problem dimensions, so
the same text in a batch of 1 and a batch of 8 goes through calls with different `M`; whether
every output element's reduction order is unchanged is a property of `gemm`'s internals, not a
guarantee.

**Decision**: the embedder runs the model **one text at a time** (batch dimension 1, sequence
padded to a fixed 256). Every text then has an identical tensor shape on every call, and
FR-005/SC-002 (bit-identical across batch composition, order and size) hold by construction rather
than by luck. The caller-facing `embed(&[&str])` is a loop. Throughput batching is a later,
*measured* optimisation — it would need SC-002's three-arrangement test to stay green.

**Fixed padding to 256** follows for the same reason (dynamic padding changes `M` per batch) and
matches the Feature 001 oracle (`gen_001_fixtures.py:335-338` pads/truncates to 256 on the Python
side; `tokenizer.json` ships 128 and both sides override it — research D6 of 001).

**Cost, measured**: the spike's whole `spike_embed` call (verify 90 MB hash + build model +
tokenize + forward, mmap path, `RAYON_NUM_THREADS=1`, release) averages **259 ms** on this host
(Apple M1 Pro) for a short text and **258 ms** for a >256-token text — identical because padding
is fixed. At 4 threads: 200 ms. The forward pass alone is bounded above by that; a fair estimate
is 100–150 ms per text single-threaded. FiQA's 57,638 passages are therefore **1.6–2.4 h at one
thread**, ~1 h at four, which is the "roughly an hour" the spec assumed. The quickstart records
the real number.

## D3. Thread count: parallelism is over rows and tiles, never inside a reduction — to be verified, not assumed

**Finding**: the CPU backend's parallelism (`cpu_backend/mod.rs:7` `use rayon::prelude::*`) is
per output channel in convolutions (not used), `gemm`'s `Parallelism::Rayon(n)` (line 1396) which
partitions output tiles, and `candle-nn-0.9.2/src/ops.rs:301` softmax over `par_chunks(dim_m1)` —
**one row per task**, with the row's own reductions (`vec_reduce_max`, `vec_reduce_sum`) sequential
inside the task. `candle-core-0.9.2/src/cpu/kernels.rs:205-235` (`par_for_each`, `par_range`) split
*indices*, not sums. Nothing read splits a single reduction across threads, so the thread count
should not change bits. `get_num_threads` reads **`RAYON_NUM_THREADS`** only
(`candle-core-0.9.2/src/utils.rs:4-13`); there is no setter and the library will not set
environment.

**Decision**: FR-005's thread-count clause is tested the way Feature 002 tested cross-process
determinism — the test spawns `current_exe()` with `RAYON_NUM_THREADS=1` and `=4` and compares
the golden set bit-for-bit. If it fails, that is a Rule 6 stop-and-report, and the likely
resolution is a spec amendment scoping bit-identity to a recorded thread count — not a weakened
test. The baseline records the thread count it ran with (D11).

**Architecture caveat, stated so the fingerprint is not over-promised**: `vec_reduce_sum` and
`gemm` use different SIMD paths on arm64 and x86-64, so "same fingerprint ⇒ bit-identical" holds
**on one architecture**. Across architectures Feature 001 measured agreement within tolerance
(embedding within the torch tolerance host↔device; BM25 bit-identical). The fingerprint deliberately
does **not** include the architecture — an index built on a Mac must open on a phone — and the
contract says so.

## D4. Model files: three pins, verified before anything is parsed

**Measured** (`reference/models/001/`, the files Feature 001 downloaded at revision
`1110a243fdf4706b3f48f1d95db1a4f5529b4d41`):

| file | bytes | SHA-256 |
|---|---:|---|
| `config.json` | 612 | `953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41` |
| `tokenizer.json` | 466,247 | `be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037` |
| `model.safetensors` | 90,868,376 | `53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db` |

`config.json` (read): `hidden_size 384`, `num_hidden_layers 6`, `num_attention_heads 12`,
`intermediate_size 1536`, `max_position_embeddings 512`, `vocab_size 30522`, `pad_token_id 0`,
`model_type bert`. The safetensors header carries **103 `F32` tensors** plus one `I64`
`embeddings.position_ids` buffer (spike `embed.rs:186-196`).

**Decision**:

- The pins are **compiled-in constants** in `xtriever_dense::model::PINNED` (repository, revision,
  the three `{name, bytes, sha256}` entries) *and* a committed `reference/models/manifest.json`
  read by `scripts/fetch-model.sh`; a test asserts the two agree so neither can drift.
- Load order (FR-003): stat size → stream-hash in 1 MiB chunks (the spike's method, so verifying
  does not itself allocate the file) → only then parse. Every failure is
  `Error::Model { model, message }` naming the file, the expected and the actual value. The
  tokenizer is built with `Tokenizer::from_bytes` (`tokenizers-0.23.2/src/tokenizer/mod.rs:473`)
  from the verified bytes, not `from_file`, so nothing reads a file twice or unverified.
- Load-time assertions (FR-002): from `config.json` — `hidden_size == 384`, `model_type == "bert"`,
  `max_position_embeddings >= 256`, `vocab_size == 30522`; from the safetensors header — dtype
  `F32` on every weight tensor and exactly 103 of them; from the tokenizer — pad id 0. **Pooling
  and normalisation are ours** (the reference is `AutoModel` + mask-weighted mean + L2, not a
  pooling config file); they are asserted by the golden oracle and named in the fingerprint. The
  plan records this as the meaning of "asserted, never assumed" for those two.
- `scripts/fetch-model.sh` fetches the three files from
  `https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/<revision>/<file>` with
  the `fetch-beir.sh` retry/timeout discipline into `reference/models/all-MiniLM-L6-v2/`
  (git-ignored by the existing `reference/models/` rule) and verifies size + SHA-256. Tests find
  the directory via `XTRIEVER_MODEL_DIR` or that default (the 001 pattern).

## D5. The forward pass, item by item (Rule 1 citations)

| step | item |
|---|---|
| tokenizer | `Tokenizer::from_bytes` (`tokenizers-0.23.2/src/tokenizer/mod.rs:473`); `with_truncation(Some(TruncationParams { max_length: 256, .. }))` → `Result` (line 661); `with_padding(Some(PaddingParams { strategy: Fixed(256), pad_id: 0, pad_token: "[PAD]", .. }))` (line 687); `encode(text, true)` (line 871); `Encoding::{get_ids, get_attention_mask, get_type_ids}` |
| config | `candle_transformers::models::bert::Config` (serde), `DTYPE = F32` (`bert.rs:15`) |
| weights | `VarBuilder::from_slice_safetensors(&bytes, DTYPE, &Device::Cpu)` (`var_builder.rs:658`); `BertModel::load(vb, &config)` (`bert.rs:466`) with an empty root prefix (the keys carry no `bert.`) |
| forward | `BertModel::forward(&input_ids, &token_type_ids, Some(&attention_mask))` (`bert.rs:495-499`); `token_type_ids` is positional in 0.9.2; the mask is extended by `(1 − mask) × f32::MIN` (`bert.rs:515-525`), so padded positions contribute `exp(f32::MIN − max) = 0` exactly |
| pooling | `hidden.broadcast_mul(&mask_f32.unsqueeze(2)).sum(1) / mask_f32.sum(1)` then `sqr().sum_keepdim(1).sqrt()` and `broadcast_div` — the spike's exact expression (`spike/embed.rs`), which is the expression the Python reference uses (`gen_001_fixtures.py:373-375`) |
| output | `Tensor::to_vec2::<f32>()`; assert `len == 384` |

**Empty text**: encodes to `[CLS] [SEP]` + padding (2 real tokens); the mean is over those two.
The generator includes it and a whitespace-only text (same encoding).
**Over-length text**: truncation keeps the first 255 tokens + `[SEP]` — the tokenizer's default
`TruncationStrategy::LongestFirst` with `direction: Right`; both sides use the defaults.
**Symmetry (FR-007)**: `TextKind` is accepted and ignored; the fingerprint carries `prefix=none`.

## D6. The fingerprint

**Decision** (FR-004): a single line, fixed field order, no whitespace:

```
sentence-transformers/all-MiniLM-L6-v2@1110a243fdf4706b3f48f1d95db1a4f5529b4d41;weights=sha256:53aa5117…128d9db;dim=384;pool=mean-mask;norm=l2;max_tokens=256;dtype=f32;prefix=none;engine=candle-0.9.2
```

Every field is an input whose change would change the vectors: the model identity (repo,
revision, weight hash), the two preprocessing choices, the truncation length, the weight
precision, the (absent) prefixes, and the **inference engine version** — ADR-0001 pins candle,
and an engine bump can move low-order bits, so it is a fingerprint change by design (an index
built by one engine version is not silently searched by another). Not included: thread count
and architecture (D3). The fingerprint is a `&'static str` constant assembled at compile time
from the same constants as the pins, so `PINNED` and the fingerprint cannot disagree.

## D7. Exact search: scores in `f64`, rounded once; ordering via `partial_cmp` then id

**Decision**: for each live row, the score is accumulated in **`f64`** from the `f32` inputs and
rounded to `f32` once at the end. A product of two `f32` is exact in `f64` (24 + 24 < 53 bits) and
384 additions in `f64` carry ~1e-14 of error, so the Rust score and a NumPy `float64` oracle agree
to the final `f32` rounding (≤ 6e-8 near 1) — comfortably inside the spec's 1e-6 and, more
importantly, **exact ties are exact on both sides** (duplicate vectors give identical bits
whichever order the sum runs). A plain `f32` accumulation over 384 terms could drift ~1e-5 and
would make near-ties ambiguous against the oracle. This is a scalar loop; the compiler does not
reorder float reductions without fast-math, so it is deterministic per architecture and reproduces
across reopening and thread counts (the index does no threading). No SIMD kernels (FR-027).

Metric formulas (`Metric` from core): `Cosine` = `dot / (‖q‖·‖d‖)` with `‖d‖` computed at `add`
in `f64`, stored as `f32`; `Dot` = `dot`; `Euclidean` = `−sqrt(Σ(q−d)²)`. All three implemented —
they are three lines apart and the oracle is trivial — so the index is usable by any `Embedder`
that reports a core `Metric`.

**Ordering**: collect `(score: f32, id: u32)` over live rows (restricted to `allowed` when given),
`sort_unstable_by` with `b.score.partial_cmp(&a.score).unwrap_or(Equal).then(a.id.cmp(&b.id))`,
truncate to `k`. Scores are finite by construction (D9), so `partial_cmp` never returns `None`;
`partial_cmp` rather than `total_cmp` so that `−0.0` and `+0.0` tie by id exactly as NumPy orders
them. Sorting 100k pairs is milliseconds; a heap is an optimisation with no measured need.

**Oracle** (FR-018): `reference/gen_004_fixtures.py` scores in `float64` from the same `f32`
values (serialised as the shortest decimal of the `float32`, which parses back to the identical
`f32`), orders by `(−score, id)`, and **refuses to emit** a case where two consecutive distinct
scores differ by less than `1e-5` — so every tie in the goldens is a designed exact tie
(duplicated vectors, including at the `k`-th rank) and every non-tie is unambiguous after `f32`
rounding. Vector sets: `dim = 8` (readable) and `dim = 384` (real), 50–500 vectors, three
metrics, `k ∈ {0, 1, 5, 10, n, n + 5}`, `allowed` ∈ {none, empty, subset, subset with unseen ids},
and a mutation script (add, replace, delete, commit, reopen) with the expected live set and results
after each commit.

## D8. On-disk format: one file per generation, replaced by rename

**Decision**: the index directory holds `index.bin`:

```
magic      8 bytes  b"XTDENSE1"
hdr_len    8 bytes  u64 LE
header     JSON     {"format_version":1,"dim":384,"metric":"cosine","fingerprint":"…","count":n}
ids        n × u32 LE, strictly ascending
norms      n × f32 LE   (‖row‖, used by Cosine; written for every metric)
vectors    n × dim × f32 LE, row i belongs to ids[i]
```

- Open validates magic, `format_version == 1`, that the file length equals
  `16 + hdr_len + n·(8 + 4·dim)`, and that ids are ascending; any failure is `Error::Corrupt`
  naming what was found. A fingerprint check is a separate `open_for(dir, &dyn Embedder)` that
  returns `Error::FingerprintMismatch { index, current }` (FR-015).
- `commit` writes `index.bin.tmp` in full, `sync_all`, then `rename` over `index.bin`. The old
  inode is **never modified or truncated**, which is the `// SAFETY:` invariant a reader's map
  relies on (D1): a handle that mapped the previous generation keeps a valid, unchanged mapping.
  Two handles on one directory therefore behave as in Feature 002 — a stale handle serves its
  snapshot until reopened; concurrent writers are unsupported and documented (spec edge case).
- All decoding is `u32::from_le_bytes` / `f32::from_le_bytes` over `chunks_exact(4)` — no
  transmute, no alignment requirement, no `bytemuck`; the buffered path decodes into `Vec<f32>` at
  open, the `mmap` path decodes per row while scanning. Both paths are the same code over a
  `&[u8]`; the parity test compares results bit-for-bit.
- `create(dir, dim, metric, fingerprint)` writes an empty generation immediately (the 002
  descriptor pattern), so `open` right after `create` works and a crash before the first commit
  leaves a valid empty index.

Sizes: 100k × 384 × 4 = 153.6 MB of vectors + 0.8 MB of ids and norms; FiQA 57,638 rows = 88.5 MB.
Rewriting the whole file per commit is O(n) — at FiQA scale ~0.1 s of I/O; recorded, not budgeted
(FR-023). Incremental generations are a later feature with a measured need.

## D9. Mutation semantics and rejections

**Decision** (FR-014, FR-017, edge cases):

- Pending state is `BTreeMap<DocId, Option<Vec<f32>>>` (`None` = delete). `add` of an existing id
  replaces on commit; `delete` of an unknown id is a no-op; nothing pending is visible to `search`
  or `len`; dropping the handle discards pending. Commit merges committed ids with pending in
  ascending id order into a new generation (D8).
- `add` rejects: wrong width → `Error::DimensionMismatch { expected, actual }`; any non-finite
  component → `Error::Schema("vector for DocId n has a non-finite component at i")`; zero norm
  under `Cosine` → `Error::Schema(…)` (its cosine is undefined). Rejected vectors are never stored.
- `search` rejects: wrong width → `DimensionMismatch`; non-finite or (under `Cosine`) zero-norm
  query → `Error::InvalidQuery(…)`. `k == 0` ⇒ `Ok(vec![])` (002 convention); `k > len` ⇒ all live
  rows; empty `allowed` ⇒ `Ok(vec![])` without scanning; unseen ids in `allowed` are ignored. A
  non-unit query under `Cosine` is fine — the formula divides by its norm.

No core `Error` variant is added; the four used (`DimensionMismatch`, `Schema`, `InvalidQuery`,
`Corrupt`, plus `FingerprintMismatch`, `Model`, `Io`) exist today (`xtriever-core/src/error.rs`).

## D10. The eval harness gains a dense configuration without depending on `xtriever-dense`

**Decision** (FR-019, FR-025; 003 D6 pattern):

- `xtriever-eval` library (still `core + serde + serde_json + sha2`): `run::DenseConfig { name,
  passage: PassageSpec { title_then_text, separator: " ", omit_empty_title }, k }`,
  `run::dense_baseline_v1()`, `run::build_passages(&Dataset, &DenseConfig) -> (Vec<String>,
  IdMap)` (`DocId(i)` = corpus position, exactly as `build`), and `run::execute_dense(&dyn
  Embedder, &dyn VectorIndex, &IdMap, &Dataset, &DenseConfig) -> Run` which embeds each judged
  query with `TextKind::Query` and calls `search(&v, None, k)`. Trait objects only — the library
  never names a dense type.
- `run::EmbeddingCacheKey { config, dataset, embedder_fingerprint, corpus_sha256, documents }`
  serialised as `cache.json` beside `index.bin`; `matches(dir)` is the FR-020 check. The corpus
  hash is the manifest's `corpus.jsonl` SHA-256 the dataset loader already verified.
- The `beir` example (dev-dependency on `xtriever-dense`, as it has one on `xtriever-lexical`)
  gains `--config dense-baseline-v1 --model-dir DIR --cache-dir DIR [--load-path buffered|mmap]`:
  if the cache key matches, `FlatIndex::open_for(dir, &embedder)`; otherwise embed every passage
  with `TextKind::Passage` (progress to stderr every 1,000), `add`, one `commit`, write
  `cache.json`. Wall times go to **stderr**, never into the report, so two runs are byte-identical
  (SC-006); observations are copied into the JSON by hand as in 003.
- `EvalReport` gains one **optional trailing** key `stage { kind: "dense", embedder_fingerprint,
  load_path, thread_count, baseline: "absolute" }` (`skip_serializing_if = None`), and
  `Observations` gains optional `embed_corpus_ms`, `search_ms`, `model_bytes_buffered`,
  `model_bytes_mmapped`. Every committed lexical report deserialises and re-serialises
  byte-identically, so the 003 contract is **extended, not changed**; its file gets a one-line
  note. `report::delta` refuses to pair reports whose `config` differs — the mechanical form of
  FR-021 ("not a delta against the lexical baseline").
- **New public items in core-dependent library code, none in core**: `xtriever-core` is untouched
  (FR-025); the harness change is additive in `run.rs` and `report.rs`; `metrics.rs` and
  `dataset.rs` are not edited (SC-009).

## D11. Baseline recipe: `dense-baseline-v1`

| item | value |
|---|---|
| passage | `title + " " + text` when the title is non-empty, else `text` (FiQA has no titles — 003 D2) |
| truncation | the model's 256 tokens (FR-002); no harness-side cut |
| query | the BEIR query text, `TextKind::Query` (identical to passage for this model, FR-007) |
| `k` | 100 (Recall@100) |
| metric | `Cosine` (unit vectors, so the score is the dot product) |
| threads | `RAYON_NUM_THREADS` as set by the operator, recorded in `stage.thread_count` |
| load path | recorded in `stage.load_path`; numbers are identical by the parity test |
| datasets | SciFact, NFCorpus, FiQA (user decision Q2 = A); FiQA embedded once, cached |
| verification | `gen_003_fixtures.py --verify-run` on every dataset (FR-022) |

No published band applies (the spec's FR-021 makes the dense number absolute): the source 003
used for BM25 (Thakur et al. 2021, arXiv 2104.08663) does not evaluate `all-MiniLM-L6-v2`, and
nothing else is pinned here. The report states the numbers with no verdict attached; if a
comparison is wanted later it must be pinned to a source the way D5 of 003 pinned the BM25 rows.

## D12. Measuring FR-023 (fresh processes, one method per number)

| observation | method |
|---|---|
| index file size | `stat -f %z <cache-dir>/fiqa/index.bin` after the run |
| peak RSS of the evaluating process | `/usr/bin/time -l cargo run --release … run --dataset fiqa` → "maximum resident set size" (the 003 method; includes the corpus and passages held by the harness) |
| model memory per load path | `/usr/bin/time -l cargo run --release -p xtriever-eval --example beir -- model-memory --model-dir … --load-path buffered` and again `--load-path mmap` — a subcommand that loads the model, embeds one text and exits, so each path is measured **from cold in its own process** (the measurement 001 owed); the difference is the transient D1 predicts |
| embed wall time | the example prints `embedded N passages in S s` to stderr (`Instant` is fine in a binary) |
| query wall time | `searched Q queries in S s` likewise |

Extrapolation to 100k chunks (Story 4 scenario 3) is labelled as such in the report with the
assumptions listed (linear in rows; model memory constant).

## D13. Dependencies (all via `cargo add`; versions from the lockfile, never from memory)

`xtriever-dense` gains: `candle-core`, `candle-nn`, `candle-transformers` at **0.9.2 with the
ADR-0001 pin comment copied verbatim** (`cargo add candle-core@0.9.2 …` — not a bare `cargo add`,
which would resolve 0.11 and trip `onig_sys`), `tokenizers 0.23.2` with `default-features = false,
features = ["fancy-regex"]` (the deny.toml `onig_sys` ban), `serde`, `serde_json`, `sha2`,
`thiserror`, and `memmap2` **optional** behind `mmap = ["dep:memmap2"]`. Dev: `proptest`,
`tempfile`, `serde_json`. `default = []`. candle is a **non-optional** dependency: it is pure Rust
(gemm, no C/C++), the crate exists to run it, and Feature 001 already `cargo check`ed it on all
three mobile targets. `deny.toml` unchanged (the `ort-sys` wrapper entry is for a later backend).

`xtriever-eval` gains a **dev**-dependency on `xtriever-dense` for the example. The library graph
stays `core + serde + serde_json + sha2` (quickstart re-runs the 003 `cargo tree` check).

wasm32 stays best-effort: the workspace check already fails at `errno` via tantivy; candle adds no
new class of failure worth tracking separately.

## D14. Python reference environment

`reference/requirements-004.in` pins **exactly** Feature 001's `torch 2.14.0 / transformers
5.17.0 / tokenizers 0.23.2 / safetensors 0.8.0 / huggingface-hub 1.31.0 / numpy 2.5.3` — the
embedding oracle *is* 001's, and the search oracle needs only NumPy. Compiled with hashes to
`requirements-004.txt`; `scripts/setup-reference-venv.sh 004`. `gen_004_fixtures.py` imports
`gen_001`'s tokenization/pooling expressions by copying them (they are ~30 lines; a cross-script
import would couple 001's argument parsing into 004) and pins its own thread environment before
importing torch, as 001 does. It emits `embeddings.json` (texts, token ids/mask, vectors,
tolerances), `search.json` (vector sets, queries, expected results per metric/k/allowed),
`mutations.json`, and `manifest.json` with per-file SHA-256 (the 002/003 `fixtures_valid`
pattern). `--verify-embed` re-embeds a Rust-exported vector file and reports cosine / max-abs
against torch, so the real baseline's corpus embeddings can be spot-checked, not only the goldens.

## Risks

| # | risk | mitigation |
|---|---|---|
| R1 | Thread count changes bits (D3) despite the reading | cross-process test; on failure stop and report (Rule 6); baseline records the count either way |
| R2 | mmap shows no peak benefit from cold (D1) | ADR-0007 condition: measured in fresh processes; if the buffered peak minus the mapped peak is under 10 % of the buffered peak, the **weights** mmap path is deleted (the index mapping stays — its benefit is structural: clean, evictable pages) |
| R3 | FiQA embedding takes hours at one thread (D2) | run once with the operator's thread count recorded; cache keyed by fingerprint + corpus hash; quickstart states the measured time |
| R4 | An accidental exact tie between non-duplicate vectors in the search goldens | generator asserts every tie is between identical rows, else rerolls the seed |
| R5 | `Encoding` for an over-length text differs between `tokenizers` Rust and Python | same crate version on both sides (D14); the golden carries the token ids so a tokenizer disagreement fails the tokenization check, not the embedding check (001 D6) |
| R6 | Report byte-identity broken by timings | timings to stderr only (D10) |
| R7 | Someone opens a 100k index on a 32-bit `usize` target | not a target; the format's `count` is `u64` and open rejects anything that does not fit `usize` with `Corrupt` |
