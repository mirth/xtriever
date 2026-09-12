# Feature 004 Report: The Dense Stage

**Branch**: `004-dense-stage` | **Closed**: 2026-09-12 | **Spec**: [spec.md](./spec.md) |
**Plan**: [plan.md](./plan.md) | **Constitution**: v1.2.0 (amended by this feature, ADR-0007)

## Verdict

`xtriever-dense` makes both dense core traits true. `MiniLmEmbedder` loads the pinned
`all-MiniLM-L6-v2` from three hash-verified files, asserts its shape from those files, and embeds
within Feature 001's tolerance of torch with **bit-identical** output across batch composition,
batch order, batch size, thread count (1 vs 4, separate processes) and both load paths.
`FlatIndex` stores rows in a versioned `index.bin`, searches exactly against a `float64` oracle
(every designed tie at the `k`-th rank resolved to the lower id), and survives reopening,
stale handles and crashed commits. The dense stage's **absolute** BEIR baseline is recorded on all
three datasets and cross-checked by `pytrec_eval`.

| | |
|---|---|
| acceptance suite | **33 / 33** offline (`cargo nextest run -p xtriever-dense`) · **16 / 16** model-backed (`--run-ignored only`) · **37 / 37** offline + **17 / 17** model-backed under `--features mmap` |
| harness | `xtriever-eval` **32 / 32** offline + 4 / 4 dataset-backed; workspace **142 / 142** |
| embedding goldens vs torch | 13 / 13 cases: cosine ≥ 0.9999, max-abs ≤ 1e-3; tokenization ids exact first |
| search goldens vs NumPy | 4 sets, 3 queries each, every `k` / `allowed` case: ids and order exact, scores within 1e-6; 8 designed ties at rank `k` |
| real corpus vs torch | 50 sampled SciFact rows: worst cosine 1.000000, worst max-abs **2.16e-07** |
| gate | fmt ✓ · clippy `-D warnings` (default and `mmap`) ✓ · deny ✓ · iOS / iOS-sim / Android `cargo check` ✓ (+ `mmap` on iOS) · no stubs ✓ · toolchain ✓ · one `unsafe` block, one item-scoped allow ✓ · eval library graph pure ✓ |
| `xtriever-core`, `xtriever-lexical`, `deny.toml`, eval `metrics.rs` / `dataset.rs` | **unchanged** (`git diff --stat main` empty; FR-025, SC-009) |
| eval delta | **not applicable** — this feature establishes the dense stage's absolute baseline (FR-021); lexical smoke unchanged |

## The baseline — `dense-baseline-v1`

Recipe (research D11): passage = `title + " " + text` (title omitted when empty), the model's own
256-token truncation, `TextKind::Query` for queries (identical to `Passage` for this model), `k =
100`, `Metric::Cosine`, buffered load path, `RAYON_NUM_THREADS=4`. Harness at `40f461b` (this
branch; the merge commit is the baseline commit, as with 002).

> **Baseline commit**: `8d558f28b755131e8c77e7583a0109d6f21056e9`

| dataset | nDCG@10 | Recall@100 | BEIR-rounded | scored | no-relevant | dropped self-ids | `--verify-run` |
|---|---|---|---|---|---|---|---|
| SciFact | **0.645082** | 0.925000 | 0.64508 / 0.925 | 300 | 0 | 0 | PASS |
| NFCorpus | **0.316673** | 0.311450 | 0.31667 / 0.31145 | 323 | 0 | 0 | PASS |
| FiQA-2018 | **0.368671** | 0.706057 | 0.36867 / 0.70606 | 648 | 0 | 0 | PASS |

These are **absolute numbers for a new stage**, not deltas against the lexical baseline (FR-021);
`beir delta` refuses to pair the two configurations, and the regression rule will apply to the
fused pipeline's number once Feature 005 exists. For orientation only — not a verdict — the
lexical baseline was 0.627 / 0.312 / 0.250 nDCG@10; the dense stage is above it on all three,
most on FiQA, where BM25 is weakest.

