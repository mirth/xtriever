## 024 (PR B) — `merge` compacts the dense file; `dense_compact_dead_share`; the Wikipedia artefact on format 2

`HybridIndex::merge` now compacts the dense file (live rows only, in id order — the dense
half of the compaction ADR-0013 planned) before merging the lexical segments; staged changes
at merge time are rewritten directly (one generation advance, no append then rewrite). A new optional
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

**Not ranking-affecting.** The dense stage's scores are bit-identical across every
compaction by construction (the same rows, byte-copied, in id order) and the tests compare
every live row's score before and after; the lexical crate changes only by one test; fusion
and re-ranking are untouched. The proof is the SciFact reproduction above (every Δ 0.0
through a rebuilt format-2 cache) — SciFact alone suffices because nothing here can change
a score, and the three-dataset run is reserved for changes that can (CLAUDE.md). The idle
`wikidemo measure` record under `runs/` follows the 019 precedent of committing the full
run file (709 lines there) so the RSS and latency medians are reproducible from the artefact.

**Tests**: eight pipeline tests (`compact_threshold.rs`: merge compacts and keeps every
dense score bit, and the live handle equals a fresh open; a single-segment merge after
deletes keeps every fused bit while the dense file compacts; a no-delete merge compacts
nothing; staged changes at merge are one rewrite; the share compacts on the crossing commit
and not at equality, in f32; `None` never; out of range refused; a descriptor without the
key reads `None`, a bad persisted value is `Corrupt`), a lexical test pinning when a merge
moves BM25 bits, a dense test over four threshold boundaries, a Python knob test that reads
`info()` and the manifest. **One spec revision (FR-005 and SC-002, by the owner's
decision), with evidence**: tantivy's BM25 statistics are deletion-inclusive until a merge
physically drops the deleted or replaced documents (002 FR-025), so a merge that drops any
— deletes or replacements, more than one segment — moves BM25 and hence fused bits, while a
merge that drops nothing keeps every bit; the fused guarantee is scoped to the latter and the
former is left for a lexical spec.

Reviewed: three Copilot rounds and a `/code-review` (12 findings, all applied — the report
lists them). Gate green: fmt, clippy, nextest (workspace), deny, the three cross-target
checks, the Python surface, the reference suites. No change under `deny.toml`, `apps/` beyond two doc lines, or any baseline.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
