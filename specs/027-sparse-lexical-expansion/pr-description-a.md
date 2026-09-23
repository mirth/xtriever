## 027 (PR A) — the sparse document encoder, its query side and its oracle

The first of three pull requests for optional sparse lexical expansion. It adds the encoder
that turns a document into weighted vocabulary entries, and the model-free rule that turns a
query into the terms it looks for. Both are checked against a Python reference. **Nothing in
the engine uses them yet:** the pipeline, the index format and every surface are unchanged, so
this merges without moving a single result. PR B adds the index option and the measurement;
PR C adds the surfaces.

**The encoder** is `opensearch-neural-sparse-encoding-doc-v3-distill`, pinned by revision and by
the size and SHA-256 of each of its four files (`model::PINNED_SPARSE`, equal to
`reference/models/manifest-sparse-doc-v3.json` by test). A document is encoded alone, truncated
to 512 tokens. Its expansion is the masked-LM head's logits, maximised over positions, passed
through `log1p(log1p(relu(·)))`, with special tokens zeroed. This is the recipe the Feature 012
spike measured.

**Why the forward pass is written out rather than taken from candle.** candle 0.9.2's DistilBERT
maps `activation: "gelu"` to the tanh approximation. The model's reference, `transformers`'
`DistilBertForMaskedLM`, uses exact GELU, as candle's own BERT does. So the module builds the
same layers under the same tensor names over `candle_nn` (about 150 lines, read against
`candle-transformers-0.9.2/src/models/distilbert.rs`) and uses `gelu_erf`. This follows the
precedent Feature 026's quantised BERT set: it composes reused layers and is not a tensor
runtime (Principle I).

**The query side** needs no model. It keeps a query's distinct token ids that are not special
tokens and have a positive entry in the encoder's `idf.json`, in ascending order. Every entry in
that table is positive (the smallest is 0.016), so in practice the rule means "every non-special
token, once". `SparseQuery::open` verifies the tokenizer and the table against expected hashes
before parsing either one, because in PR B they are a sparse index's own files.

**The oracle.** `reference/gen_027_fixtures.py` encodes 12 authored documents and 8 queries with
PyTorch (CPU, float32, eager attention, one thread). The documents include a long one (938
tokens, truncated to 512), an empty one, a non-ASCII one, and one with literal `[CLS]`/`[SEP]`.
The script writes `reference/fixtures/027/`, and `--check` re-derives the fixtures byte for byte.
The Rust side:

| check | bound | measured |
|---|---|---|
| token ids | equal | equal, 12/12 |
| weights | within 1e-4 absolute | largest difference **1.6e-5** |
| kept entries | equal apart from reference weights < 1e-4 | equal |
| term frequencies at scale 10 | equal apart from `weight × 10` within 1e-3 of a half (3 such entries) | equal |
| query terms | equal | equal, 8/8 |
| determinism | identical bits | identical digests at 1, 3 and 10 threads, debug and release |

**Also in this pull request:**

- `reference/requirements-027.txt` is compiled **with hashes** (`uv pip compile
  --generate-hashes`) from Feature 012's pins, so `scripts/setup-reference-venv.sh 027` installs
  it like the other oracles. The 012 and 022 locks carry no hashes and cannot be installed that
  way.
- `model::verify_file` now takes the error to raise, so a bad encoder file is reported as
  `Error::Model { model: "opensearch-neural-sparse-encoding-doc-v3-distill", .. }` rather than
  under the embedder's name. `model::read_pinned` reads a pinned file once and checks the bytes
  it returns, and one `sha256_hex` serves every in-memory hash in the crate.
- `SparseEncoder::token_ids` goes beyond the contract. It mirrors the embedder's tokenization
  helper, so a tokenization mismatch fails a test of its own instead of surfacing as wrong
  weights.

**The spike's two harness examples** (`crates/xtriever-eval/examples/splade_field.rs` and
`rerank_runs.rs`, about 480 lines) are included on purpose. They produced the measurements the
spec rests on, and `rerank_runs` is T026's cross-check in PR B: fed the v2 lexical run and the
dense run, it must reproduce `hybrid-rerank-v3`. PR B's harness supersedes `splade_field`, which
PR B either removes or says why it keeps.

