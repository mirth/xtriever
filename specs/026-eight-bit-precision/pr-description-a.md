## 026 (PR A) — dense format 3: eight-bit vectors

A dense row becomes `id u32 · norm f32 · scale f32 · codes dim×i8` — **396 bytes at dimension
384, against 1,544** — with one scale per vector and the dot product accumulated in integers.
Everything Feature 024 built is untouched: the manifest is still the truth, tombstones are still
an embedded bitmap, generations still advance on compaction, and a crash at any byte boundary
still leaves the previous state or the new one. Only the row body changed.

**The float vectors are not kept and there is no rescoring pass** (the owner's decision, recorded
in [ADR-0015](../../docs/adr/0015-eight-bit-vectors-and-models.md)). That buys a file small on
disk as well as in memory, and it costs a measured 0.0006 nDCG@10 on SciFact and NFCorpus with
Recall@100 unchanged — an eighth of the threshold this feature holds itself to. The consequence
is stated rather than hidden: **an eight-bit index does not rank identically to a float one**,
and the requirements say so instead of claiming identity.

**What that meant for the tests**, which is most of this diff:

- `vector()` no longer returns the bytes that were added. The tests that asserted it now assert
  what the format promises: every component within half a quantisation step.
- The scripted oracle proved, through formats 1 and 2, that the storage change moved no bit.
  Format 3 moves bits by design, so keeping that claim would have been a lie. The oracle is
  re-minted as `v3_oracle.json`, the file that made the old claim is gone, and what `replay` now
  proves is narrower and true: the same sequence gives the same results through appends,
  compactions and reopens.
- The property test's reference scorer and the 004 and 005 fixtures were recomputed **in
  Python** by `reference/gen_026_fixtures.py`, not by the crate: 177 search expectations, 6
  mutation expectations, 8 pipeline queries, and the oracle verified at **795 queries, 0
  mismatches**. The fixture manifests record which generator rescored them.

**Review round 1** (`/code-review`, nine findings, all applied in this pull request):

1. The Python generator rounded half-to-even in `f64` where the crate rounds half away from
   zero in `f32`. It now does the crate's steps (`f32` quotient, `f32::round`) and says why.
2. A denormal peak underflowed the scale to zero. The scale is floored at `f32::MIN_POSITIVE`;
   a vector that is zero at eight-bit precision is refused under Cosine at `add`, as a zero
   vector is.
3. The magic was still `XTDENSE2`. It is `XTDENSE3`; version 2 is refused by magic and by
   header with the rebuild instruction; the module, crate and Python-test spellings agree.
4. T007, T008 and T011 were ticked without their tests. `tests/format_v3.rs` and
   `tests/format_refusals.rs` exist now, the header names the scheme
   (`i8-symmetric-per-vector`) and refuses another or none, and a row whose scale or norm this
   engine could not have written is refused as corrupt by the scan — the scan rather than the
   open, because an open reads nothing beyond the manifest (Feature 024).
5. Euclidean computed and discarded an integer dot product per row. The metric is decided once
   per search; each metric is one loop.
6. Cosine divided a quantised dot product by float norms and exceeded one. The stored norm is
   now the recovered row's, the query's likewise, so a row's cosine with itself is one and
   nothing exceeds one beyond the last rounding. **Every golden moved again** for this: 78
   search cases, 8 pipeline queries, and the oracle re-minted and re-checked in Python.
7. `vector()`, the crate docs and the bench header said what version 2 did. They say what
   version 3 does.
8. The reference quantiser was copied into two suites. It lives once in `tests/support`.
9. The v1→v2 converter and its tests are deleted; they produced what this build refuses.

While there: a dimension bound of 132,104 at create and open, because past it the `i32`
accumulator would wrap silently. And T009 (an in-crate agreement test against a float index)
is **re-scoped, not ticked**: the float index no longer exists in the crate, and on synthetic
vectors the agreement sits at 98.7–99.2 % against a 99 % bound where real embeddings measured
99.5 %, so a random-vector test would test random vectors. The agreement is measured on real
corpora by the study and re-measured by PR B's three-dataset gate.

**Review round 2** (eight findings, all applied):

1. The evaluation example read `FlatIndex::vector(id)` from the embedding cache as if it were
   the embedder's output. The cache now keeps the embedder's floats beside the index
   (`vectors.f32.bin`, cache key version 3): `export-vectors` exports those, so the embedder
   check keeps its 1e-3 tolerance (re-run on SciFact: worst max_abs 2.2e-7, 0 outside), and the
   hybrid baseline feeds them to the hybrid index, which quantises once.
2. The study script named as SC-001's measurement could no longer run. It now reads those floats
   and the engine's format-3 rows, first checks the rows are the scheme byte for byte, then ranks
   the judged queries both ways. **SciFact, on the engine's own rows: candidate agreement at
   depth 100 = 0.9956, top-10 kept 0.9903, top hit unchanged 0.9933**; nDCG@10 0.6451 (floats)
   against 0.6457 (eight-bit), Recall@100 0.925 both.
3. The dimension bound assumed the codes the engine writes; a row byte on disk can be −128. It
   is now `i32::MAX / (127 × 128)` = 132,104, with a test that scores the worst row.
4. The 40-document fixture index and its goldens are re-minted, so the model-backed suites are
   green on this branch: 34 Python tests, 22 ignored FFI tests. PR B re-mints them once more with
   the eight-bit embedder.
5. One quantisation per vector, at `add`, carried in the pending set as codes (a quarter of the
   memory); the float zero-norm check is gone, the quantised one covers it.
6. The oracle's expectations are written by `reference/` (`gen_026_fixtures.py --write-oracle`);
   `mint` writes the steps. CI's `test` job now runs both check modes, which exit 1 on any
   difference, and the local gate in CLAUDE.md lists them.
7. `FlatIndex::open` docs say version 3 and name the version-2 refusal.
8. The Android README says version 3.

**SciFact after the rebuild**, against the committed baselines (the three-dataset gate is PR B's
T024): dense-baseline-v1 nDCG@10 0.64573 against 0.64508, Recall@100 0.925 unchanged;
hybrid-baseline-v2 0.71448 against 0.71437, Recall@100 0.955 unchanged.

**Measured.** The scan benchmark on 100,000 rows × 384 dimensions, after the review: 4.04 ms
against 31.5 ms for the version-1 shape it has always been compared with, on a quarter of the
memory traffic. **This feature claims size and quality, not speed** — the owner waived a kernel
probe, so that number is recorded, not leaned on.

Gate: fmt, clippy with warnings denied, `cargo nextest run --workspace` 348 passed, deny, the
three cross-target checks, `reference/gen_026_fixtures.py` and `--check-oracle` reporting zero
differences, and the Python tests with and without models (the fixture index and goldens
re-minted in round 2).

Next, in PR B: both models from the pinned eight-bit artefacts, the three-dataset quality gate,
and one rebuild of every artefact.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
