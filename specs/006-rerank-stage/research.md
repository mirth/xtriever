# Phase 0 Research: The Re-rank Stage

**Feature**: `006-rerank-stage` | **Date**: 2026-09-13 | **Plan**: [plan.md](./plan.md)

Every registry item below was read from the pinned sources on 2026-09-13 and is cited as
`crate-version/path:line` (Agent Operating Rule 1); workspace items as `crate/path:line`. The
model was fetched and measured the same day (D2). No new external dependency is introduced: the
re-rank crate takes exactly the dense crate's set (D3, D12).

---

## D1. What the cross-encoder is, and what candle 0.9.2 does and does not ship

`cross-encoder/ms-marco-MiniLM-L-6-v2` is a `BertForSequenceClassification` with one label
(`config.json`: `architectures: ["BertForSequenceClassification"]`, `id2label` of length 1,
`hidden_size 384`, `num_hidden_layers 6`, `num_attention_heads 12`, `intermediate_size 1536`,
`max_position_embeddings 512`, `vocab_size 30522`, `model_type "bert"`,
`sbert_ce_default_activation_function: torch.nn.modules.linear.Identity` — the score is the raw
logit). The reference head, confirmed by running the pinned `transformers 5.17.0` pipeline
against a hand-assembled forward on 2026-09-13 (both give `8.845856666564941` on the README
example): `logit = classifier(tanh(pooler.dense(hidden[:, 0])))` — the CLS row, a 384×384 dense
layer with tanh, a 384×1 dense layer. Dropout is identity in eval mode.

**candle-transformers 0.9.2 ships the encoder only.** `models/bert.rs` has `BertModel::load`
(`candle-transformers-0.9.2/src/models/bert.rs:466`) and `BertModel::forward(input_ids,
token_type_ids, attention_mask: Option<&Tensor>) -> Tensor` (`:495`) returning the last hidden
state `[batch, seq, hidden]`; there is **no pooler and no classification head** in the file
(`grep -n pooler` finds nothing; the only heads are `BertForMaskedLM` at `:602` and its MLM
parts). `Config` (`:57`) is a plain `Deserialize` struct without `deny_unknown_fields`, so the
cross-encoder's extra keys (`id2label`, `architectures`, `sbert_ce_…`) are ignored;
`classifier_dropout: Option<f64>` and `model_type: Option<String>` (`:75-76`) absorb the rest.
`hidden_act: "gelu"` maps to `HiddenAct::Gelu` → `gelu_erf` (`:41`), the exact-erf GELU the
reference uses.

**Decision**: `xtriever-rerank` composes the head from candle-nn primitives, the same way
Feature 004 composed mean pooling: `BertModel::load(vb.pp("bert"), &config)` for the encoder
(the safetensors keys carry the `bert.` prefix, D2), `candle_nn::linear(384, 384,
vb.pp("bert.pooler.dense"))` and `candle_nn::linear(384, 1, vb.pp("classifier"))`
(`candle-nn-0.9.2/src/linear.rs:84`, loads `weight` `(out, in)` and `bias`), `Tensor::narrow(1,
0, 1)` for the CLS row (`candle-core-0.9.2/src/tensor.rs:878`), `Tensor::tanh`
(`tensor.rs:637`, `unary_op!(tanh, Tanh)`), `Linear::forward` (`linear.rs:42`; on a 2-D input
it is a plain `matmul` with the transposed weight plus the bias), `Tensor::to_scalar::<f32>`
(`tensor.rs:662`) after `squeeze`. Twelve lines of glue, no tensor code of our own — Principle I
is satisfied exactly as in 004 (the commodity component is candle; the head is model-specific
wiring the reference defines).

**Rationale**: the alternative — waiting for or vendoring a `BertForSequenceClassification`
from a later candle — is blocked by ADR-0001 (candle pinned at 0.9.2 because 0.10+ hard-codes
`onig`). The head is three tensors; a hand-written classification head over a reused encoder is
not "a tensor runtime built by hand".

## D2. The pinned model, measured

Fetched from `https://huggingface.co/cross-encoder/ms-marco-MiniLM-L-6-v2` at the revision the
hub reported as current on 2026-09-13 (`sha 233902d25c440f23af6f7d6e94d2946bac0bee0a`, last
modified 2026-08-09); every value below is measured from the files, not read from a model card:

