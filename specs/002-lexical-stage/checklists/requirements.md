# Specification Quality Checklist: The Lexical Stage

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-11
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

The same four items that Feature 001 exempted fail here, for a related but not identical reason, and
were again **not** fixed (Agent Operating Rule 6 — record, don't work around).

Feature 001's exemption was that a spike's subject matter *is* the technology choice. This feature's
is narrower and stronger: the deliverable is **a specific Rust trait made true**. `LexicalIndex` and
its eight methods, the six `LexicalQuery` variants and the eight `Filter` variants are not an
implementation of some technology-agnostic requirement — they are the requirement, already written
down in `xtriever-core` and unchangeable here without an ADR (FR-002). Restating FR-001 without
naming them would produce a sentence that specifies nothing.

What was preserved instead, exactly as in 001:

- Success criteria stay **measurable** and outcome-shaped — counts of methods, query shapes and
  filter shapes; zero-difference comparisons; a stated tolerance — rather than prescribing how any
  of it is built.
- **No crate API items are cited.** tantivy appears only as the named backend in the Assumptions
  section and never as an API. Under Agent Operating Rule 1 the actual items must be read from the
  pinned version's documentation and cited in `plan.md`.
- The spec carries a "Why This Spec Reads Technically" section stating the deviation inline.

Unlike 001, this is production code, so the exemption is **narrower**: it covers naming the core
contract, not naming the backend's internals. A requirement that reached into tantivy's API would be
a real defect here, not a documented deviation.

### Iteration log

- **Iteration 1** (2026-09-11): Initial validation. 12 of 16 items pass outright. Four
  `[NEEDS CLARIFICATION]` markers were raised while writing the requirements; the skill caps
  questions at 3, so the fourth (FR-036, index memory scaling) was **resolved in the spec** rather
  than asked. It was the right one to drop: the user's standing instruction from Feature 001 close-out
  was to not worry about memory consumption for now, so deferral is already the answer and asking
  would have burned a question on a settled point. The three surfaced are FR-014 (the k-boundary tie),
  FR-024 (deleted documents in statistics) and FR-027 (the concurrency model) — each changes what the
  implementation must do, and none has a defensible default. The four technology-agnosticism items
  failed and were accepted as a documented deviation, per the note above.
- **Iteration 2** (2026-09-11): All three questions answered, spec updated, **14 of 16 items now
  pass** — the 2 remaining failures at that point were the technology-agnosticism deviation. The
  answers were FR-014 → accept the backend's k-boundary ordering, FR-024 → live-only statistics,
  FR-027 (now FR-028) → one handle with exclusive writes. Each answer pulled a consequence with it
  that the spec now states rather than leaves implicit:
  - **FR-014** narrows a promise `xtriever-core` states unconditionally, so it is now gated on
    amending ADR-0005 and clarifying the `search` doc comment. That doc comment is the one permitted
    core change in this feature (FR-002), and it is permitted only because the ADR satisfies
    Principle V.
  - **FR-024** makes statistics a function of the live corpus, which is what makes them independent
    of deletion history — but scoring still consumes the backend's deletion-inclusive statistics.
    New **FR-025** requires that divergence to be measured and recorded rather than papered over,
    and **FR-015** was rewritten to scope the determinism claim to a fixed mutation history and to
    require the differing-history case to be measured as a possible Principle VI finding.
  - **FR-028** pushes the writer/reader split onto the caller, which is only implementable if the
    caller can open read-only. New **FR-029** requires that path, **User Story 6** covers it, and
    **FR-031** requires each two-handle combination to be defined and tested.
  Net effect: 37 requirements became 39, 10 success criteria became 13, and a sixth user story was
  added. Renumbering moved the old FR-024–FR-037 up by one or two; all internal cross-references
  were re-checked and FR-001–FR-039 is gapless with no duplicates.
