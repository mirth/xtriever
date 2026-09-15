# ADR-0011: `hybrid-rerank-v2` lands with its SciFact drop on record

- **Status**: Accepted — 2026-09-16
- **Date**: 2026-09-16
- **Deciders**: mirth (repository owner), 2026-09-16 (option A of three)
- **Spec**: [013-lexical-quality](../../specs/013-lexical-quality/spec.md) — SC-002, report
  "Fused baselines"
- **Trigger**: Principle II / Agent Operating Rule 6 — a stage's baseline mean falls; the
  constitution asks for human review and an ADR before a ranking regression lands

## Context

Feature 013 replaces the evaluation's lexical layout (`title` × 2.0 beside `text`) with one
joined `contents` field and re-baselines every configuration that fuses the lexical list.
The lexical and hybrid stages improve on both titled sets and are unchanged on FiQA (which
has no titles). The re-ranked configuration does not, on one dataset (nDCG@10):

| configuration | SciFact | NFCorpus | FiQA | three-set mean |
|---|---|---|---|---|
| lexical v1 → v2 | 0.6270 → 0.6856 | 0.3115 → 0.3227 | 0.2502 → 0.2502 | 0.3963 → 0.4195 |
| hybrid v1 → v2 | 0.6897 → 0.7144 | 0.3450 → 0.3535 | 0.3692 → 0.3692 | 0.4680 → 0.4790 |
| hybrid-rerank v1 → v2 | 0.7039 → **0.6954** | 0.3603 → 0.3609 | 0.3742 → 0.3742 | 0.4795 → **0.4768** |

SC-002 required both fused means to be ≥ their v1 means; the re-rank mean is 0.0026 below,
all of it SciFact. The cause is not the field change: the re-ranker's *input* on SciFact is
2.5 points better (0.7144 vs 0.6897, Recall@100 0.955 vs 0.942), and re-ranking it at depth
20 with the pinned ms-marco MiniLM-L-6 cross-encoder produces a list 1.9 points below that
input. In v1 the same re-ranker added 1.4 points to a weaker list. On SciFact the cross-encoder
orders the top-20 worse than RRF over the joined-field BM25 and dense lists does; the more RRF
gets right, the more re-ranking has to lose. NFCorpus (+0.0006) and FiQA (±0) are unaffected.

## Decision

Land `hybrid-rerank-v2` with its measured baselines, SC-002 recorded as **failed for the
re-rank configuration** in the feature's report, and this ADR as the human review the
constitution asks for. Rationale:

- The regression is confined to the re-rank stage on one dataset and is a property of the
  pinned re-ranker's fit to scientific-claim queries, surfaced by a better first stage — not
  something Feature 013 introduced or can fix within its scope (no analyzer, parameter or
  model change; spec FR-006).
- Omitting the configuration would hide the finding rather than the regression: 014 (the
  sparse stage) fuses into the same pipeline and needs the v2 re-rank baseline to start from.
- Nothing shipped changes: the v1 configurations and baselines stay committed and runnable;
  the engine, the Wikipedia index and the FFI are untouched.
- The best known SciFact configuration is now `hybrid-baseline-v2` (0.7144), un-re-ranked —
  above every v1 number including the re-ranked one.

Nothing was tuned to pass: no threshold, tolerance or depth moved (Rule 6).

## Consequences

- `specs/013-lexical-quality/baselines/hybrid-rerank-v2.{scifact,nfcorpus,fiqa}.json` are
  the re-rank baselines from here on; SC-002 is met for hybrid and failed for re-rank, stated
  in the report and the PR.
- A follow-up spec owns the re-ranker question: a depth sweep on SciFact (the re-ranker may
  only harm the head), a domain-fit check of the pinned cross-encoder on scientific-claim
  queries, or making re-ranking conditional. Until it runs, the pipeline's default re-rank
  depth stays 20 (006) and no caller-facing behaviour changes.
- Revisit when: the re-ranker model is changed or re-pinned; the re-rank depth is made
  configurable per query; or 014's sparse list changes the fused input again.
