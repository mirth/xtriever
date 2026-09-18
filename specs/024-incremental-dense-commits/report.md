# Report: Incremental Dense Commits

**Status**: in progress — red checkpoint reached (PR A).

## The oracle (T002)

`crates/xtriever-dense/tests/index_oracle.rs::mint` run on the version-1 implementation:
three sequences (cosine / dot / euclidean, seeds `0x5EED0024..26`, dim 8, ~300 steps each —
143–161 adds, 55–62 replaces, 21–29 deletes, 48–56 commits, 9–17 reopens), five queries after
every commit (two with an `allowed` set), every hit's id and score bits →
`tests/support/v1_oracle.json` (289 KB). Minted twice: byte-identical. `replay` green on
version 1.

## Red checkpoint (C1, 2026-09-18)

`cargo nextest run -p xtriever-dense`: the crate's test targets `index_append`,
`index_compact`, `index_crash` and `index_prop` do not compile (`DenseStats`, `stats`,
`compact`, `set_compaction_threshold` absent); `index_persist` does not compile under
`--features mmap` (`compact`). Of the targets that compile: `index_errors` **3 failed, 9
passed** (the version-3 header, the version-1 refusal, the manifest truncations);
`index_persist` **2 failed, 5 passed** (the file names, the stale-generation sweep);
`index_oracle::replay`, `index_golden` (3), `index_mutation` (4) pass — they drive the
public API and are the oracle that must keep passing.
