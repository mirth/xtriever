# Research: Optional Sparse Lexical Expansion (Feature 027)

Every decision below is taken against the pinned versions in `Cargo.lock` (candle 0.9.2,
tokenizers 0.23.2, tantivy 0.26.2) and the spike's measurements; items are cited by path.

## D1 — The document encoder lives in `xtriever-dense`, as a new module

**Decision**: a `sparse` module in `xtriever-dense` holds the document encoder and the query side.

**Rationale**: the crate already carries everything the encoder needs and pins it — candle,
tokenizers without Oniguruma, safetensors loading through `bytes::read` (buffered or mapped),
file verification by size and SHA-256, the `Error::Model` shape. It is the crate of neural text
encoders; a sparse encoder is one more. The pipeline already depends on it, so it can hold the
concrete types without a trait (D5). `deny.toml` needs no change: no dependency is added.

**Alternatives considered**: a new `xtriever-sparse` crate — cleaner stage naming, but it would
duplicate the verification and loading code, add a crate to the dependency graph and to every
cross-target check, for no isolation the module does not already give. A trait in
`xtriever-core` — a contract change the spec does not authorise (Rule 2), and unnecessary (D5).

## D2 — Our own DistilBERT forward pass, not candle's `DistilBertForMaskedLM`

**Decision**: the encoder is candle 0.9.2's DistilBERT architecture written out in the module —
`candle_nn::{embedding, linear, layer_norm}` layers over the pinned `model.safetensors`, with the
masked-LM head (`vocab_transform`, `vocab_layer_norm`, the tied `distilbert.embeddings.word_embeddings`
as `vocab_projector` weight plus its own bias) — with **exact GELU** (`Tensor::gelu_erf`).

**Rationale**: `candle_transformers::models::distilbert::HiddenActLayer::forward` maps
`HiddenAct::Gelu` to `xs.gelu()` — candle's tanh approximation — whereas candle's own
`models::bert` maps the same config value to `gelu_erf()`, and the pinned model's reference
(`transformers` `DistilBertForMaskedLM`, config `activation: "gelu"`) uses the exact form. Using
candle's module would bake a known numerical difference into an encoder whose output the oracle
exists to check. Writing the forward pass over candle's layers is the precedent Feature 026 set
for the quantised BERT (`quantised_bert.rs`): composition of reused layers, not a tensor runtime
(Principle I holds). About 150 lines, read line for line against
`candle-transformers-0.9.2/src/models/distilbert.rs` and the reference.

**Alternatives considered**: candle's module with a wider oracle tolerance — rejected: the
tolerance would describe candle's approximation, not the model. Patching candle — not ours to
patch, and the pin exists to keep it unmodified (ADR-0001).

## D3 — The document recipe is the reference's, one document at a time

