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

**Measured.** The scan benchmark on 100,000 rows × 384 dimensions: 5.05 ms against 40.3 ms for
the version-1 shape it has always been compared with, on a quarter of the memory traffic.
**This feature claims size and quality, not speed** — the owner waived a kernel probe, so that
number is recorded, not leaned on.

Gate: fmt, clippy with warnings denied, `cargo nextest run --workspace` 333 passed, deny, the
cross-target checks, and `reference/gen_026_fixtures.py` reporting zero differences.

Next, in PR B: both models from the pinned eight-bit artefacts, the three-dataset quality gate,
and one rebuild of every artefact.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
