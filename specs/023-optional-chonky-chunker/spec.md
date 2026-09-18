# Feature Specification: Chonky as an Optional Chunker for the Wikipedia Demo Build

**Feature Branch**: `023-optional-chonky-chunker`

**Created**: 2026-09-18

**Status**: Draft

**Input**: User description: "Let's make Chonky chunking as optional chunker then." — said on
reading the Feature 022 chunking study's verdict (no chunking variant recommended by the
study's rule; the effect is corpus-dependent: chonky gains +2.9 nDCG@10 points on SciFact,
moves nothing on NFCorpus and loses 7 points on FiQA). Feature 021 had made chonky the
Wikipedia demo build's only splitter, replacing the demo's copy of the Feature 008 contract
chunker and with it the byte-identity between a demo-built index and the Rust build's.

## Why This Spec Reads Technically

The Wikipedia demo build (`wikidemo build`, `apps/python-wiki-demo`) shows a reader how to
turn a raw snapshot into a searchable index. Since Feature 021 that recipe splits every
article with `chonky`, a small neural splitter — which is the readable choice, but it is
also a choice the study could not justify as a default: it costs a `torch` and
`transformers` install on every machine that runs the demo, it costs a pinned model
download, and it makes the demo's index a different index from the one the iOS app ships
(the shipped one is cut by the Feature 008 contract chunker). This feature makes the
splitter a **build-time option**: the default recipe is the shipped one again (the 008
contract chunker, the demo's build once more provably the same as the Rust build's), and
`chonky` is selected by a flag, installed as an extra, and refused with the install command
when it is missing. Nothing chonky-related is lost — its code path, its pinned model, its
tests and its 021 run record all stay — it just stops being mandatory.

**Owner's decisions (2026-09-18, on this spec's questions)**: Q1 — the default chunker
is the 008 contract chunker, chonky is `--chunker chonky`; Q2 — chonky and what it runs on
are an optional dependency extra.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The default build is the shipped recipe (Priority: P1)

A reader runs `wikidemo build --limit 2000 --out DIR` with no chunker flag. The build
verifies the snapshot, drops the excluded articles, splits each with the 008 contract
chunker, adds the passages, commits, merges and writes the sidecars — the recipe the Rust
CLI uses for the shipped artefact. `wikidemo measure --against` the Rust-built slice of the
same 2,000 articles says PASS: same corpus identity, same documents, same order at every
depth.

**Why this priority**: it is the point of the feature — the demo's index is the shipped
index again, and a default build needs no neural splitter, no `torch`, no extra model.

**Independent Test**: build the 2,000-article slice with the default chunker; run
`measure --against target/xt-wiki-slice-rs`; expect PASS. The fixture-replay tests
(`reference/fixtures/008/chunk_a.json`, 48 cases; `chunk_b.json`, nine real articles)
pass byte-identically against the restored chunker.

**Acceptance Scenarios**:

1. **Given** the snapshot, its manifest and the two engine models, **When** `wikidemo
   build --limit 2000 --out DIR` runs, **Then** the sidecar's chunker block is the contract's
   (`{"version": 1, "budget": …, "cost": …}`), `passages_over_window` is 0, and the corpus
   identity equals the Rust slice's.
2. **Given** that index, **When** `wikidemo measure --against target/xt-wiki-slice-rs`
   runs, **Then** the verdict is PASS.
3. **Given** a machine without the chonky extra installed, **When** the default build
   runs, **Then** it completes without importing `chonky`, `transformers` or `torch`.

---

### User Story 2 - `--chunker chonky` splits as Feature 021 did (Priority: P1)

The reader adds `--chunker chonky`. The build loads the pinned chonky model from its local
directory and splits each article at predicted paragraph breaks exactly as Feature 021's
build did: each non-empty chunk one passage, byte offsets into the article, the chunks a
partition of the text, chunks past the embedder's window embedded as they are and counted.
The sidecar's chunker block names chonky, its model and revision; `about` says so.

