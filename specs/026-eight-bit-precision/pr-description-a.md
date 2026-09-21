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

**Review round 3** (nine findings, all applied):

1. The oracle checker's row-count error referenced an undefined name; it names the file.
2. A cache hit now requires a complete float sidecar, so the "rebuild it" advice rebuilds.
3. The study rounded in `f32`; the `+0.5` and floor are in `f64`, which is what `f32::round`
   and the generator do. (The same lesson as round 1, finding 1, learned twice.)
4. `vector(id)` returns `Result<Option<_>>` and refuses a damaged row exactly as the scan does,
   instead of recovering NaN or zeros; tested on the same doctored files.
5. `rescored_by_sha256` in both fixture manifests is asserted by the fixture-validity tests,
   like `generator_sha256`, so an edited generator cannot leave stale goldens behind.
6. The scan decodes scale and norm once per row and hands them to the scorer.
7. The hybrid baseline no longer materialises a row per document as an existence check.
8. `validate_query` keeps one zero-norm check, on the quantised query.
9. **All three datasets evaluated in this pull request**, as Rule 5 requires for a
   ranking-affecting change to `dense` — PR B's gate re-runs them with the eight-bit models.

**Review round 4** (six findings, all applied):

1. `gen_024_fixtures.py`, the format-2 oracle's generator, pointed at a deleted file and scored
   floats; deleted, with ADR-0013 saying why and what replaced it.
2. `int8_reranker_study.py` still parsed format-2 rows; it reads the float sidecar.
3. **One generator per golden.** The scheme now lives once, in `reference/dense_format3.py`;
   `gen_004_fixtures.py` and `gen_005_fixtures.py` import it and mint format-3 expectations
   themselves. Run fresh, both reproduce the committed goldens byte for byte. Each manifest pins
   `scheme_sha256` beside `generator_sha256`, the fixture-validity tests assert both, and
   `gen_026_fixtures.py` is the stdlib-only checker CI runs.
4. `vector()`'s bound is stated with the floor: half the row's scale, which is at least half of
   `f32::MIN_POSITIVE`; the test helper and the persistence test use the scheme's step.
5. The scan's least norm is hoisted out of the loop and the Euclidean arm recovers from the
   scale the check already decoded; `row_at` is gone.
6. The eval cache is `<dataset>/{cache.json, vectors.f32.bin, index/}`: the float sidecar sits
   beside the dense crate's directory, not inside it, so no future sweep can remove it.

**Copilot on PR A** (six comments):

1. The pipeline checker scored only the ids the committed golden listed, so a document that
   rose under quantisation could never enter. It now scores every document the query's filter
   admits, breaks ties by ingestion order as the stage does, and cuts at the golden's depth;
   the committed goldens still pass.
2. The reader accepts a row byte of −128 rather than refusing it. Deliberate, and now stated in
   the contract: the format has no checksum over codes, a damaged byte is indistinguishable
   from a written one whatever its value, and refusing the one value the engine never writes
   would cost a pass over every row for one damage pattern in 256. The dimension bound already
   counts that byte (round 2).
3. `info.format_version == 2` in the Python test and `"format_version": 2` in the fixture
   goldens are the **pipeline descriptor's** version (`xtriever-pipeline` `FORMAT_VERSION`),
   which this feature does not change; the dense manifest is the one that moved to 3. The
   fixture goldens were re-minted in round 2 and carry format-3 score bits.
4. The quickstart's gate commands evaluated float weights: they now pass the eight-bit
   artefacts' pinned directories, with a note that T023 makes them the default.
5. The quickstart ran `hybrid-rerank-v2`; it runs `v3`, the configuration this description
   reports.
6. The packagers stage float directories by name, so no rebuild could ship the eight-bit
   artefacts: **T027a added** to PR B's plan for both packagers and both loaders, and the
   quickstart says so.

**Review round 5** (seven findings; six applied, one left to the owner):

1. The mutations checker skipped steps it did not model. It handles `replace` and refuses any
   other unknown op, as does the oracle checker.
2. The hybrid baseline's row lookup had become an unchecked slice; it returns a clean error
   naming the mismatch and the rebuild.
3. `export-vectors` reads only the sampled rows, by seek: 40 rows of FiQA cost 60 KB, not 88 MB.
4. The checker's `--write` serialises with the generators' `sort_keys`, so both mint the same
   bytes.
5. The cache-key test's baseline is version 3, with 2 as the miss.
6. The test-side quantiser was a copy of the crate's while claiming independence. The crate's
   module is `#[doc(hidden)] pub`, the suites use it through thin wrappers, and the comments say
   what is independent: `reference/dense_format3.py`, which mints every golden the suites
   replay, and the suites' own accumulation, cosine, order and persistence checks.
