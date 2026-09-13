# Feature 006 Report: The Re-rank Stage

**Branch**: `006-rerank-stage` | **Closed**: 2026-09-13 | **Spec**: [spec.md](./spec.md) |
**Plan**: [plan.md](./plan.md)

## Verdict

`xtriever-rerank` implements the core's `Reranker` with the pinned
`cross-encoder/ms-marco-MiniLM-L-6-v2` on candle 0.9.2 — the encoder is candle-transformers'
`BertModel`, the classification head (CLS → pooler → tanh → linear) is composed from two
`candle_nn::Linear` layers because candle 0.9.2 ships none. Every golden pair scores within
**7.5e-6** of the Hugging Face reference (tolerance 1e-3) with every per-query order exact and
tokenization parity on all 37 pairs; scores are bit-identical across call composition, passage
order, `RAYON_NUM_THREADS` 1 vs 4 (separate processes) and the buffered vs mapped load paths.
The budget loop scores in input order and returns "not scored" past the item or time limit.
The pipeline now stores every document's passage text (`passages.bin`, format version 2,
ADR-0008), re-scores the first 20 fused candidates under the remaining budget, orders scored
hits first and every unscored hit after in fused order, degrades per stage, and explains
`rerank.score` / `rerank.rank`. **The re-ranked pipeline beats the guarded fused baseline on
all three datasets** — SC-008 holds 3 of 3, needing 2 — with Recall@100 unchanged to the digit.
**The cost is the headline finding**: 116–163 ms per pair on candle at 4 threads, 2.3–3.3 s per
query at depth 20 — far above the plan's 50–100 ms estimate (F-001).

| | |
|---|---|
| acceptance suite | `xtriever-rerank` **9 / 9** offline · **14 / 14** model-backed · **15 / 15** model-backed under `--features mmap`; `xtriever-pipeline` **60 / 60** offline · **61 / 61** under `mmap` · **2 / 2** model-backed (both models) |
| harness | `xtriever-eval` **43 / 43** offline; workspace **226 / 226** |
| goldens vs the HF reference | 37 pairs: tokenization parity 37 / 37; max abs diff **7.540e-6** (tolerance 1e-3); order 10 / 10 queries exact, incl. over-length, empty-passage, empty-query, both-empty, near-tie (gap 0.092) |
| ordering rule vs the Python oracle | 10 / 10 cases exact; 500 property cases; `--verify-rerank` SciFact 300 / 300, NFCorpus 323 / 323, FiQA 648 / 648 real queries |
| gate | fmt ✓ · clippy `-D warnings` (workspace, pipeline `mmap`, rerank `mmap`) ✓ · deny ✓ · iOS / iOS-sim / Android ✓ · wasm32 best-effort fails at `getrandom` via candle (unchanged) · no stubs ✓ · toolchain ✓ · **exactly one `unsafe` in `xtriever-rerank` (`bytes.rs`, behind `mmap`)** ✓ · zero `unsafe`/clock in the pipeline ✓ · eval library graph pure ✓ · pipeline graph free of `xtriever-rerank` ✓ |
| `xtriever-core`, `xtriever-lexical`, `xtriever-dense`, `deny.toml`, eval `metrics.rs` / `dataset.rs` | **unchanged** (`git diff --stat main` empty; FR-022, SC-009) |
| governance | constitution **v1.3.0** (Principle VII names `xtriever-rerank`; ADR-0009) · pipeline format **v2** (ADR-0008) — both decided by the owner on 2026-09-13 |

## The baseline — `hybrid-rerank-v1`

Recipe: `hybrid-baseline-v1` (lexical `title 2.0 / text 1.0`, dense `title + " " + text`,
candidate depth 100 per stage, RRF `k = 60`, `k = 100`) plus the cross-encoder at **re-rank
depth 20**; both models loaded buffered, `RAYON_NUM_THREADS=4`. Every dataset ingested from the
Feature 004 embedding cache by value: **0 documents embedded**; only queries and query–passage
pairs were scored.

> **Baseline commit**: _(fill in after merge — the commit the three baseline files land in)_

| dataset | nDCG@10 | Recall@100 | BEIR-rounded | scored | `--verify-run` | `--verify-rerank` |
|---|---|---|---|---|---|---|
| SciFact | **0.703862** | 0.941667 | 0.70386 / 0.94167 | 300 | PASS | 300 / 300 |
| NFCorpus | **0.360287** | 0.320720 | 0.36029 / 0.32072 | 323 | PASS | 323 / 323 |
| FiQA-2018 | **0.374214** | 0.707111 | 0.37421 / 0.70711 | 648 | PASS | 648 / 648 |

**Reproducibility (SC-007)**: SciFact run twice (once with a binary that re-ranked the
explain-only second search too, once with the fixed one, F-002) → byte-identical reports.

