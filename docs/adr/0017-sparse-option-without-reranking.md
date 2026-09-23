# ADR-0017: The sparse option lands with its cost without re-ranking on record

- **Status**: Accepted — 2026-09-23 (the repository owner chose option A of three: accept and
  document)
- **Date**: 2026-09-23
- **Deciders**: mirth (repository owner)
- **Spec**: [027-sparse-lexical-expansion](../../specs/027-sparse-lexical-expansion/spec.md) —
  SC-001, SC-002, the clarification of 2026-09-23
- **Trigger**: Principle II / Agent Operating Rule 6 — a configuration of the engine ranks
  below its baseline, and the constitution asks for human review and an ADR before a ranking
  regression lands (the precedent is [ADR-0011](0011-rerank-v2-scifact-regression.md))
- **Related**: [ADR-0016](0016-sparse-expansion-field.md) (the option's format)

## Context

Feature 027 adds sparse lexical expansion as an **opt-in** per-index option (off by default,
so the default pipeline is unchanged). Its success criteria are defined on the **full
pipeline**, fused and re-ranked (`hybrid-sparse-rerank-v1` against `hybrid-rerank-v3`), and it
meets them (`specs/027-sparse-lexical-expansion/runs/`):

| nDCG@10 | SciFact | NFCorpus | FiQA |
|---|---|---|---|
| `hybrid-rerank-v3` | 0.72194 | 0.36247 | 0.38964 |
| `hybrid-sparse-rerank-v1` | 0.72166 (−0.0003) | 0.35767 (−0.0048) | 0.40680 (**+0.0172**) |

SC-001 (FiQA ≥ +0.010) and SC-002 (no dataset below −0.005 on nDCG@10 or Recall@100) pass;
Recall@100 does not fall anywhere.

**Without the re-ranker** the same option ranks below the fused baseline by more than SC-002's
margin on two datasets:

| nDCG@10 | SciFact | NFCorpus | FiQA |
|---|---|---|---|
| `hybrid-baseline-v2` | 0.71542 | 0.35359 | 0.37006 |
| `hybrid-sparse-v1` | 0.70846 (**−0.0070**) | 0.34803 (**−0.0056**) | 0.40088 (+0.0308) |

Recall@100 is unchanged on SciFact and rises on NFCorpus (+0.0007) and FiQA (+0.0073): the
expansion brings relevant documents into the candidate list and, on corpora whose documents
already share their questions' words, reorders the top of the fused list slightly for the
worse; the cross-encoder then recovers the order. An installation that switches the option on
and does not re-rank — a device that skips the re-ranker to save time — would see the drop.

## Options considered

- **A — Accept and document.** The option ships as specified. Its documentation (the pipeline
  crate, the command line's `--sparse-encoder`, the bindings), the report and the pull request
  say to pair it with the re-ranker and to use it for FiQA-shaped corpora: no titles, questions
  worded differently from their answers.
- **B — Require the re-ranker.** A sparse index refuses, or warns on, searches without
  re-ranking. A larger change, and it removes the option from installations that skip
  re-ranking even on the corpora where it helps most (FiQA gains +0.031 without it).
- **C — Treat the drop as a failure** and stop the feature under Rule 6.

## Decision

**A.** The drop is a property of an opt-in configuration, not of the default pipeline; it is
on record here with the numbers, and the option's documentation carries the advice.

## Consequences

- The default pipeline and every committed option-off baseline are unaffected
  (`hybrid-rerank-v3` reproduced in every per-query score).
- Every surface that exposes the option says, where it is described: pair it with the
  re-ranker; it is for FiQA-shaped corpora.
- **Review trigger**: revisit if a later measurement puts the option's full-pipeline delta on
  any dataset below −0.005, if the default re-ranker changes, or if a corpus the engine ships
  with (the Simple English Wikipedia artefact) is ever built with the option.