**Size.** This pull request is over Rule 3's ~800 changed lines: about 1,700 lines of Rust
(the encoder, the pins, three test files, the two examples) before fixtures, the generator and
the lock file. The owner accepted the size. Splitting it would have separated the encoder from
its tests.

**Review round 1** (`/code-review`, ten findings, nine fixed in this pull request, one accepted):

1. `load` hashed each file, then read it again to parse it, so the parsed bytes were not the
   verified bytes, and the 268 MB weights were read twice. Each file is now read once
   (`read_pinned`) and the verified bytes are the ones parsed.
2. A NaN logit became weight 0 (`f32::max` ignores NaN), so a broken forward pass would have
   produced empty expansions and a successful build. `encode` now refuses a non-finite logit.
3. `field_text` had no bound, so a large scale or a bogus weight could write billions of terms.
   It now returns `Result`. The scale must be in `1..=MAX_SCALE` (1,000), and a weight must be at
   most `MAX_WEIGHT` (4.5). That bound is the encoder's own: `ln(1 + ln(1 + f32::MAX))` ≈ 4.4967,
   checked by a test. So one entry is at most 4,500 occurrences.
4. `field_text` trusted that ids were ascending and unique, so a duplicated id doubled its term
   frequency. `Expansion::validate` now refuses, rather than repairs, anything the encoder could
   not have produced. PR B's `add_encoded` goes through it.
5. There were two SHA-256-to-hex implementations. Now there is one (`model::sha256_hex`), and
   the unused error parameter is gone.
6. `rerank_runs`' score cache was keyed without the model, so a second model silently reused the
   first model's scores. Each line now records the model. Lines from another model are not
   reused, and a line with no model recorded is refused (such a cache predates the key).
7. `splade_field` wrote its own field text, rounding in f32 with a trailing space. It now calls
   `field_text` with a whole-number `--scale`.
8. `rerank_runs` aborted on a torn last line and could have written a NaN score as `null`. A
   torn last line is now skipped with a warning, and a non-finite score is refused rather than
   written.
9. `rerank_runs` hard-coded the fusion constant, the depths and α. It now reads them from
   `RerankConfig::hybrid_rerank_v3()` and validates `--depth` and `--alpha` as the harness does.
   Re-run on SciFact after the change, with a fresh cache: nDCG@10 0.721936 and Recall@100 0.955000, equal to the recorded `hybrid-rerank-v3` (0.7219361, 0.955).
10. Size: accepted, as above.

**Review round 2** (`/code-review`, ten findings: eight fixed, one checked and left as it is,
one recorded as a follow-up):

1. The round-1 torn-line recovery didn't hold. It skipped a half-written last cache line, then
   appended after it, burying the fragment mid-file where the next run aborted on it. Every
   record is written with its newline, so an unterminated tail can only be an interrupted
   write. The loader now cuts the file back to its last complete line before appending.
   Checked on a copy of the SciFact cache with a fragment appended: the tail is cut, the second
   run loads cleanly, and all 6,000 pairs are reused.
2. A score cache that exists but can't be read (permissions, a directory) was treated as empty,
   which silently re-scored everything. Only a missing file now means an empty cache.
3. The cache writer was flushed by drop, which discards a failed final write. It is now flushed
   explicitly and the error is reported.
4. `Expansion::validate` did not check what its documentation promised. It now also refuses
   special-token ids and ids outside the 30,522-entry vocabulary. The special ids are a
   constant (`SPECIAL_IDS`) tied two ways: the encoder refuses a tokenizer that disagrees, and a
   model-free test compares the constant with the ids the reference recorded.
5. `rerank_runs` did not cut each stage's input to `hybrid-rerank-v3`'s `candidate_depth`
   (100). It does now. The exported runs held 100 per query, so the check was unaffected, and
   it still gives 0.721936 / 0.955.
