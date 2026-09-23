# Contract: the records the demo writes

All JSON, UTF-8, two-space indent, keys as listed. No hostname, user name or device
identifier appears in any record; the machine is named by its hardware model.

## `<out>/index/corpus.json` and `<out>/ATTRIBUTION.txt` (build)

The 008 shapes, byte-compatible with the Rust build's (research D9): `corpus.json` =
`{schema_version, corpus_identity, snapshot{edition, snapshot_date, parquet_sha256,
jsonl_sha256}, exclusions[], chunker{version, budget, cost}, embedder_fingerprint,
partial?, counts{articles, excluded{…}, selected, passages, passages_over_window,
url_mismatches}}`. `ATTRIBUTION.txt` = the four lines of `record.rs::attribution`.

Identity: `sha256(json.dumps({"snapshot", "exclusions", "chunker",
"embedder_fingerprint"[, "partial"]}, sort_keys=True, separators=(",", ":"),
ensure_ascii=False))`. For the full corpus and the pinned embedder this is
`ea0fc78c4dce30cebf066033327cc7a442385c605be8c67eadb772329eab2027` — a unit test asserts it.

## `<out>/wiki-build.json` (build)

```json
{
  "schema_version": 1,
  "feature": "019-python-wiki-demo",
  "recorded_at": "2026-09-17T10:00:00Z",
  "corpus_identity": "…",
  "partial": 2000,
  "host": {"os": "macos", "arch": "aarch64", "threads": 10, "python": "3.12.12", "xtriever": "0.1.0"},
  "models": {"embedder": "<fingerprint>", "reranker_for_demo": "<model id>"},
  "counts": {"articles": 2000, "excluded": {"lead_contains:may mean:300": 0, "lead_contains:may refer to:300": 0, "title_suffix: (disambiguation)": 0}, "selected": 2000, "passages": 0, "passages_over_window": 0, "url_mismatches": 0},
  "phases_ms": {"fetch_verify": 0, "read_exclude": 0, "chunk": 0, "embed_ingest": 0, "commit": 0, "merge": 0, "total": 0},
  "artefact_bytes": {"…": 0, "total": 0}
}
```

## `specs/019-python-wiki-demo/runs/<machine>-<stamp>-mmap-threads<n>.json` (measure)

```json
{
  "schemaVersion": 1,
  "feature": "019-python-wiki-demo",
  "corpus": "wikipedia",
  "machine": "MacBookPro18,3",
  "os": "macOS 26.6",
  "python": "3.12.12",
  "xtrieverVersion": "0.1.0",
  "build": {"loadPath": "mmap", "effectiveThreads": 10, "threadSource": "os.cpu_count (candle's default when RAYON_NUM_THREADS is unset)"},
  "index": {"bytes": 1076416088, "documents": 427947, "formatVersion": 2, "embedderFingerprint": "…", "rerankerModelId": "…", "corpusIdentity": "…", "partial": null},
  "openMs": 0, "embedderLoadMs": 0, "rerankerLoadMs": 0, "warmupMs": 0,
  "queries": [{"id": "q01", "depth": 0, "elapsedMs": 0, "engineMs": 0, "hits": 10, "peakBytesAfter": 0}],
  "perDepthMedianMs": {"0": 0, "5": 0, "10": 0, "20": 0},
  "perDepthMaxMs": {"0": 0, "5": 0, "10": 0, "20": 0},
  "latency": {"medianFusedMs": 0, "maxFusedMs": 0, "medianRerankedMs": 0, "maxRerankedMs": 0, "medianRerankedAtEngineDefaultMs": 0, "medianTotalMs": 0, "maxTotalMs": 0},
  "footprint": {"peakBytes": 0, "peakMethod": "phys_footprint", "residentPeakBytes": 0, "ceilingBytes": 600000000, "underCeiling": true},
  "parity": {"queriesCompared": 20, "lexicalBitIdentical": 20, "fusedOrderIdentical": 20, "denseMaxAbsDiff": 0.0, "rerankMaxAbsDiff": 0.0, "allBitsIdentical": 800, "hitsCompared": 800, "toleranceAbs": 0.001, "verdict": "PASS"},
  "notes": [],
  "recordedAt": "2026-09-17T10:00:00Z"
}
```

`footprint.peakBytes` is the ceiling's own measure: the process's lifetime peak
`phys_footprint` where the platform reports it (macOS: `proc_pid_rusage`,
`ri_lifetime_max_phys_footprint` — the counter iOS enforces and the device tests read as their
ledger, ADR-0010), `peakMethod: "phys_footprint"`; elsewhere the resident peak, `peakMethod:
"ru_maxrss"`. `residentPeakBytes` is always `ru_maxrss`, the figure every record before Feature
026 carried as `peakBytes`: it counts clean pages of the memory-mapped index, so on the full
corpus it exceeds the footprint by some 200 MB, varying between runs with what was paged in.
`underCeiling` judges `peakBytes`. `queries[].peakBytesAfter` stays the resident peak. Added in
Feature 026 without a schema version change: the fields added are new, and a reader of the
earlier records finds `peakMethod: "ru_maxrss"` in them.

`latency.medianRerankedMs` is depth 10 (the demo default), `medianRerankedAtEngineDefaultMs`
depth 20, `medianTotalMs` the per-query sum of depth 0 and depth 10 — so the row lines up
with the 009 and 018 records (fused / re-ranked / total at the app default).

## `specs/019-python-wiki-demo/runs/slice-<machine>-<stamp>-….json` (measure --against)

The same shape with `"corpus": "wikipedia-slice"`, `index.partial` set, and

```json
  "against": {"artefact": "target/xt-wiki-slice-rs", "corpusIdentity": "…", "identityEqual": true,
              "counts": {"…": 0}, "countsEqual": true, "documents": 0, "documentsEqual": true,
              "verdict": "PASS"}
```

The truth side is minted live from the `--against` artefact's responses in the goldens'
shape (bits of every score), so `parity` means the same thing as in the host record — except
that `fusedOrderIdentical` counts queries whose ids are in the same order at **every** depth
(the slice rule, FR-014), and the exit code is 1 unless both `parity.verdict` and
`against.verdict` are `PASS`.
