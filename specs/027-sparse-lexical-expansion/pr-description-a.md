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
  under the embedder's name. Its streamed hash is shared as `sha256_file`.
- `SparseEncoder::token_ids` goes beyond the contract. It mirrors the embedder's tokenization
  helper, so a tokenization mismatch fails a test of its own instead of surfacing as wrong
  weights.

**Local gate:**

- `cargo fmt --check` and `cargo clippy --workspace --all-targets` (`-D warnings`) are clean.
- `cargo nextest run --workspace`: 378 passed, 81 skipped. The skips are model-backed tests, as
  before.
- `cargo deny check`: all four sections ok.
- `cargo check` passes for iOS, the iOS simulator and Android. wasm32 fails on `getrandom`, as
  tracked, and this pull request adds no dependency.
- `gen_026_fixtures.py`: both checks pass.
- `gen_027_fixtures.py --check` passes, and the `sparse_oracle` binary passes 7 of 7.
- The Python wheel builds, and `pytest` passes 34 of 34.

No `xtriever-core` trait, no `deny.toml` entry, no dependency and no on-disk format changes.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
