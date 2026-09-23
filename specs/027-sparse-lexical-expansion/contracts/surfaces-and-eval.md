# Contract: the harness (PR B) and the surfaces (PR C)

## `xtriever-eval` (PR B)

| configuration | definition |
|---|---|
| `hybrid-sparse-v1` | `hybrid-baseline-v2` with `SparseOption { scale: 10, boost: 1.0 }` |
| `hybrid-sparse-rerank-v1` | `hybrid-rerank-v3` with the same option |

`beir run --config hybrid-sparse-rerank-v1 --sparse-encoder-dir DIR [--sparse-cache-dir C]`.
The sparse cache: `C/<dataset>/{key.json, weights.bin}`; `key.json` holds the layout version
(2), the dataset, the encoder identity, the passage recipe (the lexical configuration's name),
the corpus SHA-256 and the document count, `weights.bin` every document's record in corpus
order (entry count `u32`, truncation flag `u8`, then `(u32 id, f32 weight)` pairs). A run
warns for every query whose expansion was skipped and prints how many were. A key
mismatch is a miss. While an encode runs it appends to `weights.partial` and, every 1,000
documents, syncs it and records `progress.json` (documents, bytes); an interrupted encode
resumes from that record, and the finished file is renamed to `weights.bin`. The throughput
(documents per second, thread count) and the lexical index's bytes go to stderr — never into
the report, which stays byte-identical across runs (spec 004 SC-006); the report's `stage`
block records the settings and the encoder identity.

## Command line (PR C)

`xtriever wiki build … --sparse-encoder DIR [--sparse-scale 10] [--sparse-boost 1.0]` builds the
artefact with the option; the build record gains the encoder identity, the truncation count and
the encoding throughput. The shipped artefact is not rebuilt with it (spec assumptions).

## FFI and bindings (PR C)

- `IndexConfig` gains `sparse: Option<SparseOptionConfig { encoder_dir, scale, boost }>`;
  `IndexHandle.create` honours it (the Python package's build path, FR-012).
- `IndexHandle.open` needs no new argument: a sparse index carries its query side.
- `IndexInfo` gains `sparse: Option<SparseInfo { scale, boost, encoder }>` and reports the
  index's own descriptor version rather than the build's constant.
- Swift and Kotlin wrappers expose the new `IndexInfo` field; nothing else changes for them.
- The packagers stage nothing new and never the encoder: a test fails if either script names
  the sparse manifest.
