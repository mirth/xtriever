# Feature Specification: Python Interface and Bindings

**Feature Branch**: `011-python-bindings`

**Created**: 2026-09-15

**Status**: Draft

**Input**: User description: "Feature 011: a Python interface and bindings for Xtriever. Python programs should be able to open a hybrid index with the pinned models, search it and read the pipeline's hits, explanations and stage report — the same engine, the same results bit for bit as the Rust and Swift surfaces — through an installable Python package. Reuse the existing xtriever-ffi (uniffi) surface rather than a second binding layer; goldens/parity against the 007 fixture; packaging as a wheel; host platforms (macOS arm64, Linux x86_64); CI stays light (no models in CI)."

## Why This Spec Reads Technically

The user is a Python programmer building a RAG application on a laptop or a server, who
wants Xtriever's pipeline — lexical, dense, fusion, re-rank, with explanations and a stage
report — as an ordinary installable package, without touching Rust. Xtriever already has a
foreign surface (007): one object that opens an index read-only with the pinned models and
searches it, whose Swift wrapper the iOS demo uses. This feature gives that surface a Python
face: the same calls, the same values, the same errors, the same numbers bit for bit, from
`pip install` to a first result. Because the engine and its oracles exist, the requirements
speak of identity with them rather than of new behaviour; the only new behaviour is what a
Python user needs that a phone did not.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A Python program searches an Xtriever index (Priority: P1)

A developer installs the package, points it at an index directory and the two model
directories, opens the index, and searches it. Each hit carries the external id, the passage
text, the fused score, the re-rank score when a re-ranker is attached, chunk provenance and —
on request — the seven-feature explanation; the response carries the stage report and the
elapsed time. Options (k, candidate depth, re-rank depth, time budget, strict mode, explain)
are the engine's.

**Why this priority**: This is the feature: the engine reachable from Python.

**Independent Test**: With the 007 fixture index and the models on disk, a script opens the
index and searches every fixture query; hits and scores equal the committed goldens bit for
bit.

**Acceptance Scenarios**:

1. **Given** the package installed and an index built by the engine, **When** a program opens
   it with the embedder and the re-ranker, **Then** it gets the index's identity (documents,
   format version, embedder fingerprint, re-ranker id, configuration, model load times).
2. **Given** an open index, **When** a program searches with default options, **Then** it
   receives the pipeline's hits in the engine's order with the engine's scores and the stage
   report — equal to what the Rust surface returns for the same directory, models, query and
   options.
3. **Given** a search with explanations requested, **When** the hits are read, **Then** every
   hit carries the seven pipeline features under the engine's names, with "not seen by this
   stage" as an absent value.
4. **Given** a search under a time budget that a stage exhausts, **When** non-strict, **Then**
   the response degrades as the engine defines and the report says which stage and why;
   **When** strict, **Then** the program receives the engine's error as a Python exception of
   the matching kind.
5. **Given** a missing directory, a wrong-format index, a mismatched model, or an interrupted
   commit, **When** opening, **Then** the program receives an exception carrying the engine's
   message, one exception class per engine error kind.

---

### User Story 2 - The Python results are the engine's, provably (Priority: P1)

The package ships with its oracle: a test suite that opens the 007 fixture index and checks
every query against the committed goldens (the same file the Swift suite uses), score bits
included; the error mapping against the engine's error kinds; the option handling (strict,
budget, depth, explain) against the engine's documented behaviour.

**Why this priority**: A binding that returns something slightly different is worse than no
binding (constitution II, VI).

**Independent Test**: The Python test suite passes against a locally built package with the
fixture and models present; the same suite's model-free subset passes in CI.

**Acceptance Scenarios**:

1. **Given** the fixture index and goldens, **When** the suite runs, **Then** every query's
   hits (ids, order, fused score bits, re-rank score bits, provenance, explanation) equal the
   goldens, with and without the re-ranker.
2. **Given** each engine error kind, **When** provoked from Python, **Then** the matching
   exception class is raised with the engine's message.
3. **Given** no models on the machine (CI), **When** the model-free subset runs, **Then** the
   package imports, its version and surface are what the engine exports, and every open-time
   refusal that needs no model (missing directory, bad path) raises the right exception.