| File | Bytes | SHA-256 |
|---|---:|---|
| `config.json` | 794 | `380e02c93f431831be65d99a4e7e5f67c133985bf2e77d9d4eba46847190bacc` |
| `tokenizer.json` | 711,396 | `d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66` |
| `model.safetensors` | 90,870,598 | `821d1aa69520101d6e0737f78a042ae25b19e5cb9160701909d10434f4aeb0ae` |

Safetensors header (12,090 bytes, `format: pt`): **106 tensors — 105 `F32` and one `I64`**
(`bert.embeddings.position_ids [1, 512]`, an index buffer candle's `BertEmbeddings` never reads —
`bert.rs:194` builds positions itself). Every key is prefixed `bert.` except `classifier.weight
[1, 384]` and `classifier.bias [1]`; `bert.pooler.dense.weight [384, 384]` and `.bias [384]` are
present. The 004 model had 103 `F32` tensors and no pooler; the two extra `F32` tensors here are
the pooler's, the head's two make 105 — the count is asserted at load exactly as 004 FR-002 did.

`tokenizer.json`: WordPiece, 30,522 entries, `BertNormalizer` (`lowercase: true`,
`strip_accents: null`), `TemplateProcessing` with the pair template `[CLS] A [SEP] B [SEP]`
and type ids `A → 0`, `B → 1`; **no truncation and no padding section** (the file leaves both to
the caller, unlike the sentence-transformers file 004 loaded). `tokenizer_config.json` says
`model_max_length: 512`, `do_lower_case: true`.

**Pins** (`model::PINNED`, `reference/models/manifest-rerank.json`): the three files above,
`max_tokens: 512`, `hidden: 384`, `f32_tensors: 105`. `tokenizer_config.json` is not pinned —
nothing reads it at run time; the two values it carries are asserted from `config.json` and the
tokenizer instead.

**Model identity** (spec FR-004; the `Reranker::model_id` string):

```
cross-encoder/ms-marco-MiniLM-L-6-v2@233902d25c440f23af6f7d6e94d2946bac0bee0a;weights=sha256:821d1aa69520101d6e0737f78a042ae25b19e5cb9160701909d10434f4aeb0ae;max_tokens=512;trunc=longest_first;head=cls-pooler-tanh-linear;act=identity;dtype=f32;engine=candle-0.9.2
```

Every input whose change would change a score: repository, revision, weight hash, truncation
length and strategy (D4), the head (D1), the output activation, the weight precision, the engine
version. Thread count and CPU architecture are excluded, as in the 004 fingerprint, for the
same reason (bit-identity is promised per architecture).

## D3. Load paths: buffered by default, mapped behind `mmap` — constitution amended to v1.3.0 (ADR-0009)

Spec FR-003 asks for "the same two load paths as the dense stage (safe by default, mapped
opt-in)". Constitution v1.2.0 Principle VII confined hand-written `unsafe` to `xtriever-dense`,
and ADR-0007's decision was "exactly one hand-written `unsafe` block in `xtriever-dense`". A
mapped weight loader here therefore needed one of: a constitution amendment plus an ADR; a
sideways dependency `rerank → dense` to reuse `bytes::map_readonly` (which is `pub(crate)` —
exposing it modifies `xtriever-dense`, forbidden by FR-022, and Principle V's direction is
`core ← stage crates`, not stage ← stage); or buffered-only loading (this plan's first
recommendation, on the strength of ADR-0007 condition 5's measurement: mapping the weights
saves ~13 % of load-time peak and nothing at steady state, since candle 0.9.2 copies every
tensor onto the heap — `candle-core-0.9.2/src/safetensors.rs:115-137`).

