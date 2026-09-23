## 027 (PR B) — the sparse index, its format, and its measurement

DRAFT: the measurement, the gate and the sizes are filled in at T029.

**Red-checkpoint corrections** (Agent Operating Rule 4 committed these tests failing; one
expectation was changed afterwards, and it is listed here for review):

- `crates/xtriever-eval/tests/sparse_config.rs`, `the_weights_round_trip_and_refuse_a_damaged_file`:
  the file-size assertion went from **48 to 44 bytes**. The layout is unchanged. The test's own
  comment listed the three documents as 4 + 3×8, 4 and 4 + 8 bytes, which is 28 + 4 + 12 = 44,
  but the assertion said 48: an addition slip in the test, found when the implementation wrote
  44. The check is exactly as strict as before. It moved once more, **to 47**, when review
  round 6 changed the format itself: each record gained a one-byte truncation flag
  (29 + 5 + 13), and the test gained a check that a flag other than 0 or 1 is refused.

**Review rounds on the working tree** (before the measurement), for the record:

- Round 4: the harness flushed both add paths after ingesting, so every hybrid run failed
  before committing (fixed: one path per index kind); a failed `create_sparse` left an unusable
  directory (the query side is now copied first and removed on failure); truncated documents
  were counted before staging and lost on a cache hit (counted after staging; re-derived from
  the tokenizer for cached documents); the pin list was indexed by position (destructured);
  the directory-size walk read a missing directory as 0 bytes (it errors).
- Round 5: a query the query side cannot tokenise failed the whole search (it degrades to the
  text fields, `StageReport::sparse_skipped`; strict mode errors); `set_sparse_encoder`
  accepted another encoder (refused by identity, as the embedder is by fingerprint); a resumed
  encode trusted whatever bytes a crash left (it resumes from a synced progress record);
  the two cache keys' comparisons were copies with hand-listed fields (one serde-driven
  comparison); smaller clean-ups. One finding was left as it is: `stage_one`'s guard that a
  document has an expansion exactly on a sparse index repeats the public entry points' checks,
  deliberately — the entry points name the right method, the guard protects the one function
  every path goes through.
- Round 6: the harness did not warn about skipped expansions (it warns per query and prints
  the count); the FFI reported format version 2 for every index (it reports the index's own);
  my round-5 refactor had missed the sparse cache key, which still compared a hand-listed
  field set (it uses the shared comparison now — checked); `search_lexical`'s `Match(None, …)`
  also searched `_sparse` on a sparse index (it means the caller's own fields); the cache key
  did not record the passage recipe (it does); the scale rule was restated in the pipeline
  (the dense crate owns it, `validate_scale`); a cache hit re-tokenised every passage for the
  truncation flag (the flag is stored in each record, cache format 2, and the tokeniser-only
  helper is gone); the degraded query was built separately (one builder). Deferred to PR C:
  the FFI does not yet surface `sparse_skipped` — it needs an FFI record change, which
  regenerates the bindings (T034).
- Round 7: a document with no text was expanded from `[CLS]`/`[SEP]` alone (its `_sparse`
  field is now empty, whatever expansion comes with it); the FFI dropped `sparse_skipped` (it
  carries it; the generated bindings pick it up at their next build — this replaces the
  earlier deferral to PR C); adding to a sparse index without the encoder named a method no
  binding has (the message says documents are added only on the build host, through a handle
  holding the encoder); the harness held the encoder through the queries (detached after
  ingestion) and copied the corpus text on a cache hit (built only for an encode); the sparse
  cache lived per dataset, so another passage recipe would have deleted it (one directory per
  recipe). Recorded, not changed: the lexical stage's `Match(None, …)` includes `_sparse`, and
  the pipeline spells fields out instead; a schema flag would be cleaner but changes a core
  type (ADR-0016, Consequences). The owner kept PR B as one pull request.
- Copilot: the resume path truncated `weights.partial` before checking the progress record
  against it, and `set_len` would have zero-filled a short file (the record is now checked
  first — documents within the corpus, bytes within the file — and a stale one means a fresh
  encode); the sparse report stage had no positive round-trip test (added, pinning its JSON
  shape); the sparse query was built before the `k == 0` and empty-filter short-circuits (it
  is built after them — a search that returns nothing never touches the query side); the cache
  key's doc still named layout version 1; ADR-0016 still said `search_lexical` sends the query
  unchanged (it describes the `Match(None, …)` rule and the degrade).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