---

### User Story 3 - Installation is one step on the supported platforms (Priority: P2)

On macOS (Apple silicon) and Linux (x86_64), the package installs from a wheel built from
this repository by one documented command and imports without a Rust toolchain on the
installing machine; the models are fetched by the existing pinned script (or the package
tells the user how). The generated binding code is never committed — it is produced by the
build from the same source the Swift bindings come from.

**Why this priority**: Without a wheel the feature is a Rust project with a Python file in it.

**Independent Test**: `pip install` of the built wheel into a fresh virtual environment on
each platform, then the fixture search from US1.

**Acceptance Scenarios**:

1. **Given** a checkout with the Rust toolchain, **When** the documented build command runs,
   **Then** it produces a wheel for the host platform that installs into a clean environment
   and imports.
2. **Given** the installed wheel and no Rust toolchain, **When** the US1 script runs, **Then**
   it works.
3. **Given** the repository's CI, **When** it runs, **Then** it builds the wheel on Linux and
   runs the model-free subset only — no model download, no dataset, as the standing rule
   requires.

---

### User Story 4 - A Python program can also build an index (Priority: P1)

The Python surface also covers index building: create an index with a schema, add
documents (fields and optional chunk provenance) embedded by the pinned embedder or with
caller-supplied vectors, delete, commit — so a Python user goes from their own documents to a
search without Rust. Today no non-Rust path can build an index (the CLI's builder is
Wikipedia-specific). *(Owner decision 2026-09-15, Q1 = B: read + build.)*

**Why this priority**: It decides whether a Python user can go from their documents to a
search without Rust.

**Independent Test**: A script creates an index from a handful of documents, commits, reopens
and searches; the results equal what the Rust surface produces for the same documents.

**Acceptance Scenarios**:

1. **Given** a schema (fields, kinds, boosts, dense fields) and documents (external id, field
   values, optional chunk provenance), **When** a program creates an index, adds them and
   commits, **Then** the directory is a valid index the engine (and the Swift package) opens.
2. **Given** an index built from Python, **When** searched from Python and from Rust, **Then**
   the hits are identical.
3. **Given** a replace or a delete before commit, **When** searched, **Then** the committed
   view is what is searched; after commit, the change is visible.

---

### Edge Cases

- Two Python threads searching one handle: calls are serialised by the engine (007); neither
  hangs, both get results; the interpreter is not blocked by an in-flight search.
- An index directory that is read-only or inside a bundle: opens as 008 defined (in place,
  lock-free).
- The re-ranker directory omitted: searches work with fused order; `rerank_score` is absent;
  the report says the stage was not configured.
- Non-ASCII queries and passages, very long queries (the engine truncates at the embedder's
  window): pass through unchanged.
- The handle dropped while a search runs (garbage collection): the search completes, the
  handle is freed afterwards; no crash.
- A wrong Python version, a missing shared library, an unsupported platform: a clear import
  error, not a segfault.
- Model directories that exist but hold the wrong files: the engine's error (fingerprint or
  load), not a Python-side guess.

## Requirements *(mandatory)*

### Functional Requirements

**Surface**

- **FR-001**: The Python package MUST expose the existing foreign surface — open (index
  directory, embedder directory, optional re-ranker directory, load path), info, search with
  the wire options — as Python classes and functions generated from the same source as the
  Swift bindings; no hand-maintained duplicate of the surface.
- **FR-002**: Every wire type (hit, explanation, chunk provenance, stage report, re-rank
  report, degradation, index info, options, load path) MUST arrive in Python with the same
  fields, names (in Python's naming convention) and optionality as the foreign surface
  defines.
- **FR-003**: Every engine error kind MUST map to a distinct Python exception class carrying
  the engine's message; all of them MUST share one base class.
- **FR-004**: The package MUST expose index building (Q1 = B): create with a schema
  (field name, kind, indexed, stored, boost; dense fields; candidate depth, RRF k, re-rank
  depth), add documents (external id, field values, optional chunk provenance) embedded by the
  pinned embedder or with caller-supplied vectors, delete by external id, commit; with the
  engine's semantics (committed view searched until commit; ids never reused; refusals as the
  engine's errors).
- **FR-005**: Searches MUST NOT hold the interpreter lock while the engine works, so other
  Python threads run during a search.

**Identity**

- **FR-006**: For the same directory, models, query and options, the Python result MUST equal
  the Rust surface's bit for bit (ids, order, fused and re-rank scores, provenance,
  explanation, stage report), verified against the committed 007 goldens with and without the
  re-ranker.
