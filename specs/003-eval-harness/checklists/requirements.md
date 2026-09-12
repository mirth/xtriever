# Specification Quality Checklist: The Evaluation Harness

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-12
**Feature**: [spec.md](../spec.md)

## Content Quality

- [ ] No implementation details (languages, frameworks, APIs) — **accepted deviation, see Notes**
- [x] Focused on user value and business needs
- [ ] Written for non-technical stakeholders — **accepted deviation, see Notes**
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [ ] Success criteria are technology-agnostic (no implementation details) — **accepted deviation**
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [ ] No implementation details leak into specification — **accepted deviation, see Notes**

## Notes

- Items marked incomplete require spec updates before `/speckit-clarify` or `/speckit-plan`

### Accepted deviation: the four technology-agnosticism items

The same four items Features 001 and 002 exempted fail here, and were again **not** fixed. The
constitution itself names the benchmark datasets (BEIR SciFact, NFCorpus, FiQA), the two metrics
(nDCG@10, Recall@100), the crate (`xtriever-eval`) and the CI smoke; ADR-0006 names the commit to
be measured. Restating those without the names would specify nothing. What is preserved: no crate
API items are cited (the reference package and the backend items belong in `plan.md` under Rule 1),
and every success criterion is a count, a tolerance or a byte-equality.

### Iteration log

- **Iteration 1** (2026-09-12): Initial validation. 12 of 16 items pass outright. Three
  `[NEEDS CLARIFICATION]` markers raised, each a scope decision with no defensible default:
  FR-016 (which configuration is *the* baseline), FR-020 (self-validation against published BEIR
  figures), FR-023 (whether the CI smoke job is in scope). The four technology-agnosticism items
  failed and were accepted as the documented deviation above.
- **Iteration 2** (2026-09-12): All three questions answered. FR-016 → one named configuration
  (`lexical-baseline-v1`); FR-020 → published-figure band ±0.10 as a harness-validity check, with
  the figures marked provisional until pinned at planning; FR-023 → blocking CI smoke in scope with
  a recorded one-cycle escape hatch. **14 of 16 items pass**; the 2 remaining are the accepted
  technology-agnosticism deviation.
- **Iteration 3 — implementation** (2026-09-12): the shipped behaviour was checked against every
  requirement. One design detail moved between modules with the reason recorded (report.md
  F-001: BEIR's identical-id pop is applied at scoring time, where BEIR applies it, not in the
  runner); no requirement's wording needed interpretation. The spec's "≈" dataset counts became
  exact at planning (research D1) and are asserted by the loader. Checklist state unchanged:
  12/16, the four technology-agnosticism items remain the documented deviation.