**Why this priority**: the option is the feature's other half; the 021 behaviour must
survive unchanged behind the flag.

**Independent Test**: the 021 tests (partition and offsets on the synthetic articles and on
the snapshot's first article; the document shaping; the sidecar block; the synthetic
snapshot build) pass with `--chunker chonky`; a `--limit 200` chonky build reports a
non-zero over-window count and the 021 chunker block.

**Acceptance Scenarios**:

1. **Given** the chonky extra and model installed, **When** `wikidemo build --chunker chonky
   --limit 200 --out DIR` runs, **Then** the passages are chonky's, the sidecar's chunker
   block is `{"name": "chonky", "model": "mirth/chonky_distilbert_base_uncased_1",
   "revision": "01d8aae…"}` and `about DIR` prints the chonky label with the over-window count.
2. **Given** the chonky model directory missing, **When** `--chunker chonky` is requested,
   **Then** the build stops before loading anything and prints the fetch command.

---

### User Story 3 - Chonky is an install-time extra (Priority: P2)

The demo's mandatory dependencies are the engine's wheel and the tokenizer library the
contract chunker prices with. `chonky` and what it runs on (`transformers`, `torch`) are an
optional extra: `pip install -e '.[chonky]'`. A build that asks for chonky without the extra
is refused at once with that command; a default build never needs it; the input check
(`needs_for`) asks for the chonky model directory only when chonky is the chunker.

**Why this priority**: it is what "optional" means for the person installing the demo —
without it the flag would be optional but the 2 GB of wheels would not.

**Independent Test**: with the chonky import made to fail, `wikidemo build --chunker chonky`
exits 1 in under a second naming the install command; `wikidemo build` (default) does not
touch the import; the input check for a default build lists four inputs (embedder,
re-ranker, snapshot, manifest), for a chonky build five.

**Acceptance Scenarios**:

1. **Given** an environment without the extra, **When** `wikidemo build --chunker chonky …`
   runs, **Then** it exits 1 with `pip install -e 'apps/python-wiki-demo[chonky]'` in the
   message before any model or the snapshot is opened.
2. **Given** the same environment, **When** `wikidemo build --limit 5 …` runs, **Then** it
   builds.

---

### User Story 4 - The documentation describes both (Priority: P3)

The README's recipe section describes the default (the 008 recipe, the same index as the
Rust build, the parity check) and the option (chonky: what it is, how to install the extra
and fetch the model, the over-window trade-off with the 021 numbers, that the index is then
not the shipped one). `build --help` names both chunkers and the default.

**Independent Test**: reading; the run-order block in the README works top to bottom
without the chonky steps for a default build.

---

### Edge Cases

- `--chunker` given an unknown name: refused by the argument parser with the two choices.
- `--chunker chonky` with the extra installed but the model directory missing: the input
  check reports the model with its fetch command (as in 021), before the extra is imported.
- A default build on a machine that *has* the extra: the extra is not imported.
- `measure --against` a chonky-built index from a contract-built one: the identity differs,
  so the verdict is FAIL with the identity line — as designed since 019.
- The 021 chonky slice record and the 019 parity record both stay under their specs'
  `runs/` as history; this feature's parity record is new.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `wikidemo build` MUST take `--chunker {contract,chonky}`; the default MUST be
  `contract`. `build --help` MUST name both and the default.
- **FR-002**: The contract chunker MUST be the Feature 008 contract's reference
  implementation as the demo carried it in Feature 019 (paragraphs → sentences → words →
  fragments, priced by the embedder's tokenizer at `token_count(unit) − 2` against
  `256 − token_count(title)`), restored unchanged, with its fixture-replay tests
  (`reference/fixtures/008/chunk_a.json`, `chunk_b.json`) requiring byte-identical passages.
- **FR-003**: The chonky path MUST behave as Feature 021 specified (FR-001 to FR-005 there):
  the pinned local model, one passage per non-empty chunk, byte offsets, partition asserted,
  over-window passages added unchanged and counted.