**Reproducibility (SC-006)**: SciFact run three times — the second and third from the warm cache
(`embedded 0 passages`) — byte-identical reports. The first run used `RAYON_NUM_THREADS=8`; its
numbers were identical to the 4-thread runs in every field except `stage.thread_count`, which is
what research D3 predicted; the committed file is the 4-thread run so all three carry one count.

**Cache (FR-020)**: `target/xt-dense-cache/<dataset>/{index.bin, cache.json}` keyed by
configuration, dataset, embedder fingerprint, `corpus.jsonl` SHA-256 and document count.

## Observations (FR-023, Story 4) — recorded, not budgeted

Host: Apple M1 Pro (10 cores), macOS 25.6, release build, `RAYON_NUM_THREADS=4`.

| observation | value | method |
|---|---|---|
| FiQA `index.bin` | **88,993,414 bytes** (84.9 MiB) for 57,638 rows = 1,544 B/row (384 × 4 + 8) | `stat -f %z` |
| FiQA peak RSS, whole `beir run` process | **419,217,408 bytes** (399.8 MiB) | `/usr/bin/time -l` — includes the 48 MB corpus, the passage strings, the model (~87 MB), candle's workspace and the in-memory index |
| NFCorpus peak RSS | 249,626,624 bytes | same |
| embed FiQA corpus | **5,433.2 s** (90.6 min) = **94 ms / passage** | example's stderr timer |
| embed SciFact corpus | 664.1 s at 8 threads = 128 ms / passage (8 threads were *slower* than 4: `gemm` at M = 256 scales to ~2 cores) | same |
| embed NFCorpus corpus | 339.3 s = 93 ms / passage | same |
| search, FiQA, 648 queries | 73.8 s — **~114 ms per query, of which the flat scan over 57,638 rows is a few ms**; the rest is embedding the query | same |
| model memory, buffered, fresh process | **226,328,576 B** median (222.8 / 226.3 / 226.7 MB) | `/usr/bin/time -l beir model-memory --load-path buffered`, ×3 |
| model memory, mmap, fresh process | **197,394,432 B** median (193.1 / 197.4 / 197.5 MB) | same, `--load-path mmap` |

**ADR-0007 condition 5**: mapped saves **~29 MB = 13 %** of the buffered peak — above the 10 %
deletion threshold, so `LoadPath::Mmap` **stays**. The saving is a third of the 90.9 MB file
buffer it avoids, not all of it: macOS counts touched file-backed pages in RSS and candle touches
every page while copying tensors onto the heap (D1). Steady state is identical between the paths.
The figure is a host RSS number; on iOS `phys_footprint` may treat clean mapped pages differently,
and that remains unmeasured (not claimed).

**Extrapolation to the constitution's 100k-chunk configuration — labelled as such.** Assumptions:
index bytes and embedding time scale linearly in rows; model memory is constant; queries are the
same length as FiQA's.

| quantity | measured at 57,638 (FiQA) | extrapolated to 100,000 |
|---|---|---|
| dense index on disk | 84.9 MiB | **~147 MiB** (100,000 × 1,544 B) |
| lexical index on disk (003) | 17.5 MiB | ~30 MiB |
| model, resident, steady state | ~87 MiB of tensors (either path) | ~87 MiB |
| corpus embedding time at 4 threads | 90.6 min | **~2.6 h** once, then cached |
| flat exact search per query (excluding query embedding) | a few ms | ~10 ms |

Two real points now sit on the curve Feature 002 deferred: lexical 17.5 MiB + dense 84.9 MiB on
disk at 57.6k documents. Whether a mapped dense index of ~147 MiB plus ~87 MiB of model plus the
lexical index fits a 300 MB `phys_footprint` on a phone is exactly the question the on-device
feature has to measure; nothing here claims the answer.

## Findings

### F-001 — Feature 001's "39.5× from mmap" was an ordering artifact (research D1)

candle 0.9.2 copies every safetensors tensor onto the heap whichever loader is used
(`convert_slice` → `Tensor::from_slice`). Mapping removes only the transient file buffer during
load. Measured from cold: 13 %, not 39.5×. A dated correction now sits in the 001 report under its
headline. The mmap path's structural benefit is the **vector index**, whose rows are read from the
map without a copy — that is what ADR-0007 admits and what the on-device feature can lean on.

