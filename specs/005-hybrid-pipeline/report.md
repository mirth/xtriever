# Feature 005 Report: The Hybrid Pipeline

**Branch**: `005-hybrid-pipeline` | **Closed**: 2026-09-13 | **Spec**: [spec.md](./spec.md) |
**Plan**: [plan.md](./plan.md)

## Verdict

`xtriever-pipeline` composes the lexical and dense stages behind one API. Documents go in under
external ids and come back out under them; both stages see only `DocId`s. A filter is resolved
once and applied to both stages; reciprocal rank fusion orders the candidates in `f64`, verified
bit-for-bit against a Python oracle on hand-made lists and by order on every real query; a
failing or over-budget dense stage degrades to the lexical list (or errors in strict mode);
every hit explains itself under the core's feature names. **The fused baseline beats the better
single stage on all three datasets** — SC-011 holds 3 of 3, needing 2 — and is now the number the
constitution's regression rule guards.

| | |
|---|---|
| acceptance suite | **37 / 37** offline (`cargo nextest run -p xtriever-pipeline`) · **38 / 38** under `--features mmap` · **1 / 1** model-backed |
| harness | `xtriever-eval` **37 / 37** offline; workspace **188 / 188** |
| fusion goldens vs Python RRF | 10 / 10 cases, scores within 1e-9 (bit-identical: same terms, same order) |
| real-data fusion (`--verify-fusion`) | SciFact 300 / 300, NFCorpus 323 / 323, FiQA 648 / 648 queries in RRF order |
| gate | fmt ✓ · clippy `-D warnings` (default and `mmap`) ✓ · deny ✓ · iOS / iOS-sim / Android ✓ · no stubs ✓ · toolchain ✓ · **zero `unsafe`, zero clock/thread reads** in the pipeline ✓ · eval library graph pure ✓ |
| `xtriever-core`, `xtriever-lexical`, `xtriever-dense`, `deny.toml`, eval `metrics.rs` / `dataset.rs` | **unchanged** (`git diff --stat main` empty; FR-027, SC-009) |
| eval delta | **the first real one** — six `compare` tables below; SC-011 PASS 3 / 3 |

## The baseline — `hybrid-baseline-v1`

Recipe: `lexical-baseline-v1` fields (title 2.0 / text 1.0, `standard_en`) + `dense-baseline-v1`
passages (`title + " " + text`), candidate depth 100 per stage, RRF `k = 60`, `k = 100`, buffered
load path, `RAYON_NUM_THREADS=4`. Every dataset ingested from the Feature 004 embedding cache by
value: **0 documents embedded**.

> **Baseline commit**: `27978b1a3a0c0dd420a109625e2111098e50837e`

| dataset | nDCG@10 | Recall@100 | BEIR-rounded | scored | `--verify-run` | `--verify-fusion` |
|---|---|---|---|---|---|---|
| SciFact | **0.689727** | 0.941667 | 0.68973 / 0.94167 | 300 | PASS | 300 / 300 |
| NFCorpus | **0.345008** | 0.320720 | 0.34501 / 0.32072 | 323 | PASS | 323 / 323 |
| FiQA-2018 | **0.369210** | 0.707111 | 0.36921 / 0.70711 | 648 | PASS | 648 / 648 |

**Reproducibility (SC-007)**: every dataset run twice → byte-identical reports.

### Comparisons against both stage baselines (SC-011)

| dataset | metric | lexical (003) | dense (004) | **hybrid** | vs better stage |
|---|---|---|---|---|---|
| SciFact | nDCG@10 | 0.627044 | 0.645082 | **0.689727** | **+0.044645 (+6.9 %)** |
| SciFact | Recall@100 | 0.887556 | 0.925000 | **0.941667** | +0.016667 |
| NFCorpus | nDCG@10 | 0.311523 | 0.316673 | **0.345008** | **+0.028335 (+8.9 %)** |
| NFCorpus | Recall@100 | 0.247820 | 0.311450 | **0.320720** | +0.009269 |
| FiQA | nDCG@10 | 0.250238 | 0.368671 | **0.369210** | **+0.000538 (+0.15 %)** |
| FiQA | Recall@100 | 0.551775 | 0.706057 | **0.707111** | +0.001054 |