**Decision**: tokenise the document (the index's dense passage: its dense fields joined, as the
embedder sees it) with the encoder's tokenizer, truncated to 512 tokens including the special
tokens; run the encoder alone at the document's own length (no padding); take the logits'
maximum over positions per vocabulary entry; apply `log1p(relu(·))` then `log1p(·)` (the
manifest's `activation: log1p_log1p_relu`); zero the special tokens; keep entries above zero.

**Rationale**: `reference/sparse_spike.py::encode_documents` is the recipe the spike measured
(`logits * attention_mask` then `max` over positions, `log1p(relu)`, `log1p`, `values[:, special] = 0`).
One document per call keeps every tensor shape a function of the document alone, so a weight
cannot depend on its batch — the determinism rule the embedder and the re-ranker already follow
(Principle VI). With no padding, the mask multiplication is the identity.

## D4 — Weights become term frequencies in a reserved text field

**Decision**: each kept entry becomes the term `s<token id>`, repeated `round(weight × scale)`
times (half away from zero; entries rounding to zero dropped), in a reserved lexical text field
`_sparse` under the `standard` analyzer, whose field boost is the option's boost. Scale 10 and
boost 1.0 by default.

**Rationale**: tantivy scores term frequency, not weights; repetition is how Feature 012's
`bm25x` and the spike expressed weights, and scale 10 beat scale 100 at every stage of the spike
(BM25 saturation flattens heavy repetition). `s<id>` survives the `standard` analyzer
(`SimpleTokenizer` → `RemoveLongFilter(40)` → `LowerCaser`) as one token; a wordpiece string such
as `##ing` would not. A reserved name keeps the field out of the user's schema namespace; the
create call refuses a user field of that name.

## D5 — The pipeline holds concrete sparse types, not a trait

**Decision**: `HybridIndex` gains an optional sparse state: the recorded option, the
`SparseQuery` (always, for a sparse index) and an optional `SparseEncoder` (attached for
building, like the re-ranker is attached for re-ranking: `set_sparse_encoder`).

**Rationale**: the pipeline already depends on `xtriever-dense`, so it can use the module's types
directly; no `xtriever-core` trait changes (Rule 2), no trait of the pipeline's own. The encoder
is a build-host object (268 MB) that searching never needs, so it is attached only by callers
that add documents.

## D6 — The index carries its own query side

**Decision**: at creation the pipeline copies the encoder's `tokenizer.json` and `idf.json`
(the query-side table), byte for byte, into `<index>/sparse/`, records both SHA-256s in the
descriptor, and at open verifies them and builds the `SparseQuery` from them.

**Rationale**: the query side then needs nothing from the installation: no tokenizer of the
dense stage to match, no identity to compare, no packager change — every packager already stages
the index directory, so a device carries the 1.6 MB it needs and never the 268 MB encoder. The
spec's FR-006 asked for the dense stage's tokenizer plus recorded identities; the two tokenizers
share a vocabulary, normalizer and pre-tokenizer (checked against both pinned files on
2026-09-23), so the copy costs 0.7 MB per sparse index and removes a mismatch mode entirely.
FR-006, SC-005 and the acceptance scenarios are amended to match (spec, this date).

**Alternatives considered**: the dense stage's tokenizer plus the table in the index, with a
recorded identity (the spec's first wording) — a second artefact to keep in step and a refusal
path to test, for 0.7 MB.

## D7 — The query side: the table's positive tokens, each once

**Decision**: tokenise the query with the stored tokenizer (no truncation), keep the distinct
token ids that have a positive entry in the stored table and are not special tokens, each once,
ascending. The lexical query becomes `Bool { should: [Match(Some(f), text) for every user text
field, in schema order] + [Term(_sparse, "s<id>") for each kept id] }`; the field boost applies
to the terms. An index without the option keeps `Match(None, text)` exactly.

**Rationale**: `reference/sparse_spike.py::encode_queries` kept `{i for i in ids if i not in
special and idf[i] > 0}` — the spike's "plain" weighting, which matched or beat idf-weighted
terms. `Match(None, …)` would also run the user's query text through the `standard` analyzer
against `_sparse`, matching nothing meaningful; spelling the text fields out keeps the sum of the
same BM25 clauses and leaves `_sparse` to the token terms. `search_lexical` (a caller-built
query) is unchanged and documented as not adding the expansion.

## D8 — Format: descriptor version 3 only for sparse indexes

**Decision**: the descriptor gains `sparse: Option<SparseRecord>`; a sparse index's descriptor is
written as format version 3, every other index stays 2; a reader accepts 2 without the record and
3 with it. The id map and the passage store keep their own versions.

**Rationale**: an older engine must refuse a sparse index by name (spec edge case, FR-009) —
Features 015 and 024 added fields without a version bump because an older reader could safely
ignore them; this one cannot be ignored (the older engine would search `_sparse` with analysed
text and never with the tokens). Non-sparse indexes are untouched (FR-002). ADR-0016 records the
format, the reserved field and the stored query side.

## D9 — The oracle and its tolerance

**Decision**: `reference/gen_027_fixtures.py` (PyTorch and `transformers` at the versions of
`reference/requirements-027.txt`, CPU, float32, the pinned model files) encodes 12 fixture
documents — short, long (over 512 tokens), non-ASCII, one expected to keep few entries — and 8
queries, writing every kept `(token id, weight)` and every query's kept ids. The engine must match
every weight within **1e-4 absolute**, the same set of kept entries apart from any whose
reference weight is below 1e-4, every term frequency at scale 10 except where the reference
`weight × 10` lies within 1e-3 of a half, and every query's ids exactly.

**Rationale**: exact GELU and float32 on both sides leave differences of order 1e-6 in the
weights (whose range is 0–3); 1e-4 is two orders of margin and still an order below the 0.05 that
changes a term frequency at scale 10. Queries involve no model, so they must match exactly.

## D10 — Measurement: two harness configurations and a weights cache

**Decision**: `xtriever-eval` gains `hybrid-sparse-v1` (`hybrid-baseline-v2` plus the option)
and `hybrid-sparse-rerank-v1` (`hybrid-rerank-v3` plus the option), and a sparse-weights cache
beside the dense one (`target/xt-sparse-cache-027/<dataset>/`, keyed by the encoder identity and
the corpus hash, storing raw weights so the scale can vary without re-encoding), fed to the
pipeline by `add_encoded`. The quality gate compares with `hybrid-baseline-v2` and
`hybrid-rerank-v3` on the three datasets and checks SC-001/SC-002; the option-off baselines are
re-run to confirm SC-003.

**Rationale**: the encoder costs roughly five times the embedder per document (about 46 GFLOP for
a 256-token document, the masked-LM head alone a quarter of it); FiQA's 57,638 documents are an
overnight encode on CPU, and a cache is what makes the gate re-runnable. The spike's re-rank stage
(`crates/xtriever-eval/examples/rerank_runs.rs`) reproduced `hybrid-rerank-v3` to every printed
digit and is the cross-check for the new configurations.

## D11 — Three pull requests

**Decision**: PR A — the encoder, the query side and the oracle (dense crate, `reference/`).
PR B — the pipeline, the format, ADR-0016, the harness and the three-dataset measurement. PR C —
the surfaces: the command-line build, FFI, Python (open, search, create), Swift and Kotlin
(open, search, index information), and the packagers' check.

**Rationale**: Rule 3's ~800 changed lines per pull request; the spec's clarification asked for
the engine and its measurement before the surfaces, which A and B are. FR-014 is amended from
"two" to "the engine and measurement before the surfaces, each within the size rule".

## D12 — Nothing is claimed about speed

**Decision**: the build records its encoding throughput (documents per second, thread count) and
each dataset's lexical index size with and without the option; no latency budget is set and no
speed claim is made. SC-005's size bound (4× at the default scale) is the only size criterion.

**Rationale**: Principle IV forbids unbenchmarked claims; the option is judged on quality, and its
costs are recorded for the integrator who opts in.