### F-002 — Batching is not bit-drift-free by construction (research D2)

candle folds a broadcast batch into the matmul's `M`, so the same text in different batches goes
through `gemm` calls of different shape. The embedder therefore runs one text per forward pass at
a fixed 256 tokens. SC-002 (three batch arrangements bit-identical) and the thread-count test
both pass; the price is throughput — ~94 ms per passage, and no gain beyond ~2 threads. A batched
mode would be a later, *measured* change gated by SC-002 staying green.

### F-003 — `serde_json` did not round-trip the 003 baselines (latent smoke risk, fixed)

`committed_lexical_reports_round_trip_byte_for_byte` failed on `0.24781998827361362` re-serialised
as `0.2478199882736136` — a different `f64`. `serde_json` parses floats with best-effort precision
unless its `float_roundtrip` feature is on, so the smoke compared a *1-ulp-off* baseline. Enabled
the feature (`cargo add … --features float_roundtrip`); every committed report now round-trips.
The FiQA lexical baseline also carried a hand-typed `—` escape; normalised to the character
(one byte-level change, same JSON value).

### F-004 — A BLAS oracle cannot certify an exact tie (generator design)

The first search oracle used NumPy's `@` and found two *identical* rows scoring 1.39e-17 apart —
gemv kernels accumulate different rows in different orders. The oracle now uses `math.fsum` over
exact `float64` products (correctly rounded, order-independent), which is also strictly more
accurate than the Rust side's sequential `f64` sum. `TIE_MARGIN` was set to 1e-6 (> 8 f32 ulps at
|score| ≤ 1) after 1e-5 rejected 50 seeds on the 384-dimensional set; ambiguity is now checked
per case over the top `k + 1` only, which is the only region that can affect a comparison.

### F-005 — Thread scaling stops at ~2 cores

`RAYON_NUM_THREADS=8` was slower than 4 (128 vs 94 ms per passage). Bit-identity across thread
counts holds (D3 confirmed), so the count is purely a throughput knob; 4 is the recorded default.

## Measured facts worth keeping

- Forward pass, fixed 256 tokens, release, M1 Pro: ~170 ms (1 thread), ~110 ms (4 threads,
  golden run), 93–94 ms (4 threads, corpus runs), 128 ms (8 threads).
- `verify_files` on the 90.9 MB weights: ~80 ms release, ~1.4 s debug (streamed SHA-256).
- FiQA has 55 query-id/doc-id collisions (003 D2); none was retrieved for its own query by the
  dense stage either (`dropped_identical = 0` on all three).
- wasm32 (best-effort, tracked) now fails earlier than before — at `getrandom` via candle's `rand`
  instead of at `errno` via tantivy. Same status, different first error.

## Success criteria → evidence

| SC | evidence |
|---|---|
| SC-001 every golden within tolerance | `embed_golden::every_golden_embeds_within_tolerance` (13 cases incl. `long_over_256`, `empty`, `whitespace_only`, `oov_unicode`) |
| SC-002 bit-identical across arrangements | `embed_determinism::three_batch_arrangements_are_bit_identical`, `…across_thread_counts` (1 vs 4, cross-process) |
| SC-003 every search golden exact | `index_golden::every_search_golden_matches_exactly`, `…designed_ties_at_the_k_th_rank_go_to_the_lower_id` (8 ties) |
| SC-004 reopen bit-identical | `index_persist::reopened_index_returns_bit_identical_results` |
| SC-005 three named errors | `index_errors::{fingerprint_mismatch_names_both_fingerprints, a_future_format_version_is_rejected_naming_both_versions, dimension_mismatch_names_both_dimensions_on_add_and_search}` |
| SC-006 three baselines, warm re-run byte-identical, 0 embedded | three files; SciFact runs 2 and 3 `diff`-identical, `embedded 0 passages` |
| SC-007 `--verify-run` within 1e-6 | PASS on all three |
| SC-008 observations with method; model memory measured | FiQA `observations` block; medians of three fresh processes per path |
| SC-009 offline suite without the model; untouched crates; mobile targets | 33 offline tests need no model; FR-025 diff empty; iOS / iOS-sim / Android checks |
| SC-010 two loads, same fingerprint, same bits | `fingerprint::two_loads_same_fingerprint_same_bits`; `load_paths::buffered_and_mapped_loaders_agree_bit_for_bit` |

