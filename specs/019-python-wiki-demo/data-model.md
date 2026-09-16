# Data Model: The Python Wikipedia Demo

Everything the demo holds is derived from the engine's values (`xtriever.Hit`,
`SearchResponse`, `StageReport`, `IndexInfo`) or from files 008 defines. The demo adds no
retrieval state.

## Inputs (research D11)

| Entity | Fields | Validation |
|---|---|---|
| **Paths** | `repo_root`, `artefact`, `index_dir = artefact/index`, `embedder`, `reranker`, `snapshot`, `manifest`, `expected`, `queries` | each resolved from flag → environment → default; every command checks the inputs it needs *before* loading anything and reports the first missing one with its producer (FR-003) |
| **Manifest** | `edition`, `snapshot_date`, `source`, `licence{name,url}`, `parquet{url,bytes,sha256,rows}`, `jsonl{file,bytes,sha256,lines,…}`, `exclusions[]` | read from `reference/datasets/wiki-manifest.json`; the JSONL's bytes and sha256 must match before a line is read (FR-010) |
| **Rule** | `title_suffix{value}` \| `lead_contains{value, within_chars}` | from the manifest's `exclusions`, applied in order, first match wins (D6) |
| **Article** | `id`, `url`, `title`, `text` | one JSONL line; `url` must equal the derived URL (counted as `url_mismatches`, as the Rust build counts it — a mismatch is a build error there and here) |

## Search (US1, US2)

| Entity | Fields |
|---|---|
| **SearchRequest** | `query`, `k` (default 10), `depth` ∈ {0, 5, 10, 20} (default 10), `budget_ms` (None), `strict` (False), `mode` ∈ {interpolate, replace} (interpolate), `explain` (flag), `snippet` (None) |
| **StageRun** | `label` ("fused" \| "re-ranked"), `options: SearchOptions`, `response: SearchResponse`, `wall_ms` (perf_counter around the call), `peak_bytes` after the call |
| **DisplayedHit** | `rank` (1-based), `external_id`, `title` (title line, or the external id when the text has no blank line), `passage`, `url` (None without a title line), `parent`, `ordinal` (0-based; printed +1), `score`, `rerank_score`, `mark` (re-ranked list only), `features` (eight `(name, value | None)` pairs when explained) |
| **Mark** | `new` \| `same` \| `up(n)` \| `down(n)`; plus the `dropped` list of fused hits absent from the re-ranked list — `marks(fused_ids, reranked_ids)` is a pure function (D3) |
| **Features** | in order: `bm25.score`, `bm25.rank`, `dense.score`, `dense.rank`, `fused.score`, `rerank.score`, `rerank.rank`, `rerank.combined` — from `HitExplain`; `None` prints "not seen by this stage" |
| **ReportView** | from `StageReport`: `lexical_candidates`, `dense_candidates` (None → "skipped"), `degraded{stage, reason}`, `rerank{candidates, scored, skipped}`, `time_limit_ignored`, `elapsed_ms`; plus the wall times and the peak resident size |

Invariants: the fused run is always depth 0; the re-ranked run uses the request's depth
and mode; with depth 0 there is one run and no marks. Every printed number is a field of
the engine's response or a wall clock around one call.

## Build (US3)

