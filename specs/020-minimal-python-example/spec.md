# Feature Specification: The Minimal Python Demo

**Feature Branch**: `020-minimal-python-example`

**Created**: 2026-09-17

**Status**: Draft

**Input**: User description (first form): "The minimal Python example: one file … that shows
the whole recipe on one screen — read the first N articles of the Simple English Wikipedia
snapshot … reuse the demo's `chunking.documents_for`, `rules.excluded_by` and `hits` helpers
…". **Owner's redirection (2026-09-17)**: "Can we detach it from wiki and make it
self-contained (but minimal)? Place it at `apps/python-minimal-demo`."

So: a self-contained demo at `apps/python-minimal-demo/` — its own small corpus inside the
file, no snapshot, no chunker, no import from the Wikipedia demo — that shows the whole
pipeline through the `xtriever` package alone: build an index from a handful of documents,
search it, print the fused list and then the re-ranked list. It is the thing a person copies
into their own RAG system; the Wikipedia demos (009, 019) stay as the measured ones.

## Why This Spec Reads Technically

Both Wikipedia demos prove the engine over a real corpus with records and parity checks;
neither is the first thing a person should read. What someone assembling their own RAG
system wants is one short file with nothing in the way: a few documents written out, an
index built from them with the package's own calls — create, add, commit, merge — and a
search that shows the pipeline doing its work: the lexical + dense fusion first, then the
cross-encoder's re-ordering. Detaching it from Wikipedia removes the snapshot, the
exclusion rules, the chunker and the artefact shape, none of which the recipe needs when
the documents are already passage-sized. What remains is the engine's contract, and the
test says the file's output *is* the engine's: the same hits, bit for bit, as a direct call.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A person reads the whole recipe on one screen and runs it (Priority: P1)

One file. Run with a query, it builds an index from the documents written in the file,
searches it twice — the fused stage, then the re-ranked stage — and prints both lists, each
hit with its rank, id, score and text; then it removes the index it built. Every step has a
one-line comment; the file needs the package and the two models, nothing else.

**Why this priority**: This is the feature — the thing to copy.

**Independent Test**: `python apps/python-minimal-demo/demo.py "how do bees make honey"`
prints the two lists within ten seconds on this laptop; the file is at most 80 lines
including comments and docstring.

**Acceptance Scenarios**:

1. **Given** the package and the two models, **When** the script runs with a query, **Then**
   it prints how many documents it indexed, the fused list, the re-ranked list — rank, id,
   score, text per hit — and exits 0, leaving nothing on disk.
2. **Given** the file, **When** read, **Then** the documents, the schema, the build (create,
   add, commit, merge), the two searches and the printing are visibly separate blocks with
   one comment each, and nothing else is in the file.
3. **Given** a query that matches nothing lexically, **When** run, **Then** the dense stage
   still returns the nearest documents and the lists are printed (the pipeline degrades to
   what it has, it does not fail).
4. **Given** no query, **When** run, **Then** a one-line usage message and exit 2.

---

### User Story 2 - The file's output is the engine's (Priority: P1)

The hits the script prints are the hits the engine returns for the same documents and
query — ids in the same order and identical scores — verified against a direct use of the
package in the test; a second run gives the same output (determinism).

**Independent Test**: the model-backed test builds the same documents directly through the
package, searches at the same two depths, and compares ids in order and score bits.

---

### User Story 3 - A README that fits the file (Priority: P2)

A short README: what the demo is, the two inputs and how to fetch them, the one command,
what the output shows (fused, then re-ranked), what is deliberately left out and which demo
does it (chunking and a real corpus: 019; the phone: 009).

---

### Edge Cases

- A missing model directory: the script fails on the package's own error (it does not
  reimplement input checks); the README names the fetch command.
- The re-ranker is optional in the package; the script always attaches it — the point is
  the two stages.
- The corpus is small (eight to twelve documents) so the lexical and dense stages visibly
  disagree on at least one query the README suggests; the documents are ordinary prose,
  not the 007 fixture's nonsense vocabulary.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The demo MUST be one file, `apps/python-minimal-demo/demo.py`, plus a README
  and its tests; it MUST depend on nothing but the standard library, the `xtriever` wheel
  and the two pinned models at their standard locations (overridable by the two environment
  variables the other demos use); it MUST NOT import from `apps/python-wiki-demo` or read
  the Wikipedia snapshot.
- **FR-002**: The corpus MUST be written in the file: eight to twelve short documents of
  ordinary prose, each with an id and a text (a title line, a blank line, a body — the
  convention the other demos use — so the same file shape works for a real corpus).
- **FR-003**: The script MUST, as visibly separate commented blocks: define the schema (one
  text field, the dense field — the layout Feature 013 measured best); create the index in
  a temporary directory; add the documents; commit; merge; search at depth 0 and at depth
  10 (k = 5); print both lists; remove the temporary directory.
- **FR-004**: Each printed hit MUST show its rank, id, the engine's score, its re-rank score
  when re-ranked, and its text's first line.
- **FR-005**: The file MUST be at most 80 lines including the docstring and comments.
- **FR-006**: Exactly one positional argument (the query); none → usage and exit 2.
- **FR-007**: Tests first, committed failing: a model-free test of the line budget, the
  usage exit and the printing on stub hits; a model-backed test that the script's hits equal
  a direct package call's on the same documents at both depths (ids in order, score bits)
  and that two runs agree.
- **FR-008**: No change under `crates/`, `swift/`, `python/src`, `apps/python-wiki-demo` or
  any baseline; no new dependency; CI runs nothing that needs the models.

### Key Entities

- **Document**: id, text (title line + blank line + body).
- **Hit line**: rank, id, score, re-rank score (re-ranked list only), first line of the text.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The file is ≤ 80 lines (counted by a test).
- **SC-002**: The script's hits equal a direct package call's for the same documents and
  query at depths 0 and 10 — ids in order and identical score bits (the test); two runs give
  identical output.
- **SC-003**: From a fresh checkout with the wheel and the models, one command runs the demo
  end to end in under ten seconds on this laptop.
- **SC-004**: `git diff --stat main -- crates/ swift/ python/src apps/python-wiki-demo specs/*/baselines` is empty.

## Assumptions

- **The index is built each run** into a temporary directory and removed: with a dozen
  documents that is a second or two, and it keeps the file free of output-path handling.
- **k = 5, depths 0 and 10** (the demos' default depth; with ≤ 12 documents depth 10
  re-ranks the whole head).
- **Tests run in the Wikipedia demo's environment** (`apps/python-wiki-demo/.venv`, which
  has the wheel and pytest) — the minimal demo itself needs no environment of its own
  beyond a Python with the wheel installed; the README says so.
- **No branch or commit is made by the agent**; the owner commits at the checkpoints.
