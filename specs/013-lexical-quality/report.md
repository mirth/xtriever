# Report: Lexical Quality — One Field for BM25

**Feature**: `013-lexical-quality` | **Date**: 2026-09-16 | **Status**: done — SC-001 met, SC-002 met for hybrid and **failed for re-rank** ([ADR-0011](../../docs/adr/0011-rerank-v2-scifact-regression.md))

## Verdict

One configuration change — the evaluation's BM25 indexes one joined `contents` field instead
of `title` × 2.0 beside `text` — lifts the lexical baseline by 5.9 / 1.1 nDCG@10 points on
SciFact / NFCorpus (FiQA, no titles, unchanged) and the hybrid baseline by 2.5 / 0.9. The
re-ranked configuration gains 0.06 on NFCorpus and loses 0.84 on SciFact, where the pinned
cross-encoder now orders the top-20 worse than the fused list it receives; the three-set
re-rank mean is 0.0026 below v1, reported and landed under ADR-0011. Nothing in the engine
changed: `xtriever-eval` gained a `Source` variant and three constructors, the example's
dense fields follow the schema, the CI smoke moved to the v2 lexical baseline.

## Attribution (research D1, the 012 spike's BM25, nDCG@10)

| layout | SciFact | NFCorpus | FiQA |
|---|---|---|---|
| **engine** (`lexical-baseline-v1`: `title` × 2.0 + `text`, tantivy `en_stem`) | 0.6270 | 0.3115 | 0.2502 |
| spike, engine shape (`title` × 2.0 + `text`) | 0.6207 | 0.3115 | 0.2473 |
| spike, `title` + `text` (boost 1) | 0.6646 | 0.3247 | 0.2473 |
| spike, `text` only | 0.6752 | 0.3176 | 0.2473 |
| **spike, one joined field** (`title + " " + text`) | **0.6867** | **0.3228** | 0.2473 |
| joined + `title` × 0.5 | 0.6854 | 0.3255 | — |
| joined + `title` × 1.0 | 0.6632 | 0.3197 | — |
| joined, k1 0.9 / b 0.4 | 0.6823 | 0.3224 | 0.2340 |
| joined + Lucene stop words | 0.6826 | 0.3150 | 0.2405 |
| joined + stop words + k1 0.9 / b 0.4 | 0.6778 | 0.3139 | 0.2293 |
| **engine, `lexical-baseline-v2`** (one joined field, this feature) | **0.6856** | **0.3227** | 0.2502 |

The engine-shape replica reproduces the engine within 0.6 / 0.0 / 0.3 points, so the
tokenizer and stemmer are not the gap; the joined field is. The engine's v2 lands within
0.1 / 0.0 / 0.3 of the spike's prediction.

## Red checkpoint (Rule 4, T004)

`cargo nextest run -p xtriever-eval -E 'test(v2) | test(title_and_text)'` does not compile:

```text
error[E0599]: no variant or associated item named `TitleAndText` found for enum `Source`
error[E0599]: no function or associated item named `lexical_baseline_v2` found for struct `EvalConfig`   (×4)
error[E0599]: no function or associated item named `hybrid_baseline_v2` found for struct `HybridConfig`
error[E0599]: no function or associated item named `hybrid_rerank_v2` found for struct `RerankConfig`
```

The four tests: `v2_config_is_v1_with_one_joined_field`, `v2_hybrid_and_rerank_wrap_the_v2_lexical`,
`v1_is_unchanged` (`tests/run.rs`); `title_and_text_joins_with_one_space_and_omits_empty_sides`
(`tests/hybrid_run.rs`). Before any change, `lexical-baseline-v1` on SciFact reproduced its
committed baseline exactly (nDCG@10 0.627044, Recall@100 0.887556; T001).

## Lexical baselines (T007, SC-001)

`lexical-baseline-v2` on the three sets, each run verified by the 003 reference scorer
(`--verify-run … PASS`, 1e-6), `beir compare` against `specs/003-eval-harness/baselines/lexical-baseline-v1.<d>.json`:

| dataset | metric | v1 | v2 | abs | rel | SC-001 floor |
|---|---|---|---|---|---|---|
| scifact | nDCG@10 | 0.627044 | 0.685602 | **+0.058559** | +9.34% | ≥ +0.05 ✓ |
| scifact | Recall@100 | 0.887556 | 0.921333 | +0.033778 | +3.81% | — |
| nfcorpus | nDCG@10 | 0.311523 | 0.322688 | **+0.011165** | +3.58% | ≥ +0.008 ✓ |
| nfcorpus | Recall@100 | 0.247820 | 0.247331 | −0.000489 | −0.20% | — |
| fiqa | nDCG@10 | 0.250238 | 0.250238 | +0.000000 | +0.00% | ±0.001 ✓ |
| fiqa | Recall@100 | 0.551775 | 0.551775 | +0.000000 | +0.00% | — |