- **FR-004**: The sidecar's `chunker` block MUST be the contract's
  `{"version": 1, "budget": "256 - token_count(title)", "cost": "MiniLmEmbedder::token_count(unit) - 2"}`
  for a contract build and Feature 021's `{"name", "model", "revision"}` for a chonky build;
  the corpus identity MUST be computed over it as before, so a contract-built slice's
  identity equals the Rust build's for the same articles.
- **FR-005**: `chonky`, `transformers` and `torch` MUST move from the demo's dependencies
  to an optional extra named `chonky` at their pinned versions; `tokenizers` MUST stay a
  dependency (it prices the contract chunker and counts the over-window passages). A
  chonky build without the extra MUST be refused with the install command before any input
  is opened; a contract build MUST NOT import the extra.
- **FR-006**: The input check MUST require the chonky model directory only for a chonky
  build; a missing directory is reported with its fetch command as in 021.
- **FR-007**: `counts.passages_over_window` and the `chunking` statistics MUST be reported
  for both chunkers (a contract build's over-window count is 0 by construction).
- **FR-008**: Tests first, committed failing: the `--chunker` parsing and default; the input
  check per chunker; the fixture-replay tests restored; the 021 chonky tests kept and run
  through the option; the synthetic-snapshot build with each chunker (models); the
  refusal without the extra (the import made to fail in the test).
- **FR-009**: The 2,000-article slice MUST be rebuilt with the default chunker and
  `measure --against target/xt-wiki-slice-rs` MUST PASS; the build record and the parity
  verdict MUST be committed under this feature's `runs/`. A `--limit 200` chonky build MUST
  be run and its counts recorded there too.
- **FR-010**: The README MUST describe the default recipe (with the parity claim back) and
  the chonky option (install, fetch, trade-off, not the shipped index). `about`'s chunker
  line MUST cover both blocks (it already does).
- **FR-011**: No change under `crates/`, `swift/`, `python/src`, `apps/python-minimal-demo`,
  `reference/`, any baseline or the shipped artefact; the host-goldens `measure` unchanged;
  CI runs nothing that needs a model.

### Key Entities

- **Chunker option**: `contract` (default) or `chonky`; chosen per build; recorded in the
  sidecar's `chunker` block and so in the corpus identity.
- **Passage**: as before (id `<article id>#<ordinal>`, title, text, `ChunkInfo`).
- **Counts / chunking statistics**: `passages`, `passages_over_window`, token median, p90,
  max — for either chunker.
- **Extra**: the `chonky` optional dependency group (chonky, transformers, torch at the 021
  pins).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The default-built 2,000-article slice's `measure --against` the Rust-built
  slice is PASS (identity, counts, documents, order at every depth) — the demo's build is
  the shipped recipe again.
- **SC-002**: The fixture replay passes: 48 + 9 cases byte-identical; the 021 chonky tests
  pass unchanged in substance behind the option.
- **SC-003**: Without the extra, `wikidemo build --chunker chonky …` is refused in under a
  second with the install command; a default build with `--limit 5` completes and the test
  asserts none of `chonky`, `transformers`, `torch` was imported.
- **SC-004**: `git diff --stat main -- crates/ swift/ python/src apps/python-minimal-demo
  reference/ specs/*/baselines` is empty; the PR stays under ~800 changed lines or states
  the split (the restored chunker copy is ~270 lines of a prior commit).

## Assumptions

- **The Rust-built 2,000-article slice is still on disk** (`target/xt-wiki-slice-rs` from
  Feature 019); if not, it is rebuilt with the Rust CLI as 019's quickstart describes
  (~20 minutes) before the parity check.
- **The restored chunker is the 019 copy verbatim** (commit `802cf72`), so parity is
  expected without re-derivation; the fixtures decide.
- **The chonky path is 021's code moved behind a flag**, not re-implemented.
- **No branch or commit by the agent**; the owner creates `023-optional-chonky-chunker`
  off `main` after 022 merges and commits at the checkpoints.