**SC-011 verdict: PASS — fused nDCG@10 is not below (and in fact above) the better single stage
on 3 of 3 datasets.** Against the lexical baseline the gains are +10 %, +10.8 % and +47.5 %.
FiQA is where the lexical stage is weakest (0.250) and the dense stage strongest; RRF adds only
0.15 % there — it cannot invent signal the lexical list does not carry. `beir delta` still
refuses the mixed pair (004 FR-021); `beir compare` is the cross-configuration table without an
ADR line.

## Observations (SC-010) — recorded, not budgeted

Host: Apple M1 Pro, macOS 25.6, release build, `RAYON_NUM_THREADS=4`.

| observation | value | method |
|---|---|---|
| FiQA ingest from the cache (57,638 docs, both stages, one commit) | **5.2 s** | example stderr |
| FiQA hybrid directory | **107,880,448 B** (102.9 MiB): `dense/index.bin` 88,993,414 B + `lexical/` 17.5 MiB + `ids.json` **508,023 B** | `du -sk`, `stat -f %z` |
| FiQA peak RSS, whole `beir run` process | **776,044,544 B** (740 MiB) — corpus 48 MB + built fields + the 004 cache `FlatIndex` (88 MB, read to feed `add_embedded`) + the hybrid's own dense stage (88 MB) + model (~87 MB) | `/usr/bin/time -l` |
| NFCorpus peak RSS | 264,077,312 B | same |
| search, end to end, FiQA | **113 ms / query** (648 in 73.3 s); SciFact 97 ms; NFCorpus 94 ms | example stderr |
| of which query embedding | ~95 ms (measured by skipping the dense stage with a spent budget: 0.3 ms lexical-only vs 95.8 ms end to end) | throwaway probe |
| `Filter::Ids` over half of SciFact (2,592 ids) | lexical-only search **0.3 ms → 2.0 ms**; end to end unchanged (95.7 vs 95.8 ms) | throwaway probe, 3 queries × 5 runs |

The id map is 8.8 bytes per document as JSON; at 100k documents ~0.9 MB, read in milliseconds.
The `Filter::Ids` cost (research D6) is real but two orders of magnitude below query embedding;
the core-trait change it would take to avoid it is not justified by this number.

## Findings

### F-001 — The spec's filter clause was false under rank fusion (spec corrected)

Story 2 scenario 3 and SC-004 said a filtered search "equals the unrestricted result filtered to
that set". Under RRF that is false in general: removing candidates shifts ranks, so the fusion of
two *restricted* lists differs from the restricted fusion of two unrestricted lists — and the
former is the intended contract (FR-012: resolve once, restrict both stages *before* ranking).
Corrected in the spec at implementation time with a note; the test asserts what the contract
actually promises: hits ⊆ set, equality with the fusion of the directly restricted stage
searches, and no stage score changed by the filter.

### F-002 — `--verify-fusion` needs complete stage lists, not the fused top-k's explanations

The first export rebuilt each stage list from the explanations of the *fused* top-100, which
omits candidates that fell outside it, so the Python oracle renumbered ranks and disagreed on
every query. The export now runs a second search with `k = 2 × depth` (the union bound) so every
candidate's rank appears, and exports `[rank, id]` pairs; the oracle scores from the given ranks
and compares tie blocks as sets (it breaks ties by external id, the pipeline by internal id).
Result: 1,271 / 1,271 real queries agree.

### F-003 — `search_lexical` needs a text for the dense stage (contract corrected)

The contract's `search_lexical(query: &LexicalQuery, …)` had nothing for the dense stage to
embed. Added `dense_text: &str`; `search(text)` is the common case that passes the same text to
both.

### F-004 — A test bug in the disjoint fusion case

The first draft of `one_list_only_documents_still_appear` expected all 12 disjoint ids in a
`k = 10` result. The golden was right; the assertion now checks both lists contribute.

### F-005 — Scaffolding the harness at the red checkpoint

Unlike the stage crates, `xtriever-eval` is an existing library, so its new entry points were
scaffolded with `Error::Run("NotImplemented …")` so the workspace compiled red and
`check-no-stubs.sh` (extended to the pipeline crate) could hold the line until Phase 8.

## Measured facts worth keeping

- Query latency in the hybrid pipeline is ~95 % query embedding; the two stage searches and
  fusion together are ~2–3 ms at SciFact scale. The pipeline feature's cost model is the dense
  stage's.
