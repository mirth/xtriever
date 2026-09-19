## 024 (PR B) — `merge` compacts the dense file; `dense_compact_dead_share`; the Wikipedia artefact on format 2

`HybridIndex::merge` now compacts the dense file (live rows only, in id order — the dense
half of the compaction ADR-0013 planned) before merging the lexical segments. A new optional
`HybridConfig::dense_compact_dead_share` (`0..=1`, default `None`) makes a commit that would
leave more than that share of the vectors dead a rewrite instead of an append; it is recorded
in the descriptor (serde default — the pipeline format version is unchanged) and exposed on the
FFI `IndexConfig` with a uniffi default, so existing Python and Swift callers are unaffected.

**The shipped Wikipedia artefact** is converted to format 2 without re-embedding
(`reference/convert_dense_v1_to_v2.py`, a record of the step, not a supported tool):
`wikidemo measure` over the host goldens — parity **PASS, 800/800 score bits**; peak RSS on
the idle run 1,013 MB, below the 019 record's 1,029 MB (contended runs peak higher, with
latencies that show it). The SciFact hybrid baseline (nDCG@10 0.7143693584, Recall@100 0.955) and the dense baseline
(0.6450816521 / 0.925) reproduce through the rebuilt format-2 cache with every Δ 0.0.

**Tests**: six pipeline tests (`compact_threshold.rs`: merge compacts and keeps every dense
score bit; the share compacts on the crossing commit and not at equality; `None` never; out
of range refused; a descriptor without the key reads `None`) plus the no-delete merge case
kept bit-identical, plus the plain-delete merge bit-identical while the dense file compacts;
a Python knob test. **One spec revision (FR-005 and SC-002, by the owner's decision), with evidence**: a merge after
*replacements* moves BM25 bits — reproduced on `TantivyIndex` alone with no dense stage, on
a branch where the lexical crate has no diff against `main` — so the fused-bit guarantee is
scoped to adds and plain deletes and the replacement case is left for a lexical spec; a
merge after plain deletes keeps every bit.

Gate green: fmt, clippy, nextest (workspace, 318), deny, the three cross-target checks, the
Python surface (34). No change under `deny.toml`, `apps/` beyond two doc lines, or any baseline.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
