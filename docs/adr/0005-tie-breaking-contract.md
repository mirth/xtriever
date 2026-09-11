# ADR-0005: Stages re-sort score ties into ascending `DocId`

- **Status**: Accepted — 2026-09-11; **amended** 2026-09-12 (k-boundary, see below)
- **Date**: 2026-09-11
- **Deciders**: mirth (repository owner), 2026-09-11
- **Origin**: [research.md](../../specs/001-ios-build-spike/research.md) risk R4, raised by Feature 001
- **Blocks**: the first `LexicalIndex` implementation, and any multi-segment index

## Context

Two documents state the same rule, and the chosen backend does not follow it.

**Principle VI** (constitution): *"Same index + same query + same config ⇒ identical results (ties
broken by ascending `DocId`)."*

**`xtriever-core`** repeats it on the contract itself — `LexicalIndex::search` and
`VectorIndex::search` both say *"Top-`k` hits, highest score first, ties broken by ascending
`DocId`."*

**tantivy 0.26.2** breaks ties by ascending `DocAddress`, not `DocId`. From the `TopDocs` docs:

> *"This collector guarantees a stable sorting in case of a tie on the document score/sort key: The
> document address (`DocAddress`) is used as a tie breaker. In case of a tie on the sort key,
> documents are always sorted by ascending `DocAddress`."*

`DocAddress` is `{ segment_ord: SegmentOrdinal, doc_id: DocId }` — **segment-ordinal-major**. The two
orderings coincide only while the index has exactly one segment.

Feature 001 sidestepped this rather than solving it: it pinned the writer to one thread, asserted
`segment_count == 1`, and recorded that the on-device ranking was bit-identical to the host golden.
That result is sound, and it is silent about the multi-segment case. A real lexical index has
multiple segments almost immediately — every commit can create one, and merges change segment
ordinals — so the first production implementation walks straight into it.

Worth stating plainly: this is **not** a determinism problem. tantivy's ordering is perfectly
deterministic. It is a *contract* problem — the order is deterministic but not the order
`xtriever-core` promises, and a caller who relies on the documented promise gets a different answer
than the backend gives.

## Decision

**Stage implementations re-sort ties before returning.** `LexicalIndex::search` and
`VectorIndex::search` return hits ordered by `(score DESC, DocId ASC)`, re-sorting the backend's
output where the backend's own tie-break differs.

The core contract stays as written. `DocId` is the one identifier every backend shares, it is what
`Hit` carries, and it is the only tie-break that can mean the same thing across tantivy, a future
vector index, and anything else.

Concretely, for the tantivy-backed stage: collect `k` hits, map each `DocAddress` to the internal
`DocId`, then `sort_by` on `(Reverse(score), doc_id)` with a stable sort.

### Cost

A stable sort of `k` elements per query, where `k` is a top-k limit — typically 10 to 100. Negligible
next to the search itself, and it does not grow with corpus size.

### What this does *not* cover

Ties **at the k boundary**. If the k-th and (k+1)-th documents have equal scores, which one the
backend returned is already decided before re-sorting, and re-sorting cannot recover a document the
collector never emitted. Making that boundary deterministic would mean over-fetching and truncating
after the re-sort. That is a real but separate decision, and the lexical spec should take it
deliberately rather than inherit it by accident — this ADR records the gap rather than pretending the
re-sort closes it.

## Consequences

**Binding on Feature 002.** Nothing in the current tree violates this ADR — Feature 001 ran at one
segment, where the two orderings coincide — so accepting it requires no code change today. It binds
the first `LexicalIndex` and `VectorIndex` implementations, and `xtriever-core`'s trait docs now
point implementers at it (see `traits.rs`), because the failure mode is silent until two documents
tie.

**Positive**

- The documented contract becomes true, rather than true-only-at-one-segment.
- `xtriever-core` stays backend-agnostic: no `DocAddress`, no segment concept, leaks into it.
- Cross-backend comparisons stay meaningful, which matters directly for fusion — merging a lexical
  and a dense ranking is much harder to reason about if the two disagree on tie order.
- The Feature 001 golden fixtures remain valid unchanged, since at one segment the two orders agree.

**Negative**

- Every stage implementation carries the re-sort. It is easy to omit and the omission is invisible
  until two documents tie, which makes it a good candidate for a property test in the lexical spec
  rather than a code-review expectation.
- The k-boundary gap above remains open.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Amend `xtriever-core` to promise `DocAddress` order | `DocAddress` is a tantivy concept. Putting it in the core contract would leak one backend's internals into the interface every other backend must implement, directly against Principle V's "small, explicit interfaces". |
| Weaken the contract to "deterministic, backend-defined" | Cheapest, and it makes the promise nearly useless: a caller could no longer compare two backends' output, and fusion would have to special-case each. It also silently widens Principle VI, which is a constitution amendment wearing a disguise. |
| Force a single segment forever | What Feature 001 did as a *measurement control*, not a design. It would cap index size and forbid background merges — a severe constraint accepted to avoid a stable sort of 10 elements. |
| Leave it, since it only shows up on ties | Ties are not rare. BM25 scores collide readily on short documents with equal term frequency and equal quantized field length — Feature 001's first fixture attempt produced a top-10 that was *entirely* tied, which is what surfaced this in the first place. |

---

## Amendment — the k-boundary (accepted 2026-09-12)

- **Status of this amendment**: Accepted — 2026-09-12, mirth (repository owner)
- **Origin**: Feature 002 clarification Q1 ([spec.md](../../specs/002-lexical-stage/spec.md),
  Clarifications › Session 2026-09-11; FR-014)

The gap this ADR recorded under *"What this does not cover"* is now decided.

**Decision**: A score tie spanning the `k`-th and `(k+1)`-th positions is resolved by the backend's
own ordering. Stages **do not over-fetch** to close it. The `DocId` tie-break therefore governs the
**order** of the returned hits, not their **membership**: given a tie group straddling the boundary,
the members that appear are whichever the backend selected — stably, since its order is
deterministic — and those that appear are ordered by ascending `DocId`.

**Rationale**: Over-fetching by a fixed margin is correct only for tie groups smaller than the
margin and needs a second rule for larger ones; adaptive over-fetching degenerates to reading the
whole posting list on a `Term` query over a keyword field where every document ties. Accepting the
boundary costs nothing, is deterministic, and is honest about what the backend decides. Chosen for
simplicity by the repository owner.

**Contract change**: `LexicalIndex::search`'s doc comment in `xtriever-core` gains the sentence in
[contracts/lexical-index.md](../../specs/002-lexical-stage/contracts/lexical-index.md) ("Contract
caveat"). `VectorIndex::search` gains the same sentence, since the reasoning is backend-independent.
This is documentation only — no signature, type or behaviour changes — and it is the only
`xtriever-core` edit Feature 002 makes (spec FR-002, FR-014).

**Test obligation**: Feature 002 plants a k-boundary tie in its fixture corpus and asserts the
returned membership and order exactly, so the accepted behaviour is executable rather than
described (spec Story 2 scenario 5).
