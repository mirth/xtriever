# Contract: `xtriever wiki …` (crate `xtriever-cli`, binary `xtriever`)

**Feature**: `008-wiki-corpus` | `anyhow` errors, exit status 1 on any failure, 2 on usage

```text
xtriever wiki build --manifest reference/datasets/wiki-manifest.json
                    --snapshot-dir reference/datasets/wiki
                    --embedder-dir reference/models/all-MiniLM-L6-v2
                    --out target/xt-wiki
                    [--cache-dir target/xt-wiki-cache] [--load-path buffered|mmap]
                    [--limit N]            # first N articles only — development runs, identity marked "partial"
xtriever wiki verify   --index target/xt-wiki/index --embedder-dir …   # 100 % window + url + tiling checks, prints counts
xtriever wiki expected --index target/xt-wiki/index --embedder-dir … --reranker-dir … --queries reference/fixtures/008/queries.json --out target/xt-wiki/expected.json
```

## `build`

- Verifies the parquet and JSONL against the manifest before reading a line; a mismatch is a
  failure naming the file and both hashes.
- Reads `simple.jsonl` line by line; applies the manifest's exclusion rules in order; keeps
  per-rule counts.
- Chunks with `xtriever_analysis::chunk` under `cost = token_count`, `budget = 254 −
  token_count(title)`; a title whose own count is ≥ 254 is a failure (none exists — verified).
- Creates `<out>.partial/index` with `HybridIndex::create(HybridConfig::new(schema,
  dense_fields = ["text"]))` where schema = `title` (Text `standard_en`, indexed, boost 2.0),
  `text` (Text `standard_en`, indexed, boost 1.0); `candidate_depth`, `rrf_k`, `rerank_depth`
  at the pipeline defaults (100 / 60 / 20).
- Embeds per shard through the cache (data-model "Embedding cache"), one passage at a time,
  `TextKind::Passage`; ingests per shard with `add_embedded`; `commit`; `merge`.
- Runs the `verify` checks over the finished index; writes `corpus.json` inside the index,
  `wiki-build.json` and `ATTRIBUTION.txt` beside it; renames `<out>.partial` → `<out>`.
- Prints one line per phase with its wall time and, during embedding, progress every shard
  (`shard 117/118 hit` / `embedded 4096 in 385.2 s`).
- Exit 1 with a message on: manifest mismatch, a passage over the window, a URL mismatch, a
  title over budget, an I/O error, a pipeline error. Never writes `<out>` on failure.

## `verify`

Opens the index (read-only, mapped), reads every passage from the store, `token_count`s it,
re-derives every article URL from the title line and compares with the snapshot's `url`
(needs `--snapshot-dir`), checks `corpus.json` is present and its counts equal the index's,
prints the counts and `max_tokens_seen`, exits 1 on any violation. This is what `build` runs
as its last phase; standalone it re-verifies a shipped artefact.

## `expected`

Opens the index with both models, runs each measurement query at re-rank depths 0, 5, 20 with
`k = 10, explain = true`, and writes the FFI-shaped goldens (`external_id`, score bits per
stage, re-rank score bits) exactly as `xtriever-ffi`'s `fixture_index --scifact` does — the
device parity check reads this file. Deterministic under `RAYON_NUM_THREADS=1`.
