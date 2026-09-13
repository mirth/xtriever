# ADR-0010: The on-device RSS ceiling becomes 600 MB for the full pipeline

- **Status**: Accepted — 2026-09-13 (constitution amended to v1.4.0)
- **Date**: 2026-09-13
- **Deciders**: mirth (repository owner), 2026-09-13
- **Spec**: [007-ffi-surface](../../specs/007-ffi-surface/spec.md) — report finding F-002
- **Amends**: Principle III, "Memory is budgeted" (default ceiling 300 MB → 600 MB, reference
  configuration spelled out)

## Context

Principle III at v1.3.0: "On-device configurations MUST stay under the RSS ceiling stated in
their spec (default: 300 MB for a 100k-chunk index including loaded models)." The 300 MB was set
at ratification, before any model had run on a phone, and Feature 001's spike measured only one
model over a synthetic corpus. Feature 007 is the first measurement of the **full pipeline** —
BM25 → dense → fusion → cross-encoder re-rank — on a physical device (iPhone 16e, iOS 26.6.2,
Release, both models memory-mapped, SciFact's 5,183 documents), and it fails the ceiling on all
three runs, recorded verbatim under `specs/007-ffi-surface/runs/`:

| run | load path | after `open` | peak (kernel ledger) | vs 300 MB |
|---|---|---|---|---|
| mmap, 1 thread | mapped | 262.0 MB | 376.4 MB | FAIL |
| buffered, 1 thread | buffered | 361.3 MB | 383.0 MB | FAIL |
| mmap, default threads | mapped | 259.9 MB | 372.7 MB | FAIL |

Where the memory is (007 report F-002): two `F32` MiniLM models resident (2 × ~87 MB of tensors
— candle 0.9.2 copies every tensor onto the heap whichever loader is used, so mapping saves the
load-time file buffer, ~100 MB at open on iOS, and nothing at steady state), two tokenizers, the
index, and an **80–115 MB transient** during re-ranking (attention over 512-token pairs). The
5k-document SciFact index is small; the constitution's reference configuration is **100k
chunks**, whose dense vectors alone (100,000 × 384 × 4 B ≈ 154 MB) are all touched by the exact
flat scan. The honest full-pipeline number for the reference configuration is therefore about
550–600 MB, not 300.

Rule 6 forbids fixing a failed target by weakening the threshold, so 007 stopped and reported.
The levers that would bring the pipeline under 300 MB were listed, each a feature of its own:
drop the re-ranker on device; `F16`/int8 weights (an ADR: fingerprints, goldens and tolerances
change); 256-token re-rank input (a measured quality cost); loading the re-ranker per query.

## Decision

The owner chose to change the ceiling rather than the pipeline. Principle III's default becomes:

> default: **600 MB** for the full pipeline — a 100k-chunk hybrid index with the embedder and
> the cross-encoder re-ranker loaded (ADR-0010)

This is an amendment through Governance (PR touching the constitution, this ADR, human
approval, version bump), not a threshold edit. It is **MINOR** (1.3.0 → 1.4.0): the principle —
memory is budgeted, every on-device configuration states and meets a ceiling — is unchanged; its
default is re-derived from the first real measurement and its reference configuration is made
explicit, as v1.3.0 did for Principle VII.

Why 600 MB and not another number:

- 383 MB measured on 5k documents + ~154 MB of vectors at 100k chunks + the passage store and
  lexical working set ≈ 550 MB; 600 leaves a margin smaller than one model.
- 512 MB would pass today's SciFact runs and predictably fail again the moment the 100k-chunk
  Wikipedia index exists — a ceiling set to be missed is not a budget.
- Tiered ceilings (300 MB embedder-only / 600 MB with re-ranker) were considered: they keep the
  old number alive, but the shipped configuration is the full pipeline and no spec targets the
  embedder-only tier; a second number that nothing measures against is noise.
- No numeric default at all removes the guardrail; the point of the default is that a spec that
  says nothing is still bound.
- Device headroom: the 16e's observed per-app limit was 3.54 GB; 4 GB iPhones allow roughly
  2 GB in the foreground. 600 MB is a comfortable fraction on every device the demo targets.

## Consequences

- The three 007 run records keep `ceilingBytes: 300000000` and `verdict: FAIL` exactly as
  recorded — they are evidence, not a scoreboard. Under v1.4.0 the same peaks (372–383 MB) are
  a PASS; the 007 report says so beside the original verdict.
- `DeviceMeasurementTests.ceilingBytes` becomes 600 MB, citing this ADR. Every future device run
  is judged against 600 MB unless its spec states a tighter one.
- The reference configuration is now explicit: a spec that measures a smaller index or one
  model MUST say so and MUST NOT claim the ceiling is met for the reference configuration.
- The levers stay on the table as optimisations, not obligations. Any of them that lands is
  measured against the same harness and recorded; none is required by this decision.
- Review trigger: when the 100k-chunk index exists (the Wikipedia corpus feature), its device
  run either confirms 600 MB or produces the first honest number for the reference
  configuration — in which case this ADR is revisited with that number, not extrapolated
  from SciFact again.

## Alternatives considered

| Alternative | Why not |
|---|---|
| Keep 300 MB, ship without the re-ranker on device | Removes the pipeline's last stage from the one product that demonstrates it; the demo would show a different engine from the one measured on the laptop |
| Keep 300 MB, `F16`/int8 weights | Halves or quarters 174 MB of tensors but changes the pinned artefacts, fingerprints, goldens and tolerances — a Principle I measurement plus an ADR; still leaves the 100k-chunk index over 300 MB once vectors are counted |
| Keep 300 MB, load the re-ranker per query | ~180 ms per query and 90 MB of churn per search; hides the cost in latency instead of memory |
| 512 MB | Passes SciFact, fails the reference configuration by construction |
| Tiered 300 / 600 MB | A second number no spec measures against |
| No numeric default | Loses the guardrail for specs that say nothing |
