# Specification Quality Checklist: iOS Build Spike — tantivy, tokenizers, candle on Device

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-10
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

Four checklist items — "no implementation details", "written for non-technical stakeholders",
"success criteria are technology-agnostic", and "no implementation details leak" — fail as literally
written, and were **not** fixed. They are recorded here rather than worked around, per Agent
Operating Rule 6.

The reason is that this feature is a de-risking spike whose subject matter *is* the technology
choice. The question it exists to answer — "do `tantivy`, `tokenizers` and `candle` build and run on
an iPhone inside 300 MB?" — cannot be restated without naming those crates and the
`aarch64-apple-ios` triples. Rewriting the requirements technology-agnostically would produce a spec
that no longer specifies the work: FR-001's verdict matrix, FR-002's feature-set record, and SC-001's
8-of-8 count would all lose their referents. The stakeholder for a spike is a developer, not a
business reader.

What was preserved instead:

- Success criteria remain **measurable** and outcome-shaped (counts, thresholds, byte-equality)
  rather than prescribing how the binding is built.
- Specific crate API items are **not** cited anywhere in the spec. Under Agent Operating Rule 1 those
  must be read from the pinned versions' documentation and cited in `plan.md`, not guessed here.
- The spec carries a "Why This Spec Reads Technically" section stating this deviation inline, so a
  future reader does not mistake it for sloppiness.

If a future spec for ordinary product work fails these same four items, that *is* a real defect —
this exemption is specific to spikes.

### Iteration log

- **Iteration 1** (2026-09-10): Initial validation. 11 of 16 items pass. 2 `[NEEDS CLARIFICATION]`
  markers open (FR-031 spike-code lifetime, FR-032 model delivery), both surfaced to the user as
  questions along with a third question on weight precision that arose while writing the memory
  criteria. 4 technology-agnosticism items failed and were accepted as a documented deviation
  rather than resolved.
- **Iteration 2** (2026-09-10): All three questions answered. FR-031 resolved to a provisional
  binding with a durable harness; FR-032 resolved to bundled weights; new FR-033 added pinning
  as-published 32-bit weights. Assumptions updated to tie the embedding tolerance to FR-033 and to
  record the expected weight share of the 300 MB ceiling. **12 of 16 items pass**; the 4 remaining
  failures are exactly the accepted technology-agnosticism deviation documented above. No further
  iteration needed — those 4 will not pass by design.