**Decision (the owner's, 2026-09-13)**: amend. Constitution **v1.3.0** (MINOR): "Hand-written
`unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in
read-only memory mapping behind a non-default feature; … (ADR-0007, ADR-0009)".
**[ADR-0009](../../docs/adr/0009-unsafe-readonly-mmap-in-rerank.md)** admits in
`xtriever-rerank` the same eleven-line `bytes::map_readonly` behind a non-default `mmap` feature
under ADR-0007's conditions 1–4 (safe default; one item-scoped block with the real invariant;
bit-for-bit parity test against the buffered path; `memmap2` optional via `cargo add`,
`deny.toml` unchanged). Condition 5's deletion clause is **not** re-applied — the model family
was already measured and kept; the fresh-process measurement per path is recorded for the record.
`MiniLmCrossEncoder::load(dir, LoadPath)` mirrors `MiniLmEmbedder::load` exactly:
`LoadPath::{Buffered, Mmap}` (the latter `#[cfg(feature = "mmap")]`), `bytes::read` in the
weight loader only (there is no index file in this crate), `VarBuilder::from_slice_safetensors`
(`candle-nn-0.9.2/src/var_builder.rs:658`, the safe constructor) on either byte source.

**Rationale** (the owner's, recorded in ADR-0009): the two model-loading stage crates should be
symmetrical, and an on-device deployment that loads both models at start-up pays the transient
file buffer twice under buffered-only; the mapping is the mechanism the on-device budget will use
and should exist for both models before that feature measures it. The block is duplicated rather
than shared: a helper crate for eleven lines would sit outside Principle V's structure, and a
`core` home would put `unsafe` in the crate the constitution most wants free of it.

## D4. Tokenization: pairs, `longest_first` at 512, and the empty-passage quirk of the reference

The reference call is `tokenizer(query, passage, truncation=True, max_length=512)` on the fast
tokenizer (`transformers 5.17.0`, backed by the same `tokenizers 0.23.2` the crate uses). Measured
on 2026-09-13:

| Input | Reference ids | Note |
|---|---|---|
| `("a query", "")` | `[CLS] a query [SEP]` (4 ids, all type 0) | **the empty passage is dropped**: `PreTrainedTokenizerFast._encode_plus` builds `[(text, text_pair)] if text_pair else [text]`, so an empty second string falls back to single-sequence encoding — no second `[SEP]` |
| `("", "a passage")` | `[CLS] [SEP] a passage [SEP]` (5 ids; types `0 0 1 1 1`) | an empty *query* keeps the pair template |
| `("", "")` | `[CLS] [SEP]` | single-sequence of an empty string |
| 300-word query, 300-word passage | 512 ids, 256 of each type | `longest_first` trims the longer side token by token until the pair fits |

`tokenizers 0.23.2`: `Tokenizer::encode<E: Into<EncodeInput>>(input, add_special_tokens)`
(`tokenizers-0.23.2/src/tokenizer/mod.rs:871`); a `(query, passage)` tuple converts to
`EncodeInput::Dual` (`mod.rs:298-305`), a `&str` to `Single`. `TruncationParams { max_length,
strategy, stride, direction }` (`src/utils/truncation.rs:23`), `TruncationStrategy::LongestFirst`
is the `#[default]` (`:53-55`), so `with_truncation(Some(TruncationParams { max_length: 512,
..Default::default() }))` is the reference strategy. No padding is set (each pair is scored at
its own length, spec assumption; `attention_mask` is then all ones and `BertModel::forward`
takes `None`, which is `ones_like` at `bert.rs:505`). `Encoding::get_ids` / `get_type_ids` /
`get_attention_mask` (`src/tokenizer/encoding.rs:147/151/171`).

**Decision**: `encode(query, passage)` mirrors the reference literally: if `passage.is_empty()`
encode `query` as a single sequence; otherwise encode the pair. The goldens include all three
empty combinations and an over-length pair, and each golden pair carries its `input_ids` and
`token_type_ids` so the parity test (`tokenize_for_test`, the 004 device) pins the tokenization
independently of the scores. The empty-passage rule is deliberately reference-faithful rather
than "sensible": FR-006 says the reference's score comes back, and the reference scores the
query alone.

## D5. Determinism: one pair per forward at its own length

004 established (research D2) that candle folds a batch into gemm's `M`
(`candle-core-0.9.2/src/cpu_backend/mod.rs:1400-1403`), so a text's result depends on the batch
it is in unless every text runs alone at a fixed shape. Here each pair runs alone, batch 1,
sequence length = its own token count — a pair's shapes depend only on the pair, so its score
cannot depend on the other passages in the call, their order, or the call boundaries (FR-005).
Thread count: 004 proved bit-identity between `RAYON_NUM_THREADS=1` and `4` at a fixed shape
in separate processes (`tests/embed_determinism.rs`); the same test is written for the
cross-encoder over the golden set (every shape in the set). If it fails it is a **finding**
(Rule 6) reported with the differing pairs, not a widened tolerance — and the identity string
would then have to carry the thread count, which the spec does not want.

Scores are `f32` logits read with `to_scalar`; no `f64` accumulation of our own is involved
(unlike the flat index's dot products), so there is nothing to round.

## D6. The budget loop and the clock (FR-007)

`Reranker::rerank(&self, query, passages: &[Passage<'_>], budget: &Budget) ->
Result<Vec<Option<f32>>>` (`xtriever-core/src/traits.rs:97-102`); `Budget { max_time:
Option<Duration>, max_items: Option<usize> }` (`types.rs:305-310`); `Passage { id, text }`
(`types.rs:295-299`).

**Decision**: the loop is

```
let start = Instant::now();
for (i, p) in passages.iter().enumerate() {
    if max_items.is_some_and(|n| i >= n) { break }
    if max_time.is_some_and(|t| start.elapsed() >= t) { break }
    out[i] = Some(score_pair(query, p.text)?);
}
```

The time check runs **before** each pair, the first included, with `>=`: a zero item limit and
a zero time limit both score nothing (Story 2 scenario 4 and the "spent before the first pair"
edge case agree); a limit the first pair fits — the check for pair 1 sees only the nanoseconds
since entry — always yields at least one score (scenario 2). `Instant` is permitted here:
Principle III names `xtriever-rerank` among the leaf crates, and the constitution's `Instant`
ban is on the pure crates. The pipeline still never reads a clock (D8): it computes the
*remaining* time from its caller-supplied source and hands it over as `max_time`. A non-finite
logit is `Error::Model` (FR-008); the result vector always has `passages.len()` entries.

## D7. The passage text store: `passages.bin`, pipeline format version 2 (Q2 = A; FR-010; ADR-0008)

The stages cannot supply text: `LexicalIndex` has no document-fetch method
(`xtriever-core/src/traits.rs:22-44`) and `TantivyIndex` exposes none (`xtriever-lexical/src/
index.rs`, public items: `create`, `open`, `merge` plus the trait), and FR-022 forbids adding
one. The pipeline stores the dense passage text (`HybridIndex::passage`, `index.rs:245-258`,
the configured fields joined by one space — exactly what the embedder saw and what the
cross-encoder should see) itself.

**Format** (`<dir>/passages.bin`, the dense `index.bin` conventions — magic, length-prefixed
JSON header, fixed-width tables, checked offset arithmetic, `.tmp` + `rename`):

```
"XTPASS01"                              8-byte magic
u64 LE                                  header length
{"format_version":1,"count":N}          JSON header
(N + 1) × u64 LE                        byte offsets into the text block; text(i) = block[off[i]..off[i+1]]
UTF-8 text block
```

Position = internal id, like `ids.json` (`ids.rs:18-24`); a deleted or never-assigned slot is an
empty range (deleted ids are never returned by a stage, so the ambiguity with a genuinely empty
passage — the "document with no text" edge case — is harmless, and both score as the reference
scores an empty passage, D4). **Reads are on demand**: `open` loads the header and the offset
table (8 bytes per id — 800 KB at 100k documents) and reads a hit's text with `seek` +
`read_exact` on a freshly opened handle when a search needs it; the corpus text is never held
in memory whole. This is what makes Q2 = A cheap at run time: FiQA's 48 MB costs disk, not RSS
(the constitution's on-device RSS clause, Principle III). Writes are whole-file at `commit`:
pending texts (a `BTreeMap<u32, String>` staged by `add`/`add_embedded`, `None` by `delete`)
are merged with the previous generation's bytes into `passages.bin.tmp`, then renamed — the
same O(N) commit the dense stage already pays for `index.bin`.

**Versioning**: every hybrid index has the file from now on. A v1 directory (Feature 005) has
none, so it is **not readable by this build**: `FORMAT_VERSION` becomes 2 and `Descriptor::read`
refuses version 1 by its existing check (`descriptor.rs:45-51`), naming both versions; the
message adds "rebuild the index". No migration is written — the only v1 directories are the
harness's throwaway index directories and test temp dirs. This is an on-disk format change to
the pipeline's own format and takes an ADR under Principle V: **ADR-0008** (written with this plan and accepted by the owner on
2026-09-13; it lands with the PR that bumps the version). The descriptor also gains `rerank_depth` (D8), which the
same bump covers. The edge case "re-ranker attached to an index whose text store is absent" is
therefore resolved at open, before any re-ranker is involved.

**Commit order** becomes lexical → dense → **passages** → id map → descriptor, under the same
`commit.pending` marker; the four-count check gains a fifth count (the store's `count` must
equal the id map's length, i.e. the assigned-id space — not the live count, since deleted slots
stay). *Rejected*: storing text as JSON in `ids.json` (rewrites 48 MB of JSON per commit and
parses it whole at open); reading text back from tantivy's stored fields (needs a lexical-stage
API change, FR-022); a per-document file (100k files).

## D8. Re-ranking in `search`: candidate set, ordering rule, budget, degradation (Q1 = A; FR-011–FR-015)

Today's search is eight steps (`search.rs:63-148`): short-circuits, filter once, lexical, dense
under check points A/B, fuse or degrade, build hits. Re-ranking is step 9, after the fused (or
degraded) list exists and before hits are built.

**Decision**:

- **Depth** `d`: `HybridConfig.rerank_depth` (default 20, persisted in the descriptor as
  `candidate_depth` and `rrf_k` are) overridable per call by `SearchOptions.rerank_depth:
  Option<usize>`; `0` disables. The re-ranker itself is attached to the handle, not the
  directory: `HybridIndex::set_reranker(Option<Box<dyn Reranker>>)` — any re-ranker can be
  attached to any index, so its identity is not stored (the eval report records it, D10).
- **Candidate list**: the fused list is built to length `max(k, d)` (today it is cut at `k`),
  so a candidate at fused rank `k < r ≤ d` can be promoted into the top `k`. The first
  `min(d, len)` candidates become `Passage`s with text from the store (D7).
- **Budget** (FR-013): check point **C** before the stage (the same `check_budget` as A/B); the
  re-ranker receives `Budget { max_items: opts.budget.max_items, max_time: limit −
  elapsed() (saturating) }` when both a limit and a time source exist, `max_time: None`
  otherwise (a limit without a source is already reported as `time_limit_ignored`, 005 D7 —
  the re-ranker measuring the *whole* limit itself would contradict "remaining time").
- **Ordering** (Q1 = A): scored candidates first, `(rerank_score DESC, DocId ASC)`; then every
  unscored candidate — not reached by the budget, and beyond `d` — in fused order; truncated
  to `k`. `HybridHit.score` keeps its meaning (the fused score, or the lexical score when
  degraded); the re-rank score is a separate `rerank_score: Option<f32>` on the hit, so the
  LTR feature gets both numbers and nothing is overwritten. The response's `stages.rerank`
  reports `candidates` (passages handed over) and `scored`.
- **Degradation** (FR-014, FR-015): `rerank` returning `Err` ⇒ the fused list with
  `stages.rerank.skipped = StageError(..)` in the default mode, the error in strict mode;
  check point C spent ⇒ `BudgetExceeded` / `BudgetExhausted("rerank stage: …")` as 005 does for
  dense. A wrong-length or non-finite result is `Error::Model { model: model_id, message }`
  **in every mode** — it is a defect, not a stage failure. A partial result (`None`s) is not a
  degradation and is not an error in strict mode: it is the contract working. The dense stage
  degrading does not skip the re-ranker: the "fused" list is then the lexical list, re-ranked
  the same way (Story 3 scenario 6).
- **Explain** (FR-016): `HitExplain` gains `rerank_score: Option<f32>` and `rerank_rank:
  Option<u32>` (1-based position among the scored candidates); `features()` grows to 7 entries
  under `features::RERANK_SCORE` (`xtriever-core/src/types.rs:359`) and the pipeline's own
  `RERANK_RANK = "rerank.rank"` (no core change). Absent = `None` / `NaN`, never 0.

**Rationale** for `max(k, d)`: without it "depth 100, k 10" would re-rank candidates that
cannot be returned; with it the re-ranker's promotion range is honest. For "one rule" (Q1 = A):
the response needs no second scale and the partial case is exactly the full case with a
shorter scored prefix — one code path, one property test (scored prefix sorted by re-rank
score, suffix in fused order, set equality with the fused list).

## D9. Cost, for planning and for the budget defaults

Reference pipeline on this host (`torch 2.14.0`, one thread, no warm-up): 34-token pair 7 ms
after warm-up, 512-token pair 53 ms. candle at 4 threads embedded a fixed 256-token text in
94 ms (004 F-005, thread scaling is poor); a cross-encoder pair at a BEIR passage's typical
150–300 tokens is expected at **50–100 ms**, so depth 20 is 1–2 s per query and the
three-dataset baseline (300 + 323 + 648 queries) is ~40 minutes at `RAYON_NUM_THREADS=4`. The
observations (D11) replace this estimate with measurements. Memory: the same 90.9 MB of `F32`
tensors as the embedder ⇒ ~225 MB peak in a fresh buffered load (004 measured 226 MB), ~90 MB
steady; the harness process, already 776 MB at FiQA, grows by that.

## D10. Harness: `hybrid-rerank-v1`, the report extension, the delta against the guarded baseline (FR-017–FR-019)

- `run.rs` gains `RerankConfig { name: "hybrid-rerank-v1", hybrid: HybridConfig
  (hybrid_baseline_v1), rerank_depth: 20 }` with `validate` (`rerank_depth ≥ 1`, `≤ k`,
  hybrid valid). The runner is unchanged: `execute_external(dataset, name, k, &mut retrieve)`
  (`run.rs:490-520`) names no stage; the example's closure attaches the re-ranker and passes
  `rerank_depth` through `SearchOptions`.
- `report.rs`: `StageInfo` (`report.rs:54-67`) gains `reranker_model_id: Option<String>` and
  `rerank_depth: Option<usize>` (`skip_serializing_if = "Option::is_none"`, so every existing
  report file serialises unchanged — the 004 additive discipline); `kind` is `"hybrid-rerank"`.
  `Observations` gains `rerank_ms`, `rerank_pairs` and `rerank_model_bytes_buffered`
  (optional, same rule). `delta` still refuses mixed configurations (004 FR-021); the
  cross-configuration table is `compare` (`report.rs:356`), already in place — FR-018's
  "delta table against `hybrid-baseline-v1`" is a `compare` table, the SC-008 verdict is
  computed and written by hand in the report as 005 did for SC-011.
- `examples/beir.rs`: `--config hybrid-rerank-v1` runs `evaluate_hybrid` with a re-ranker
  attached (a `TimedReranker` wrapper in the example — `Instant` is fine in a binary — sums the
  stage's wall time and pair count for `rerank_ms`/`rerank_pairs`); `--rerank-model-dir M`
  (default `reference/models/ms-marco-MiniLM-L-6-v2`); `--load-path` applies to both models;
  `model-memory --model rerank --load-path buffered|mmap` measures the cross-encoder's
  fresh-process load per path (ADR-0009 condition 5). The 004 cache is reused by value as in 005
  (`embedded 0 documents`); every judged query is re-ranked at depth 20 with `k = 100`
  (Recall@100 cannot change, spec assumption; the report checks it is byte-equal per dataset).
- **Regression rule from here on** (FR-019): the re-ranked nDCG@10 not below
  `hybrid-baseline-v1` on ≥ 2 of 3 is the acceptance bar; a miss stops the feature for a
  report. On acceptance, `hybrid-rerank-v1` becomes the guarded pipeline number.

## D11. Observations to record (Story 6, SC-010), no budgets

FiQA: wall per query end-to-end and the re-rank part (`rerank_ms / queries`), per-pair
(`rerank_ms / rerank_pairs`), `rerank_pairs`, peak RSS of the process (`/usr/bin/time -l`),
index directory bytes now including `passages.bin` (the FR-010 cost, expected ~48 MB), and the
cross-encoder's fresh-process load peak per load path (`model-memory --model rerank
--load-path buffered|mmap`, three runs each, median), each with method. No `criterion` benchmark and no performance claim (Principle IV's budget
clause is N/A as in 004/005 — the spec states none).

## D12. Dependencies

`xtriever-rerank`: `candle-core`, `candle-nn`, `candle-transformers` **= 0.9.2** (the ADR-0001
pin, same comment block as the dense crate), `tokenizers 0.23.2` (`default-features = false,
features = ["fancy-regex"]`), `serde`, `serde_json`, `sha2`, `xtriever-core`, and `memmap2`
**optional** behind `[features] mmap = ["dep:memmap2"]` (D3) — every one already in
`Cargo.lock` at that version; added with `cargo add`, never typed. Dev: `serde_json`,
`tempfile`. `deny.toml` unchanged (the crate's graph equals the dense crate's). `xtriever-pipeline` gains `xtriever-rerank`? **No** — the pipeline takes a
`Box<dyn Reranker>` (core trait) and never names the concrete type; only the harness example
depends on `xtriever-rerank` (dev-dependency of `xtriever-eval`, like `xtriever-dense` and
`xtriever-pipeline` today). Direction stays `core ← {lexical, dense, rerank} ← pipeline ←
example`.

Python: the 004 pins unchanged (`reference/.venv-004`; `torch 2.14.0`, `transformers 5.17.0`,
`tokenizers 0.23.2`) — the oracle is the same stack scoring a different model, and a separate
venv would be a second copy of 3 GB of torch for nothing. `gen_006_fixtures.py` carries the
004 interpreter guard and thread pinning.

## D13. Test strategy: offline first, model-backed `#[ignore]`

- `xtriever-rerank` offline: `fixtures_valid` (manifest hashes), `model_pins` (compiled pins =
  manifest; identity string names every input), `budget` **with a stub scorer** — the budget
  loop is written over a `fn(&str, &str) -> Result<f32>` so item limits, a zero time limit and
  the output length are testable without the model. Model-backed (`#[ignore]`): `model_load`
  (verification errors name file and both values; config and header assertions), `score_golden`
  (SC-001: tolerance and per-query order, tokenization parity first), `score_determinism`
  (SC-002: three arrangements in-process, two thread counts cross-process), `budget_time`
  (SC-003: 40 max-length passages under a 100 ms limit ⇒ ≥ 1 and < 40 scored), `load_paths`
  (`cfg(feature = "mmap")`: buffered vs mapped scores bit-identical over the golden set —
  ADR-0009 condition 3).
- `xtriever-pipeline` offline, over the 005 fixture index with stub re-rankers (`TableReranker`
  by id with an `n` cut-off, `FailingReranker`, `WrongLengthReranker`, `NanReranker`, a
  `CapturingReranker` that records the `Budget` it received): `rerank_golden`
  (`pipeline_order.json`, D14), `rerank_prop` (ordering invariants), `rerank` (US3 scenarios,
  depth 0 / no re-ranker ⇒ byte-equal to 005 responses, `max(k, d)`), `degrade` extended
  (check point C, remaining time, strict), `explain` extended (US4), `passages` (store round
  trip, delete/replace, on-demand read after reopen, v1 directory refused naming both versions,
  torn store detected by count), `persist` extended (fifth count, commit order with the store).
  `model_roundtrip` (`#[ignore]`) attaches the real cross-encoder to the real embedder's index.
- `xtriever-eval`: `rerank_run` (config validation, `StageInfo`/`Observations` round trip with
  and without the new keys, `compare` between a hybrid and a rerank report).

## D14. Fixtures (`reference/gen_006_fixtures.py`)

- `rerank.json`: the model identity string (asserted equal to `MODEL_ID`), ~10 queries × 4–6
  hand-written passages including: a 600-word passage (truncated to 512 with the query), an
  empty passage, an empty query, both empty, and a **near-tie** query (two passages differing
  by one word). Each pair carries `input_ids`, `token_type_ids` and the `f32` logit
  (`float_roundtrip`-safe: written with `repr`); each query carries the reference order. The
  generator **refuses to emit** a query whose smallest score gap is below `10 × 1e-3`: SC-001's
  "exact order" must be a fair test at the tolerance, so the near-tie is near (gap in
  `[0.01, 1.0]`, asserted), never within tolerance — the 004 tie discipline in a new form.
- `pipeline_order.json`: ~10 named cases of `(fused ids, stub scores with nulls, d, k)` →
  expected `(id, rerank_score?)` order under D8's rule, computed by an independent Python
  implementation (ties, `d > k`, `d < k`, `m < d`, none scored, `d ≥ len`, `d = 0`).
- `manifest.json` with SHA-256 of both, checked by `fixtures_valid` in both crates.

## Risks

| Risk | Mitigation |
|---|---|
| Bit-identity across thread counts fails for some sequence length | Finding, not a fix (Rule 6); reported per pair; identity string decision deferred to the human |
| candle's per-pair cost is worse than 100 ms at 512 tokens | Depth stays 20; the number is recorded; the on-device feature sets its budget from it |
| The re-ranked nDCG@10 is below the fused baseline on ≥ 2 datasets | Stop and report (FR-019); candidates: depth, truncation, a head defect (parity test isolates it) |
| `passages.bin` rewrite per commit is slow for incremental use | Same class as the dense `index.bin`; measured once on FiQA; a segmented store is a later feature |
| Buffered vs mapped scores differ by a bit | A defect in the mapped path (ADR-0009 condition 3): fixed or the path dropped, never accommodated |