- Peak RSS of the *harness* run doubles the dense index (cache + hybrid copy). A production
  ingest embeds instead of copying and would not.
- wasm32 (best-effort, tracked) unchanged: fails at `getrandom` via candle.

## Success criteria → evidence

| SC | evidence |
|---|---|
| SC-001 fusion goldens exact | `fusion_golden::every_fusion_golden_matches_exactly` (10 cases), `tie_at_k_goes_to_the_lower_id` |
| SC-002 1,000-doc round trip, replace, delete | `ingest::a_thousand_documents_round_trip_replace_and_delete` |
| SC-003 identical before/after reopen | `persist::reopened_index_gives_identical_hits_and_ids` (+ `mapped_open_gives_identical_hits`) |
| SC-004 filtered = fusion of restricted stages (corrected) | `search::filtered_search_is_the_fusion_of_both_stages_restricted_to_the_same_set` |
| SC-005 degrade / strict / lexical error | `degrade::*` (8 tests incl. both check points, `BudgetExhausted`, ignored time limit, item cap) |
| SC-006 explanations exact, ranking unchanged | `explain::*` |
| SC-007 three baselines, verified, reproducible | table above; every dataset twice, `diff` identical |
| SC-008 0 re-embedded | `embedded 0 documents (004 cache)` on every run |
| SC-009 offline suite without the model; untouched crates; mobile targets | 37 offline tests; FR-027 diff empty; three `cargo check`s |
| SC-010 FiQA ingest recorded | observations table |
| SC-011 fused ≥ better stage on ≥ 2 of 3 | **3 of 3**, comparison table |

## Known costs (stated, not claimed small)

- `Filter::Ids` materialises the allowed set as a term-set query (research D6); measured small.
- The harness's hybrid run holds two copies of the dense vectors (cache + index); the number in
  `observations.peak_rss_bytes` is the harness's, not the pipeline's.
- Hits are per chunk with no grouping by source (user decision Q1); a RAG caller groups.

## Review round 1 (GitHub Copilot, 2026-09-13) — 5 comments, all acted on

| # | finding | action |
|---|---|---|
| 1 | The four-count check cannot detect a **same-cardinality** partial commit: a replace (or delete + add) that crashes after the lexical commit leaves every live count unchanged while the stages hold mixed generations — FR-005 violated | `commit` now writes a **`commit.pending` marker** (the new generation number) before the first stage commit and removes it only after the descriptor is written; `open` refuses a directory whose marker exists, before touching either stage. The count check stays as a second line of defence. Tests reproduce the crash's on-disk state (marker + lexical committed through the stage's own handle) for both the count-changing case and the same-cardinality replace; a completed commit leaves no marker. Research D3, data-model and contract amended |
| 2 | `--export-explain` labelled lines with `query_ids[qi]` from all judged ids, but `execute_external` skips dangling judged ids without calling the closure — after the first dangling id every label would be wrong | The runner closure now receives `(query_id, text)`; the export uses the id it was called with. Contract and the runner test updated (each call carries its own id; dangling ids get no call) |
| 3 | `commit_lexical_only_for_test` was a public method whose only effect is an inconsistent index | Removed. The FR-005 tests build the crash state through the filesystem and the lexical stage's own handle (`support::crash_after_lexical_commit`), which is also a more faithful reproduction of a real crash |
| 4 | An empty resolved filter reported `dense_candidates = Some(0)`, i.e. "the dense stage ran and found nothing", although neither stage ran | `None` for both short-circuits (`k == 0`, empty filter); `StageReport::dense_candidates` documented as "`None` = did not run (degraded, or short-circuited)"; test updated |
| 5 | The contract's `--export-explain` schema still said bare id lists; the implementation exchanges `[rank, id]` pairs | Contract updated with the actual schema, the completeness guarantee (`k = 2 × depth` second search) and the tie-block comparison rule |

After the round: `xtriever-pipeline` 39 / 39 offline (+ 40 / 40 under `mmap`), `xtriever-eval`
37 / 37, workspace 190 / 190; the three baselines re-run with the fixed export are identical in
every field but `harness_commit`; `--verify-fusion` 1,271 / 1,271; zero `unsafe`, zero clock or
thread reads in the pipeline.