- **FR-007**: The engine's degradation, strict mode, time budget and tie-breaking behaviour
  MUST be unchanged and reachable: the Python tests exercise strict vs non-strict under a
  1 ms budget, depth 0, and explain on/off.

**Packaging**

- **FR-008**: The package MUST build into a wheel for the host platform by one documented
  command from a checkout (Rust toolchain required to build, not to install), on macOS arm64
  and Linux x86_64; the generated binding code and the shared library MUST NOT be committed.
- **FR-009**: The wheel MUST NOT contain the models; the package MUST document the pinned
  fetch (the repository's script) and MUST fail with the engine's error, not a crash, when a
  model directory is missing.
- **FR-010**: Supported Python versions MUST be stated and enforced by the package metadata;
  an unsupported interpreter or platform MUST fail at install or import with a message.

**Discipline**

- **FR-011**: The Python test suite MUST be written first and committed failing; the golden
  comparison is against `swift/Xtriever/Tests/Fixtures/expected.json` (the 007 oracle) — no
  new goldens are minted for this feature.
- **FR-012**: CI MUST build the wheel and run only the model-free subset of the suite on
  Linux; no model, dataset or simulator job (standing rule).
- **FR-013**: No change to `xtriever-core` traits, `deny.toml`, the stage crates or the
  pipeline; changes to `xtriever-ffi` are limited to the builder exports (Q1 = B) and MUST keep the Swift package's existing surface and goldens
  unchanged.
- **FR-014**: No new hand-written `unsafe`; Python-side code is plain Python over the generated
  module (thin naming/ergonomic wrappers at most, no logic).

### Key Entities

- **Package**: the installable unit — the generated binding module, the shared library, a
  thin public module, metadata (version = the workspace version, supported Pythons,
  platforms).
- **Index handle**: the open index with its models; `info()` and `search(query, options)`.
- **Search options / response / hit / explanation / stage report**: the 007 wire types,
  unchanged.
- **Errors**: one exception per engine error kind under a common base.
- **Builder**: schema, document, field value, chunk provenance; create / add /
  delete / commit.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On both supported platforms, `pip install` of the built wheel into a fresh
  environment followed by the fixture search succeeds with no Rust toolchain on the path.
- **SC-002**: 100 % of the 007 fixture queries return hits equal to the committed goldens from
  Python — ids, order, fused score bits, re-rank score bits, provenance, explanation — with
  and without the re-ranker.
- **SC-003**: Every engine error kind is raised as its own exception class in the test suite
  (one test per kind that can be provoked without a network).
- **SC-004**: A search from Python costs no more than 5 % over the Rust surface's wall time on
  the same query set on the host (the binding adds a call, not a computation).
- **SC-005**: With one search in flight, a second Python thread completes 100 % of 20
  short computations without waiting for the search to finish (the interpreter lock is
  released).
- **SC-006**: CI's added job runs in under 10 minutes and downloads nothing but crates.
- **SC-007**: An index built from Python with the fixture's 40 documents produces the
  same hits as the fixture index built by Rust for every fixture query.

## Assumptions

- **Binding mechanism**: the existing foreign surface's generator produces the Python module;
  the engine is not touched (constitution I, V). A second binding layer was not considered.
- **Synchronous API**: Python calls are synchronous; concurrency is the caller's (threads —
  the lock is released; or an executor). An async facade is not part of this feature.
- **Models**: fetched by `scripts/fetch-model.sh` into directories the user passes; the
  package does not download.
- **Platforms**: the two the owner named; other platforms build from source if the toolchain
  supports them, unverified.
- **Python floor**: the lowest version the generator supports, stated in the plan.
- **Version**: the package version equals the workspace crate version.
- **CI**: one Linux job — build the wheel, run the model-free tests — path-filtered to the
  FFI crate and the Python directory; no macOS job (standing rule on runner cost).
