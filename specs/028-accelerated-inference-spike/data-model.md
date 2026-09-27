# Data Model: Accelerated Inference Spike (Feature 028)

**Spec**: [spec.md](./spec.md) · **Research**: [research.md](./research.md)

## Variant

One build of the engine for one measurement.

| field | values | set by |
|---|---|---|
| `computePath` | `cpu` · `accelerate` · `metal` | the model crates' feature (none · `spike-accelerate` · `spike-metal`) |
| `rerankBatch` | `false` · `true` | `xtriever-rerank`'s `spike-batch` |
| `device` | `iPhone17,5` · `MacBookPro18,3` | where it runs |

**Rules**
- `spike-accelerate` and `spike-metal` are mutually exclusive: a build with both fails to compile
  with a message naming both.
- `cpu` with `rerankBatch: false` is today's default build, byte for byte (FR-011).
- The six phone variants and six host variants are the latency table's rows (US1); a variant that
  cannot build or run is a row with its error (SC-001).

## Run-record additions

The three harnesses keep their record shapes; each gains three fields.

| field | type | meaning |
|---|---|---|
| `computePath` | string | as in Variant; from the staged `compute-path.json` (phone) or the host script's flag |
| `rerankBatch` | bool | as in Variant, same source |
| `hitsDigest` | string, 64 hex | SHA-256 over every measured response, in order: for each query (the file's order), each depth (0, 5, 10, 20 — or the demo's fused then re-ranked), each hit in rank order — the external id, the fused score's bits, the re-rank score's bits or `-`. Two runs are identical exactly when their digests are equal. |

The iOS demo's record already names the corpus; the Swift package's and the Python demo's
records already carry per-depth latency, footprint and parity. Nothing is removed or renamed, so
earlier records still read.

## Parity comparison

Per variant, against the host's goldens (`target/xt-wiki/expected.json`), from the harnesses'
existing parity blocks:

| field | from |
|---|---|
| max dense-score difference | the parity block (absolute) |
| max re-rank-score difference | the parity block (absolute) |
| lexical scores bit-identical | queries of 20 |
| fused order identical at depth 0 | queries of 20 |
| order differences at depths 5, 10, 20 | hits whose id at a rank differs from the golden's |

**Pass**: every score within 1e-3 of the golden (the tolerance the device harnesses already use),
lexical bit-identical (the lexical stage is not accelerated), fused order identical at depth 0.

## Build measurement (host)

| field | meaning |
|---|---|
| `path` | as in Variant |
| `passages` | 5,183 (SciFact's corpus) |
| `embedSeconds` | wall time of the fresh-cache run minus the warm-cache run of the same path |
| `passagesPerSecond` | `passages / embedSeconds` |
| `projectedWikipediaHours` | `427,947 / passagesPerSecond / 3600` |
| `mixedNdcg10` | SciFact nDCG@10 of the CPU path searching the Metal-built cache (D10) |

## Verdict

One row per (path, device), from FR-010:

| field | meaning |
|---|---|
| `speedup` | 1 − median re-ranked phase ÷ today's (phone); build throughput ratio (host) |
| `underCeiling` | peak footprint < 600 MB (phone) |
| `repeatable` | two runs' `hitsDigest` equal |
| `withinTolerance` | the parity comparison's pass |
| `quality` | nDCG@10 and Recall@100 deltas on the three datasets — only for a path recommended by the four above |
| `verdict` | `go` if all hold, else `no-go`, with the failing condition named |
| `followUp` | what a feature would build for a `go` (the path, the fallback, the identity decision, the bench) |