## Known costs (stated, not claimed small)

- Per-text inference at fixed length: every passage costs a full 256-token pass; FiQA is 90 min.
- `commit` rewrites the whole `index.bin` (85 MiB at FiQA scale); fine for one-shot builds, a
  measured reason is needed before incremental generations.
- Query latency is dominated by query embedding (~114 ms), not the scan; the pipeline feature
  should count that as the dense stage's cost.
- `search.json` is 2.2 MB of committed goldens (120 × 384 floats plus the small sets).

## Review round 1 (GitHub Copilot, 2026-09-12) — 10 comments, all acted on

| # | finding | action |
|---|---|---|
| 1 | The `// SAFETY:` comment claimed the no-modification invariant was "a property of this crate's own code", but another process can still truncate or rewrite a mapped file; the safe `open_mapped*` / `load(Mmap)` APIs cannot enforce it | Reworded honestly: the crate guarantees it never writes a mapped file in place; the external-writer precondition is stated on `LoadPath::Mmap`, `open_mapped`, `open_mapped_for`, the crate docs, the contract and ADR-0007 condition 2 — the same contract tantivy's `MmapDirectory` offers behind a safe API, and the reason the path is opt-in. Making the constructors `unsafe fn` would push `unsafe` into every consumer, which Principle VII forbids; **no code can enforce this invariant, so it is documented rather than pretended** |
| 2 | `decode` computed section offsets with unchecked `+`/`×` from the untrusted header — overflow instead of `Corrupt` | `checked_mul` / `checked_add` for every offset; test `absurd_count_or_dim_is_corrupt_not_a_panic` (`count = u64::MAX`, `dim = usize::MAX`, `hdr_len = u64::MAX`) |
| 3 | A finite vector with norm > `f32::MAX` (e.g. `[f32::MAX, f32::MAX]`) stored `+inf` and scored its own cosine as 0 | `add` rejects a norm that is not a finite `f32` (`Schema`); test `a_norm_that_does_not_fit_f32_is_rejected_on_add` (and a 1e19-norm vector still scores 1.0 against itself) |
| 4 | `commit` took `pending` out of `self` before any fallible I/O, so a failed commit silently dropped every staged change | `pending` is borrowed during the merge and cleared only after the new generation is written **and** reopened; test `a_failed_commit_keeps_the_staged_changes_for_a_retry` (directory replaced by a file → `Io`; retry commits exactly the staged changes) |
| 5 | `16 + hdr_len` could overflow on a corrupt header | `16usize.checked_add(hdr_len)`; covered by the test in #2 |
| 6 | `search` validated the query before the `k == 0` / empty-allowed shortcut | **Kept, made explicit**: a malformed query is a caller error whatever `k` is, and hiding it behind `k == 0` would let it surface later; the contract and data-model now state the order, and `a_malformed_query_is_an_error_even_when_no_work_would_be_done` pins it |
| 7 | `smoke` ran the metric-decrease loop before the configuration check, so a mixed-configuration pair that also regressed was reported as an ordinary regression | `smoke` calls `delta` (which validates configurations) **first**; test `smoke_reports_a_mixed_configuration_before_any_metric_comparison` |
| 8 | data-model's in-memory `Generation` (typed `ids`/`norms`/`Rows` columns) no longer matched the implementation (`Bytes` + `Layout`, decoded on access) | data-model corrected to the actual representation |
| 9 | data-model still said `tie_margin: 1e-5`; the fixture and F-004 say 1e-6 | corrected (and research D7 now records the change with its reason) |
| 10 | quickstart's wasm32 expectation said `errno`; the run fails first at `getrandom` via candle now | corrected |

After the round: `xtriever-dense` 36 / 36 offline (+ 41 / 41 under `mmap`), `xtriever-eval`
38 / 38; clippy clean on both feature sets; the one `unsafe {}` and its single item-scoped allow
unchanged.
