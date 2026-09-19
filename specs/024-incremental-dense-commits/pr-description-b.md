## 024 (PR B) — `merge` compacts the dense file; `dense_compact_dead_share`; the Wikipedia artefact on format 2

`HybridIndex::merge` now compacts the dense file (live rows only, in id order — the dense
half of the compaction ADR-0013 planned) before merging the lexical segments. A new optional
`HybridConfig::dense_compact_dead_share` (`0..=1`, default `None`) makes a commit that would
leave more than that share of the vectors dead a rewrite instead of an append; it is recorded
in the descriptor (serde default — the pipeline format version is unchanged) and exposed on the
FFI `IndexConfig` with a uniffi default, so existing Python and Swift callers are unaffected.

**The shipped Wikipedia artefact** is converted to format 2 without re-embedding
(`reference/convert_dense_v1_to_v2.py`, a record of the step, not a supported tool):
`wikidemo measure` over the host goldens — parity **PASS, 800/800 score bits**, peak RSS
1,035 MB beside the 019 record's 1,029 MB. The SciFact hybrid baseline (nDCG@10 0.7143693584, Recall@100 0.955) and the dense baseline
(0.6450816521 / 0.925) reproduce through the rebuilt format-2 cache with every Δ 0.0.

**Tests**: six pipeline tests (`compact_threshold.rs`: merge compacts and keeps every dense
score bit; the share compacts on the crossing commit and not at equality; `None` never; out
of range refused; a descriptor without the key reads `None`) plus the no-delete merge case
kept bit-identical; a Python knob test. One observation recorded in ADR-0013: a merge after
deletes changes BM25 statistics (tantivy garbage-collects deleted documents) — pre-existing
lexical behaviour, not a dense change.

Gate green: fmt, clippy, nextest (workspace, 317), deny, the three cross-target checks, the
Python surface (34). No change under `deny.toml`, `apps/` beyond two doc lines, or any baseline.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