The engine's joined field lands on the spike's prediction (0.6867 / 0.3228 / 0.2473 in the
spike's BM25; 0.6856 / 0.3227 / 0.2502 in the engine — within 0.1 / 0.0 / 0.3 points, the
tokenizer's share). FiQA, which has no titles, is bit-identical to v1: the one field is the
`text` field under another name. NFCorpus Recall@100 slips by 0.0005 (one document in one
query's tail); nDCG@10 is the target metric and gains 1.1 points.

## Fused baselines (T008–T009, SC-002)

Every run verified by the 003 reference scorer (PASS, 1e-6). `dense-baseline-v1` on SciFact
re-run after the `dense_fields` change equals the 004 baseline (0.645082 / 0.925000): the
dense list is untouched. nDCG@10 / Recall@100:

| configuration | dataset | v1 | v2 | Δ nDCG@10 | Δ Recall@100 |
|---|---|---|---|---|---|
| hybrid-baseline | scifact | 0.689727 / 0.941667 | 0.714369 / 0.955000 | **+0.024642** | +0.013333 |
| hybrid-baseline | nfcorpus | 0.345008 / 0.320720 | 0.353510 / 0.321648 | **+0.008501** | +0.000929 |
| hybrid-baseline | fiqa | 0.369210 / 0.707111 | 0.369210 / 0.707111 | +0.000000 | +0.000000 |
| hybrid-baseline | **mean** | 0.467982 | 0.479030 | **+0.011048** | — |
| hybrid-rerank | scifact | 0.703862 / 0.941667 | 0.695430 / 0.955000 | **−0.008431** | +0.013333 |
| hybrid-rerank | nfcorpus | 0.360287 / 0.320720 | 0.360854 / 0.321648 | +0.000567 | +0.000929 |
| hybrid-rerank | fiqa | 0.374214 / 0.707111 | 0.374214 / 0.707111 | +0.000000 | +0.000000 |
| hybrid-rerank | **mean** | 0.479454 | 0.476833 | **−0.002622** | — |

**SC-002 passes for `hybrid-baseline-v2` and fails for `hybrid-rerank-v2`** (mean −0.0026,
all of it SciFact). ⛔ Rule 6 stop-point — reported, not adjusted.

### The SciFact re-rank drop is the re-ranker, not the field change

On SciFact the fused list improves by 2.5 points (0.6897 → 0.7144) and its Recall@100 by
1.3, yet re-ranking the better list at depth 20 lands *below* re-ranking the worse one
(0.6954 vs 0.7039) — and 1.9 points below its own input (0.7144). In v1 the cross-encoder
added 1.4 points to the fused list; in v2 it removes 1.9. The ms-marco MiniLM-L-6 cross-encoder
orders SciFact's top-20 worse than RRF over the joined-field BM25 and dense lists does, so
the more RRF gets right, the more the re-ranker has to lose. NFCorpus and FiQA are unaffected
(+0.0006, ±0). This is a 006 finding surfaced by a stronger candidate list: the re-ranker's
value on scientific-claim queries is negative once the first stage is good enough, and the
best known SciFact configuration is now `hybrid-baseline-v2` (0.7144), not a re-ranked one.

## Success criteria

| criterion | result |
|---|---|
| SC-001 lexical v2 ≥ v1 + 0.05 / + 0.008 / ± 0.001 | **PASS** — +0.0586 / +0.0112 / +0.0000 |
| SC-002 fused means ≥ v1 | hybrid **PASS** (0.4680 → 0.4790); re-rank **FAIL** (0.4795 → 0.4768; SciFact −0.0084, NFCorpus +0.0006, FiQA ±0) — ADR-0011 |
| SC-003 every v2 baseline verifies to 1e-6 | **PASS** — nine `--verify-run` PASS |
| SC-004 v1 baselines byte-identical, v1 reproduces | **PASS** — `git diff main -- specs/00{3,4,5,6}-*` empty; `lexical-baseline-v1` SciFact 0.627044 / 0.887556 before and after; `dense-baseline-v1` SciFact 0.645082 / 0.925000 after the `dense_fields` change |
| SC-005 CI smoke passes on v2 | local `beir smoke --config lexical-baseline-v2` → `eval-smoke: PASS`; CI on push |
| SC-006 no executable change under `crates/` outside `xtriever-eval`; the FR-008 doc comment in `chunking.rs` permitted | **PASS** — `git diff --stat main -- crates/ ':!crates/xtriever-eval' ':!crates/xtriever-cli/src/wiki/chunking.rs'` empty; the `chunking.rs` diff is 8 `///` lines, no statement. (The criterion originally read "no file under `crates/` other than `xtriever-eval`", which FR-008's CLI guidance made unachievable; narrowed in review, not to pass a number.) |

## Findings

- **F-001 — The re-ranker loses on SciFact once the first stage is good** (above; ADR-0011).
  `hybrid-baseline-v2` 0.7144 → `hybrid-rerank-v2` 0.6954. **Studied in 014**
  (`specs/014-rerank-depth-study/report.md`): the loss is replace-order re-ranking, not depth —
  every replace depth loses on SciFact; interpolating the fused and cross-encoder scores
  (α 0.5, depth 20) scores 0.7207 / 0.3622 / 0.3910, mean 0.4913 vs 0.4768, and is the
  decided follow-up default — **resolved by 015** (ADR-0012): the default; `hybrid-rerank-v3`
  SciFact 0.7207, above the un-re-ranked 0.7144.
- **F-002 — Most of the sparse stage's predicted gain on the titled sets was the layout.**
  012's three-way fusion `rrf-lex+dense+dot` predicted 0.714 / 0.351 / 0.388; the v2 hybrid
  alone reaches 0.714 / 0.354 / 0.369. FiQA (+1.9 points) is where the sparse expansions still
  add; 014's expected gain should be re-stated from the v2 baselines (mean 0.479, not 0.468).
- **F-003 — The tokenizer's share is ≤ 0.3 points.** The engine's v2 differs from the spike's
  one-field BM25 by 0.1 / 0.0 / 0.3 (tantivy `en_stem` vs split-on-non-alphanumerics +
  Snowball); no analyzer work is warranted by these sets.
- **F-004 — NFCorpus Recall@100 slips 0.0005** (0.247820 → 0.247331) while nDCG@10 gains 1.1
  points: one relevant document leaves one query's top-100 tail. Stated, not acted on.

## Review round 1 (Copilot, six comments, all taken)

1. The `TitleAndText` = dense-passage invariant was tested only where the text is non-empty
   because `build_passages` kept a trailing separator for a title with an empty text. Now one
   `join_title_text` serves both builders and the test asserts every document (both present,
   empty title, empty text, both empty). No corpus document is title-only (0 / 0 / 0), so no
   passage, cache entry or baseline changes; the 004 tests still pass unchanged.
2. Plan Principle III was **N/A** although `xtriever-eval` is a named pure crate and changes:
   now **PASS**, citing no added dependency and the passing target checks.
3. – 6. SC-006 ("no file under `crates/` other than `xtriever-eval`") contradicted FR-008 (the
   CLI guidance lives in `chunking.rs`'s doc comment) and the gate command would never have
   been empty: SC-006 is narrowed to executable changes with that one comment explicitly
   permitted, and the gate (quickstart, T013, plan rule 2) excludes exactly that path and
   checks its diff is `///` lines only — both re-run, both empty.

## Deliberately not done

- BM25 parameters (k1 0.9 / b 0.4: −0.4 / −0.0 / −1.3 points), a stop-word list (−0.4 / −0.8 /
  −0.7), an extra title field beside the joined one (±0.3, opposite signs) — measured, lost.
- The Wikipedia index rebuild (`title` 2.0 / `text` 1.0 stays): hours plus a device record; its
  retrieval quality has no oracle; 014 re-encodes the corpus and decides the schema.
- The 007 / 009 / 011 fixture goldens: boundary parity on the 005 fixture's own schema.
- Any re-rank depth or model change (F-001): a separate spec.

## Gate (Rule 5)

fmt ✓ · clippy workspace (host, `x86_64-pc-windows-msvc`) ✓ · nextest workspace ✓ · deny ✓ ·
iOS / iOS-sim / Android checks ✓ · wasm32 best-effort (tracked `getrandom` failure, unchanged) ·
no-stubs ✓ · SC-006 gate (paths excluded: `xtriever-eval`, the `chunking.rs` comment) empty, comment diff `///`-only ✓ · smoke v2 ✓ · no device / team identifiers in the tree ✓.
