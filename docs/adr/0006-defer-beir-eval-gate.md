# ADR-0006: Defer the BEIR eval gate until `xtriever-eval` exists; Feature 002 is the baseline

- **Status**: Accepted — 2026-09-12
- **Date**: 2026-09-11
- **Deciders**: mirth (repository owner), 2026-09-12
- **Spec**: [002-lexical-stage](../../specs/002-lexical-stage/spec.md)
- **Relates to**: constitution Principle II (ranking quality), Quality Gates table row "eval smoke"

## Context

Principle II states: *"Ranking quality MUST be measured with `xtriever-eval` on the fixed benchmark
set (BEIR SciFact, NFCorpus, FiQA) and reported as nDCG@10 / Recall@100 deltas in the PR
description."* The Quality Gates table lists *"eval smoke (SciFact) on changes to
`lexical`/`dense`/`rerank`/`ltr`/`pipeline`"* as **blocking**.

Feature 002 is the first change to `xtriever-lexical` that affects ranking — it *creates* ranking.
Three facts make the clause unsatisfiable as written:

1. `crates/xtriever-eval` is a placeholder: one `lib.rs`, no BEIR loader, no nDCG, no qrels parsing
   (`Cargo.toml` description: *"placeholder until its spec lands"*).
2. There is no baseline. A *delta* needs two measurements; before this feature there is no lexical
   stage to measure.
3. CI has no eval job (`.github/workflows/ci.yml` contains no eval or criterion step), so the
   "blocking" row is nominal today.

Feature 001 marked the clause N/A on the grounds that a build spike changes no ranking. That
reasoning does not transfer: this feature is exactly the kind of change the clause targets.

## Decision

The eval clause is **deferred, not waived**, under three conditions:

1. **Feature 002 records the baseline artefact instead of the delta.** Its PR descriptions carry the
   line *"eval delta: N/A — ADR-0006"*, and the feature's `report.md` records the commit hash that a
   future `xtriever-eval` run must use as the baseline for the first real delta.
2. **The gate is restored by the `xtriever-eval` spec, not by a later lexical change.** The eval
   spec's first task is to run SciFact against the commit named in (1) and record absolute nDCG@10 /
   Recall@100. From that point the clause applies to every ranking-affecting PR as written.
3. **No ranking-affecting change after Feature 002 may cite this ADR.** It covers exactly one
   feature. A second lexical/dense/rerank/ltr/pipeline change landing before `xtriever-eval` exists
   needs its own decision.

Building a minimal SciFact smoke inside Feature 002 was considered and rejected: it is the eval
spec's work (BEIR download and pinning, qrels, nDCG implementation, its own oracles), and folding it
in would roughly double this feature's scope while producing an unreviewed harness.

## Consequences

**Positive**: the first lexical stage lands with its correctness oracles intact (BM25 parity, exact
goldens, property tests) and without a hand-rolled evaluation nobody has reviewed. The deferral is
visible in every PR description rather than silent.

**Negative**: for the window between Feature 002 and the eval spec, a lexical ranking regression
would be caught only by the golden fixtures, which test *agreement with tantivy*, not *retrieval
quality*. That window is bounded by condition 2 and should be short.

**Review triggers**: the `xtriever-eval` spec lands (retire this ADR); any ranking-affecting PR
attempts to cite it (condition 3 — refuse).

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Mark N/A as Feature 001 did | Dishonest: this feature creates ranking behaviour, which is the clause's target. |
| Build a SciFact smoke in 002 | Doubles scope; produces an eval harness with no spec, no oracle and no review. |
| Block Feature 002 until the eval spec lands | Inverts the dependency — the eval spec needs a stage to evaluate. |
| Amend the constitution to weaken the clause | The clause is right; only its timing is impossible. A one-feature deferral is narrower than a permanent wording change. |