### The delta against the guarded baseline (SC-008)

`beir compare hybrid-baseline-v1.$d.json hybrid-rerank-v1.$d.json`:

| dataset | metric | `hybrid-baseline-v1` (005) | **`hybrid-rerank-v1`** | abs | rel |
|---|---|---|---|---|---|
| SciFact | nDCG@10 | 0.689727 | **0.703862** | **+0.014135** | **+2.05 %** |
| SciFact | Recall@100 | 0.941667 | 0.941667 | +0.000000 | +0.00 % |
| NFCorpus | nDCG@10 | 0.345008 | **0.360287** | **+0.015279** | **+4.43 %** |
| NFCorpus | Recall@100 | 0.320720 | 0.320720 | +0.000000 | +0.00 % |
| FiQA | nDCG@10 | 0.369210 | **0.374214** | **+0.005005** | **+1.36 %** |
| FiQA | Recall@100 | 0.707111 | 0.707111 | +0.000000 | +0.00 % |

**SC-008 verdict: PASS — re-ranked nDCG@10 is above `hybrid-baseline-v1` on 3 of 3 datasets.**
Recall@100 is byte-equal on every dataset, as the spec's assumption predicts (re-ordering the
first 20 of 100 candidates cannot change what the top 100 contains). Against the lexical
baseline (003) the pipeline now stands at +12.3 %, +15.7 % and +49.5 %. `beir delta` still
refuses the mixed pair (exit 1); `compare` is the cross-configuration table. From here on
`hybrid-rerank-v1` is the guarded pipeline number.

## Observations (SC-010) — recorded, not budgeted

All on Apple M1 Pro, macOS 25.6, `RAYON_NUM_THREADS=4`, release build, buffered load paths.

| what | value | method |
|---|---|---|
| per-pair re-rank time | SciFact **155.6 ms**, NFCorpus **163.0 ms**, FiQA **116.0 ms** | `TimedReranker` totals ÷ pairs (`re-ranked N pairs in T s` on stderr); the explain-only second search runs with depth 0 and is not counted |
| per-query re-rank time (depth 20) | SciFact 3,112.9 ms, NFCorpus 3,259.8 ms, FiQA 2,320.4 ms | totals ÷ judged queries |
| per-query end to end | 3,213.6 / 3,369.2 / 2,471.6 ms | `searched N queries in T s`; query embedding is ~95 ms of it, the stage searches and fusion ~5 ms — the re-ranker is 94 % |
| pairs scored | 6,000 / 6,460 / 12,960 | every query × depth 20 (no budget) |
| whole-run wall time | 994 s / 1,124 s / 1,700 s | `/usr/bin/time -l` real |
| FiQA peak RSS, whole `beir run` process | **1,047,330,816 B** (999 MiB) — the 005 run was 776 MB; the cross-encoder adds ~90 MB of tensors plus the store's write buffers | `/usr/bin/time -l` |
| FiQA index directory | **152,621,056 B** (was 107,880,448 B in 005) | `du -sk` × 1024 |
| FiQA `passages.bin` | **44,737,642 B** — 41 % of the directory; the 48 MB `corpus.jsonl` minus JSON overhead | `stat -f %z` |
| cross-encoder fresh-process load, buffered | **251,772,928 B** median (251,019,264 / 251,772,928 / 253,755,392); load 104–112 ms | `beir model-memory --model rerank --load-path buffered` × 3 |
| cross-encoder fresh-process load, mapped | **245,022,720 B** median (244,973,568 / 245,022,720 / 244,989,952); load 98–113 ms | `--load-path mmap` × 3 |
| mapped saving | 6.75 MB, **2.7 %** of the buffered peak (004 measured 13 % for the embedder) | ADR-0009 condition 5 — recorded; no deletion clause (the decision was symmetry) |

The FiQA report carries these as `observations` with the method string.

## Findings

### F-001 — candle scores a pair in 116–163 ms, not 50–100 ms (the cost of this stage)

The plan's estimate (research D9) came from the torch reference (7 ms for a 34-token pair, 53 ms
at 512 tokens, one thread) scaled by 004's candle experience. Measured: **156 ms per pair on
SciFact**, 163 on NFCorpus, 116 on FiQA (shorter passages), at 4 threads. BEIR abstracts sit near
the 512-token cap, attention is quadratic in length, and candle's thread scaling was already
poor at 004 (F-005). At depth 20 that is 2.3–3.3 s per query — an order of magnitude above the
005 pipeline's 113 ms. Nothing in this feature is tuned around it (Rule 6): the number is
recorded, and it is the input to the on-device budget of the next feature — depth 20 on a phone
means seconds, so the budget mechanism (`Budget::max_time`, partial results kept) is not a
nicety but the normal path. A per-length cost curve and candle's matmul path at batch 1 are the
first things to measure next; batching is excluded by the spec (FR-023) and by the determinism
argument (research D5).