| Entity | Fields |
|---|---|
| **BuildRequest** | `out` (must not exist), `limit` (None or ≥ 1), `snapshot`, `manifest`, `embedder`, `reranker` (attached so `about` can name it; not part of the identity), `load_path` (mmap) |
| **Passage** | from the chunker: `text`, `byte_range (start, end)`, `cost` — the 008 contract's output (D5) |
| **Document** | `external_id = f"{article.id}#{ordinal}"`, `fields = {"title": TEXT(title), "text": TEXT(f"{title}\n\n{passage.text}")}`, `chunk = ChunkInfo(parent=article.id, ordinal, byte_start, byte_end)` (D7) |
| **Counts** | `articles` (read), `excluded{rule_name: n}` (the Rust names), `selected`, `passages`, `passages_over_window` (always 0: the chunker's bound; verified on the fixture articles, not re-tokenised per passage at build — see below), `url_mismatches` |
| **Phases (ms)** | `fetch_verify`, `read_exclude`, `chunk`, `embed_ingest` (the demo cannot separate embedding from staging: both happen inside `add`), `commit`, `merge`, `total` |
| **CorpusIdentity** (`corpus.json`) | `schema_version: 1`, `corpus_identity` (D9), `snapshot{edition, snapshot_date, parquet_sha256, jsonl_sha256}`, `exclusions[]`, `chunker{version: 1, budget, cost}`, `embedder_fingerprint`, `partial` (the limit, when set), `counts` |
| **Attribution** (`ATTRIBUTION.txt`) | the four lines of 008's `attribution()` with the identity and the RFC 3339 build time |
| **BuildRecord** (`wiki-build.json`) | `schema_version: 1`, `feature: "019-python-wiki-demo"`, `recorded_at`, `corpus_identity`, `host{os, arch, threads}`, `models{embedder, reranker_for_demo}`, `counts`, `phases_ms`, `artefact_bytes{…, total}`, `partial` |

State: `<out>.partial/` during the build (a stale one removed at start) → renamed to
`<out>/` after `index/corpus.json`, `ATTRIBUTION.txt` and `wiki-build.json` are written.
`<out>` existing at start → refused. A passage-count / window check as the Rust `verify`
pass does is not repeated (that pass re-tokenises every stored passage with the embedder,
which the package does not expose); the chunker's bound is the fixture-tested guarantee.

## About (US4)

| Entity | Source |
|---|---|
| corpus edition, snapshot date, identity, `partial`, counts | `index/corpus.json` |
| documents, format version, embedder fingerprint, re-ranker id, candidate depth, rrf k, re-rank depth (engine), re-rank mode, embedder / re-ranker load ms | `IndexHandle.info()` |
| the demo's default depth (10) beside the engine's, labelled | the demo's constant |
| open ms | wall clock around `IndexHandle.open` |
| attribution text, licence URL | `ATTRIBUTION.txt` verbatim; the manifest's `licence.url` |

## Measure (US5)

| Entity | Fields |
|---|---|
| **MeasureRequest** | `artefact`, `expected` (host goldens) **or** `against` (a second artefact), `queries`, `depths = [0, 5, 10, 20]`, `k = 10`, `out` (the record path; default `specs/019-python-wiki-demo/runs/<machine>-<UTC stamp>-mmap-threads<n|default>.json`) |
| **Truth** | per query id and depth: hits `[{external_id, score_bits, bm25_score_bits, dense_score_bits, rerank_score_bits, rerank_rank, rerank_combined_bits}]` — from `expected.json`, or minted live from the `--against` artefact's responses in the same shape |
| **Comparison** (per query) | `lexical_bit_identical`, `fused_order_identical` (depth 0), `dense_max_abs_diff`, `rerank_max_abs_diff`, `all_bits_identical` (count of hits whose every bit field matched), `incomplete[]` (the device test's messages) |
| **MeasurementRecord** | `schemaVersion: 1`, `feature`, `corpus` ("wikipedia" \| "wikipedia-slice"), `machine` (hardware model, e.g. `MacBookPro18,3`; never a hostname), `os`, `python`, `xtrieverVersion`, `build{loadPath, effectiveThreads, threadSource}`, `index{bytes, documents, formatVersion, embedderFingerprint, rerankerModelId, corpusIdentity, partial}`, `openMs`, `embedderLoadMs`, `rerankerLoadMs`, `warmupMs`, `queries[{id, depth, elapsedMs, engineMs, hits, peakBytesAfter}]`, `perDepthMedianMs{…}`, `perDepthMaxMs{…}`, `latency{medianFusedMs, maxFusedMs, medianRerankedMs, maxRerankedMs, medianRerankedAtEngineDefaultMs, medianTotalMs}` (total = fused + re-ranked at depth 10, per query), `footprint{peakBytes, peakMethod: "ru_maxrss", ceilingBytes: 600000000, underCeiling}`, `parity{queriesCompared, lexicalBitIdentical, fusedOrderIdentical, denseMaxAbsDiff, rerankMaxAbsDiff, allBitsIdentical, toleranceAbs: 0.001, verdict}`, `against{artefact, corpusIdentity, counts, identityEqual, countsEqual}` (slice mode only), `notes[]`, `recordedAt` |

Verdict rule (the device's, D13): `PASS` iff every query compared, no `incomplete`
entry, every query lexical-identical and fused-order-identical, and both max diffs ≤ 1e-3.
The verdict is printed and the process exits 1 on `FAIL` after writing the record.
