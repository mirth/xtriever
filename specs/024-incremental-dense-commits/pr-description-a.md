## 024 (PR A) — dense format version 2: append-only rows, tombstones, explicit compaction

`FlatIndex::commit` no longer rewrites `dense/index.bin`. Version 2 keeps the rows in an
append-only `vectors.<g>.bin` (`id · norm · vector`) described by an atomically replaced
`manifest.bin` (JSON header + a `roaring` tombstone set): a commit appends the new rows and
marks deleted or replaced rows dead; `compact()` (new; the pipeline's `merge` will call it in
PR B, and `commit` calls it when `set_compaction_threshold` is set) rewrites the live rows in
ascending id order under the next generation. No committed byte is ever modified; a crash at
any byte boundary reopens to the previous state (`index_crash` enumerates them all).
ADR-0013; ADR-0007's condition 2 amended (a mapped byte is never modified — the file is only
extended beyond every mapping or replaced by rename).

**Results are unchanged, bit for bit.** An oracle minted on the version-1 implementation
(`tests/support/v1_oracle.json`: 3 metrics, ~300-step add/replace/delete/commit/reopen
sequences, ~800 queries with and without filters) replays identically on version 2; the 004
goldens and the Swift fixture goldens reproduce (the fixture's `expected.json` differs only
in its `generated_by` commit hash); 1,000 random property cases survive `compact` and reopen.

**Bench** (`MacBookPro18,3`, 100k × 384): scan 31.6 ms vs 32.8 ms for the version-1 shape;
a 10-row commit writes **15.4 KB in 17.5 ms** (three fsyncs: rows, manifest, directory) instead of **154 MB in 175 ms**.

**Review round 1** (six comments, all applied): checked layout arithmetic in the manifest
decoder, a duplicate-live-id check at open, the property test honouring `PROPTEST_CASES`
(1,000 cases run), the threshold's strict comparison tested at equality, `compact`'s
read-only wording corrected, the fixture's committed `expected.json` kept (only its
provenance line had changed).

**Review round 2** (two comments, applied): the generation advances with checked
arithmetic (`Corrupt` at `u64::MAX`), and both `commit` and `compact` do every fallible step
before the manifest rename, so a failure never leaves the handle disagreeing with disk.

**Review round 3** (four comments, applied): mappings cover exactly the committed rows (a
crashed tail is never mapped), row-space exhaustion is refused before any I/O, the
generation error is plain, and a mapped handle keeps mapping after its first append and
after a compaction to zero rows.

**Review round 4** (four comments, applied): the live-row table is a map keyed by id (memory
follows rows, not the largest id), the directory is fsynced at the protocol's ordering
points, a temporary mapping is dropped before any truncation, and the buffered open reads
exactly the committed bytes.

**Version 1 is not read**: `open` refuses `index.bin` naming both versions (owner decision).
The fixture index is regenerated here; the shipped Wikipedia artefact and the pipeline knob
(`dense_compact_dead_share`) follow in PR B.

Dependencies: `roaring` (workspace version) added to `xtriever-dense`; `criterion` as a
dev-dependency — the workspace's first bench (`benches/scan.rs`). Gate green: fmt, clippy,
nextest (294), deny, the three cross-target checks, the Python surface (33).

Size: 2,038 + / 302 − over 18 files — ~900 lines of crate source, ~1,000 of tests, the
bench and the ADR — plus the 289 KB oracle. The split stated in the plan: PR B is the
pipeline, the FFI field and the artefacts.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
