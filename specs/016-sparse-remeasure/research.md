# Research: Sparse Stage Re-measurement

**Feature**: `016-sparse-remeasure` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## D1 — The inputs, all on disk, and which list plays which role

| role | artefact | shape |
|---|---|---|
| lexical (v2) and dense candidate lists, **as the engine saw them** | `target/xt-rerank-study/<d>/explain-d50.jsonl` (014), keys `lexical` / `dense`: `[[rank, id], …]`, 100 each | the pipeline's own candidate lists (a 2×depth search exposes every candidate's stage rank, `examples/beir.rs` explain export) |
| the engine's fused order and fused scores | same file, `fused` / `fused_scores` (100) | the oracle for the fusion code (FR-002) |
| the engine's cross-encoder scores for the v2 fused top-50 | same file, `rerank`: `[[rank, id, score]]`, 50 per query (15,000 / 16,150 / 32,400 pairs) | f32 logits from the 014 depth-50 runs |
| sparse dot list (doc-v3, unquantised) | `target/xt-sparse-runs/<d>/dot@opensearch-neural-sparse-encoding-doc-v3-distill@babf71f3.jsonl` (012), top-100 per query | the list a sparse stage would contribute at candidate depth 100 |
| anchors | `specs/013-lexical-quality/baselines/hybrid-baseline-v2.<d>.json`, `specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.<d>.json`, `target/xt-rr3-run.<d>.jsonl` (the v3 lists), 012's `summary.json` (`rrf-lex+dense+dot` 0.7139 / 0.3508 / 0.3881) | per-query metrics and lists |
| passages for the reference scorer | `reference/datasets/beir/<d>/corpus.jsonl`, joined as the pipeline's passage (`title + " " + text`, empty side omitted — 013's `join_title_text`) | what the engine's re-ranker saw |
| the tie key | corpus position (`DocId(i)` in the harness) | `rerank_study.corpus_positions` |

**Decision**: fuse from the explain exports' `lexical` / `dense` lists rather than from the
separately exported runs — they are the very lists the engine fused, so the reproduction of
`hybrid-baseline-v2` is exact by construction, and any residual difference is a defect in the
fusion code, not in the inputs. The 013 `target/xt-lex2-run.<d>.jsonl` runs are equal to the
explain's `lexical` lists (both are the engine's top-100) — checked once, stated.

## D2 — The fusion: the engine's rule, re-implemented in Python with the engine's ties

`xtriever_pipeline::fusion::fused_terms` (`src/fusion.rs:23–45`): for each list, term
`1/(rrf_k + rank)` with a 1-based rank, `rrf_k` 60, summed per document over the lists it
appears in; sorted by `(−sum, DocId)`. The 012 spike's `rrf` (`reference/sparse_spike.py:545`)
sorts ties by the external id string — not the engine's rule. **Decision**: a new
`fuse(lists, positions, k=60)` in `reference/sparse_remeasure.py` with ties by corpus position
and the fused score returned (the interpolation needs it); FR-002's reproduction check makes
the tie rule executable. Three-way fusion adds the sparse list as a third term — the engine has
no third stage, so this *is* the design a sparse stage would implement (012 D5: "so the
engine's three-way fusion is predicted from the engine's own two runs").

## D3 — The re-ranking: 014's derivation, reused

`reference/rerank_study.py` already implements the interpolating rule exactly as the engine
does (`head`, `rest`, `order_lin`, `minmax`; verified list for list against the engine in 015).
**Decision**: import it. The head of a fused list is its first 20 candidates with a
cross-encoder score; scores come from the explain `rerank` map where the pair is covered and
from the reference where not (D4). FR-003's reproduction — re-ranking the v2 fused list with
explain scores equals `target/xt-rr3-run.<d>.jsonl` — reuses `rerank_study.compare_runs`.

## D4 — The gaps: the 006 reference cross-encoder, used sparingly and measured

A three-way top-20 candidate outside the v2 fused top-50 has no engine score. The 006
reference (`reference/gen_006_fixtures.py` `Reference.score(query, passage)` — the pinned
`ms-marco-MiniLM-L-6-v2` through `transformers`, `truncation=True, max_length=512`, the head
checked against the pipeline's manual composition) was verified against the engine at
`TOLERANCE_ABS = 1e-3` on the 006 fixture pairs. **Decision**: score only the uncovered pairs
with it (count and share reported per dataset), and measure its agreement with the engine on
this corpus by re-scoring a fixed sample of covered pairs (the first 200 covered pairs per
dataset in query order) — maximum and mean absolute difference reported; a maximum above the
006 tolerance is a finding, not a stop (the engine's score is used wherever both exist).
Environment: `reference/.venv-012` holds `torch 2.14.0` and `transformers 5.17.0` (012); CPU
or MPS, whichever the 006 reference picks (it uses the default device); the reference's
passage is the pipeline's passage text (D1). Cost: bounded by 20 × queries pairs (≤ 6,000 /
6,460 / 12,960) if nothing were covered; the expected share is small (the sparse list mostly
re-orders documents the two current lists already retrieve — 012 measured 0.35–0.7 top-10
Jaccard between the lists); ≈ 50–150 ms per pair on this machine → minutes to an hour.

## D5 — Variants and cells

`rrf(lex2, dense)` (the anchor, must reproduce v2), `rrf(lex2, dense, dot)`, `rrf(dense, dot)`,
`rrf(lex2, dot)`; each un-re-ranked and re-ranked (`lin-0.5`, depth 20). Cells under
`specs/016-sparse-remeasure/runs/<variant>[-rr].<d>.json` in the 014 cell shape plus, for
re-ranked cells, `reference_scored_pairs`, `reference_share`, and per dataset the agreement
figures; `table.md` / `table.json`; `decision.json`. Runs under `target/xt-sparse-remeasure/`.
Scoring by `gen_003_fixtures.reference` (imported; `probe()` first).

## D6 — The decision rule, restated for the script

`qualifies = mean(rrf(lex2, dense, dot)-rr) ≥ 0.4913 + 0.005 and ∀ d: ndcg(d) ≥
ndcg(hybrid-rerank-v3, d) − 0.005`. One row decides; the other rows are findings. Outcome
text: "specify the sparse stage (expected gain +X mean)" or "012's GO withdrawn: <cells>";
reopening conditions listed in the report regardless.

## D7 — Tests first

`reference/tests_016/`: `fuse` against hand cases including a tie broken by position (not by
id string), a document in one list only, three lists; `head_scores` (explain-covered vs
uncovered split) on a synthetic explain line; the decision rule's floor / drop / none cases; a
synthetic end-to-end: two lists + a synthetic sparse list → fused → re-ranked with a stub
scorer → runs written in the harness shape. The two reproduction checks (FR-002 / FR-003) run
on the real artefacts in the quickstart, not in pytest.

## D8 — Not done

No new encoding (the 012 caches are reused only through their dot runs); no quantised
variant; no `bm25x`; no Rust; no change to any baseline; no device measurement.