7. Rule 3's 800-line budget is exceeded. **Not split**: the owner waived the budget for this
   pull request when the review rounds began; the reviewer's split (format, eval harness,
   reference generators) is the natural one if that changes.

**Review round 6** (seven findings, all applied; the reviewer's own verdict this round was
that the correctness surface is clean and what remained was cleanup and one soft test):

1. The hybrid baseline and `export-vectors` opened the cache's index writably — a sweep and a
   full row-file read — for a row count and a width the cache key, the embedder and the
   sidecar already give. Neither opens it now; the export reads the key file instead.
2. The persistence test re-implemented `assert_recovered`; it calls it.
3. The sidecar sample opens the file once, and its byte layout is decoded and encoded in one
   place each.
4. ADR-0013's two remaining pointers to deleted files carry the superseded note.
5. The direction property asserted 0.999, a statistic of random vectors that a legitimate
   input could miss. It now asserts what the half-step bound implies: with
   `t = ‖e‖ / ‖v‖` and `‖e‖ ≤ (step / 2) · √dim`, the cosine is at least `(1 − t) / (1 + t)`.
6. A missing golden makes the checker exit 1 instead of passing with nothing checked.
7. **The stored norm is checked only under Cosine.** Dot and Euclidean never read it, so a
   damaged norm changed no score yet refused every search reaching the row and every
   `vector()` on it. The reviewer's Principle VI point stands: the index now degrades to the
   scores it would have returned. Tested for zero, NaN, infinite and negative norms under both
   metrics against the intact index's bits; the contract's refusal row says which metric
   reads the norm.

**Review round 7** (seven findings, all applied):

1. and 3. My round-6 sampler derived the width from the file's length, so an empty sidecar
   passed as width zero and the export wrote empty vectors with exit 0. There is one reader now,
   opened once, reading rows on demand for both the export and the hybrid baseline, with one
   completeness check: the exact length. The embedder's width is a field of the cache key, so
   no reader infers it. An empty sidecar is refused with the rebuild instruction (verified).
   The cache-key test caught the first version of this: the field was written and read but not
   compared, so a width change would have been a hit; `mismatch` compares it now.
2. The stored norm is computed once at `add` and carried in the pending set beside the codes;
   commit and compaction copy it.
4. The persistence test again asserts that an id never added returns nothing and that a
   reopened handle — the binary-search path — recovers the same rows.
5. T016 names the checker's real invocations; the flag it named did not exist.
6. The exported quantiser asserts finiteness in debug builds and says callers validate first.
7. Rule 3: the task list claimed each PR stays under 800 lines; it no longer does. The branch
   carries about 1,550 inserted lines of Rust, which the owner waived for this pull request.

**Evaluation, all three datasets, this build against the committed baselines.** Every delta is
inside the 0.005 bound; no metric on any dataset dropped by more than 0.001. Recall@100 is
unchanged everywhere except NFCorpus, where it rose by 0.0003.

| Dataset | Config | nDCG@10 | baseline | Δ | Recall@100 | baseline |
|---|---|---|---|---|---|---|
| SciFact | dense-baseline-v1 | 0.64573 | 0.64508 | +0.0007 | 0.925 | 0.925 |
| SciFact | hybrid-baseline-v2 | 0.71448 | 0.71437 | +0.0001 | 0.955 | 0.955 |
| NFCorpus | dense-baseline-v1 | 0.31575 | 0.31667 | −0.0009 | 0.31173 | 0.31145 |
| NFCorpus | hybrid-baseline-v2 | 0.35316 | 0.35351 | −0.0004 | 0.32178 | 0.32165 |
| NFCorpus | hybrid-rerank-v3 | 0.36189 | 0.36225 | −0.0004 | 0.32178 | 0.32165 |
| FiQA | dense-baseline-v1 | 0.36862 | 0.36867 | −0.0001 | 0.70606 | 0.70606 |
| FiQA | hybrid-baseline-v2 | 0.36933 | 0.36921 | +0.0001 | 0.70711 | 0.70711 |
| FiQA | hybrid-rerank-v3 | 0.39082 | 0.39096 | −0.0001 | 0.70711 | 0.70711 |

The study on the engine's own rows, all three datasets (SC-001 promises ≥ 0.99 agreement at
depth 100): SciFact 0.9956, NFCorpus 0.9947, FiQA 0.9947; top-10 kept 0.990–0.995; the stored
rows match the scheme byte for byte on every corpus. Embedding time for the record: SciFact
5,183 passages in 32 min under a load average of 70, NFCorpus 3,633 in 6.6 min, FiQA 57,638 in
97 min; the FiQA re-rank of 12,960 pairs took 25 min.

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
