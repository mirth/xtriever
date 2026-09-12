# Specification Quality Checklist: The Dense Stage

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

The same four items Features 001–003 exempted fail here and were again **not** fixed: the
deliverable is two named `xtriever-core` traits made true against a model whose identity Feature
001 pinned by revision and hash. Restating that without the names would specify nothing. As before,
no crate API items are cited — the inference engine's items belong in `plan.md` under Rule 1 — and
every success criterion is a count, a tolerance or a byte-equality.

### Iteration log

- **Iteration 1** (2026-09-12): Initial validation. 12 of 16 items pass outright. Two
  `[NEEDS CLARIFICATION]` markers raised — the weight load path (FR-008: memory versus an `unsafe`
  block outside the SIMD allowance, with ADR-0002's permission expired) and the dense baseline's
  dataset coverage (Story 3 scenario 5: FiQA embedding cost). Both are scope/governance decisions
  with no defensible default. The four technology-agnosticism items failed and were accepted as the
  documented deviation above.
- **Iteration 2** (2026-09-12): Both markers resolved by the human. FR-008 → **both load paths
  behind one Cargo feature** (safe buffered by default, `mmap` opt-in under a new ADR that also
  settles the Principle VII wording; bit-identical test; memory measured per path). The vector
  index's read path follows the same flag and ADR (Assumptions). Story 3 scenario 5 → **all three
  datasets**, FiQA embedded once and cached locally; SC-006 and FR-023 now name the set. Story 4
  scenario 2 and FR-023 reworded from "if both paths exist" to "each path". 12 of 16 pass; the
  remaining 4 are the accepted deviation. Ready for `/speckit-plan`.