### F-002 — The explain-only second search must not re-rank

`--export-explain` runs a second search per query at `k = 2 × candidate_depth` to expose the
complete stage lists (005 F-002). With a re-ranker attached that search re-ranked too, doubling
the stage's cost and — worse — doubling `rerank_ms` / `rerank_pairs`. The first SciFact run
(12,000 pairs, 1,940 s) showed it; the second search now passes `rerank_depth: Some(0)`. The
baseline file is unaffected (timings never enter a report): the re-run was byte-identical.

### F-003 — The mapped load saves 2.7 % here, not 13 %

ADR-0007 condition 5 measured a 29 MB (13 %) saving for the embedder's weights. The same
measurement for the cross-encoder — same file size, same loader — gives 6.75 MB (2.7 %). The
difference is in how much of the transient file buffer is resident at the peak, which depends on
allocator and page-cache state the measurement does not control; the 004 number should be read
as an upper bound. ADR-0009 chose the mapped path for symmetry with the dense stage rather than
for this number, and applies no deletion clause; the finding is recorded so the on-device feature
does not budget on 13 %.

### F-004 — The reference drops an empty passage to single-sequence encoding

`transformers`' fast tokenizer builds `[(text, text_pair)] if text_pair else [text]`, so an
empty passage string is encoded as the query alone — `[CLS] q [SEP]`, no second `[SEP]` — while
an empty *query* keeps the pair template. The Rust side mirrors the call literally
(`encode(query, true)` when the passage is empty), the golden includes the case, and the
tokenization-parity test pins it (research D4). Without the parity test this would have been a
1e-3 tolerance failure with no explanation.

### F-005 — The format bump broke one 005 test's expected text only

`persist.rs` proved a future descriptor version is refused by writing `"format_version": 2`;
with version 2 now current the replacement became a no-op. The expected text moved to 7; the
check itself is unchanged (T042's rule).

## Measured facts worth keeping

- **Bit-identity holds per pair at variable length**: candle 0.9.2's per-shape determinism
  (004) extends to every sequence length in the golden set and to the two thread counts.
- **Debug-build cost**: a 512-token pair takes ~10 s unoptimized; the model-backed suites need
  ~3.5 min for the rerank crate and ~40 s for the pipeline round trip. Run them in release when
  iterating.
- **The five-count check** (descriptor, id map, lexical, dense, and the store's slot count
  against the id map's length) catches a torn `passages.bin` and an off-by-one header count;
  the `commit.pending` marker is still refused first.
- **`HybridHit.text` costs one file read per returned hit**: 100 reads per query at `k = 100`,
  invisible next to the re-ranker (the whole non-rerank part of a query is ~150 ms including
  embedding).

## Success criteria → evidence

| SC | evidence |
|---|---|
| SC-001 | `score_golden`: 37 / 37 within 1e-3 (worst 7.5e-6), 10 / 10 orders exact, all five named cases |
| SC-002 | `score_determinism`: 0 differing bits over three arrangements and RAYON 1 vs 4 (child processes); `load_paths`: buffered = mapped to the bit |
| SC-003 | `budget.rs` (stub): limits {0, 1, 5, 10, 20} exact; zero time limit ⇒ none; `budget_time` (model): 1 of 40 max-length pairs in 100 ms (debug), 3 of 5 under an item limit |
| SC-004 | `rerank_golden` 10 / 10, `rerank_prop` 500 cases, `rerank.rs`: depth 0 / no re-ranker ⇒ byte-equal 005 responses |
| SC-005 | `degrade.rs`: failing ⇒ fused order + `skipped` (default) / `Error::Model` (strict); wrong length and NaN ⇒ `Error::Model` in both modes |
| SC-006 | `explain.rs`: `rerank.score` / `rerank.rank` exactly where scored, seven names, hits unchanged without explain |
| SC-007 | three baselines, `--verify-run` PASS within 1e-6, SciFact byte-identical on re-run |
| SC-008 | **PASS 3 / 3** (+2.05 %, +4.43 %, +1.36 %) |
| SC-009 | offline suites need no model; FR-022 diff empty; three mobile targets check; default features carry no C/C++ |
| SC-010 | the observations table above, with method |

## Known costs (stated, not claimed small)

- **Per query**: +2.3–3.3 s at depth 20 on this host — the stage is the pipeline's cost now.
- **Disk**: the corpus text once more (FiQA +44.7 MB, +41 %).
- **Memory**: a second ~90 MB model resident; the FiQA harness process peaks at 1.0 GB (not the
  index's footprint — see the method).
- **Governance**: two `unsafe` blocks in the workspace now (one per model-loading crate),
  identical, each behind a non-default feature.

## Review round 1

_(GitHub Copilot comments, when they arrive, are recorded here with the action taken.)_