6. Moving `splade_field` to the shipped rule could, in principle, refuse or re-round the spike's
   exports. Checked, and it doesn't. Across the three exports (1.24 M, 0.79 M and 12.9 M entries)
   no ids are out of order and no weight is zero, negative or above 4.5. The f32-to-f64 rounding
   change alters one entry's term frequency in SciFact and none in NFCorpus or FiQA.
7. **Follow-up, not in this pull request:** the float and eight-bit embedder loaders and the
   re-ranker's loaders still verify a file's hash, then open it again to parse. That is the
   gap round 1 closed for the sparse encoder. It predates this feature (Features 004, 006 and
   026), and closing it touches `xtriever-rerank`, which this pull request otherwise leaves
   alone. `model::read_pinned` is the fix to reuse.
8. The `SAFETY` comment on the crate's one `unsafe` block said the weights are mapped "after
   `model::verify_files`". The sparse loader verifies the mapped bytes instead. The comment now
   names both orders. The argument itself (the crate never writes to a mapped byte) is
   unchanged.
9. There were two hash-mismatch paths. `read_verified` now uses `model::check_sha256`, which
   takes the expected hash and the error to raise. The ordering check uses `windows(2)`.
10. `splade_field` counted terms by splitting each field string again. It now counts the
    separators, using `field_text`'s single-space contract, without restating the rounding rule.

**Review round 3** (`/code-review`, nine findings: six fixed, three left with reasons):

1. The score cache was keyed without the dataset. SciFact and FiQA ids are both numeric, so a
   cache shared between them would have served one dataset's scores to the other. Each line now
   records the dataset as well as the model. Lines from another dataset are not reused, and a
   line recording neither is refused. The existing SciFact cache, tagged with its dataset,
   still reproduces 0.721936 / 0.955.
2. A run naming documents the dataset doesn't hold was fused anyway, with the unknown ids
   dropped silently. It is now refused, naming the count and the first one. SciFact's runs given
   with `--dataset nfcorpus` report 30,000 unknown documents.
3. A flag with no value swallowed the next flag, so `--scores --depth 5` created a file named
   `--depth`. The parser, now shared by both examples in `examples/common/mod.rs`, refuses a
   missing value, a flag in the value position, an unknown flag, a repeated flag and a stray
   argument. Each case was checked by hand.
4. The sparse encoder's memory-mapped load, which goes through the crate's one `unsafe` block,
   was not tested against the buffered load. `tests/sparse_load_paths.rs` now does that (ADR-0007
   condition 3): bit-identical expansions over every fixture document.
5. `idf.json` was read whole just to hash it. The encoder never parses it, so it is now verified
   by the streamed hash (`verify_file`) and never held in memory. Left as is: a buffered load
   holds the weights twice while candle copies them. Every model in the engine loads that way,
   and `LoadPath::Mmap` is the path that avoids it. The loader's comment says so.
6. Left as is: the embedder's verify-then-read gap is round 2's follow-up, recorded above.
7. The `load_cache` doc comment repeated its first line. Fixed.
8. `flags()` and `repo_root()` were copied between the two examples. There is now one copy, in
   `examples/common/mod.rs` (see 3).
9. Left as is: each cache lookup allocates its key. That is two small allocations beside a
   cross-encoder call or a cache hit, a few milliseconds per sweep, and changing the map's shape
   would add code for no measurable gain.

**Local gate:**

- `cargo fmt --check` and `cargo clippy --workspace --all-targets` (`-D warnings`) are clean.
- `cargo nextest run --workspace`: 384 passed, 81 skipped. The skips are model-backed tests, as
  before.
- `cargo deny check`: all four sections ok.
- `cargo check` passes for iOS, the iOS simulator and Android. wasm32 fails on `getrandom`, as
  tracked, and this pull request adds no dependency.
- `gen_026_fixtures.py`: both checks pass.
- `gen_027_fixtures.py --check` passes. The `sparse_oracle` and `sparse_load_paths` binaries
  (with `--features mmap`) pass 8 of 8.
- The Python wheel builds, and `pytest` passes 34 of 34.

No `xtriever-core` trait, no `deny.toml` entry, no dependency and no on-disk format changes.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
