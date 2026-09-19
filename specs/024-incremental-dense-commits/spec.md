# Feature Specification: Incremental Dense Commits — an Append/Tombstone/Compact Vector File

**Feature Branch**: `024-incremental-dense-commits`

**Created**: 2026-09-18

**Status**: Draft

**Input**: User description: "The problem is the current flat embeddings file. I don't want
it to overwrite it on each `add` + `commit`." → "Let's build own append/tombstone/compact
format." — after a survey of the alternatives (tantivy fast fields, `usearch`, `hnsw_rs`,
`arroy`, `lance`, `parquet`, `redb`/`sled`/`heed`, `vecstore`): none gives an exact,
contiguous, memory-mappable *and* appendable f32 matrix; `vecstore` in particular rewrites
its whole `vectors.bin` (JSON) on every save.

**Owner's decisions (2026-09-18, on this spec's questions)**: Q1 — format version 1 is
dropped, not read: every existing artefact is regenerated in version 2; Q2 — compaction runs
on the explicit `merge` *and*, when configured, automatically after a commit whose dead-row
share exceeds a threshold (default: off).

## Why This Spec Reads Technically

The dense stage keeps every passage's vector in one flat file, `dense/index.bin`, and
searches it by an exact linear scan. Today `commit` rebuilds that file from scratch —
committed rows plus pending changes, written to a temporary file and renamed into place. It
is crash-safe and simple, and it costs O(index) per commit: adding ten passages to the full
Simple English Wikipedia index (427,947 rows, ~660 MB of vectors) rewrites ~660 MB. That
rules the engine out for the thing it is meant for on a phone — indexing a user's documents
as they arrive. This feature changes the file's shape so that a commit writes only what
changed: new rows are **appended**, deleted or replaced rows are marked in a small
**tombstone** set, and the O(index) rewrite moves to where it belongs — `merge`, the explicit
**compaction** step the lexical stage already has. Search results do not change by a single
bit: the scan still visits every live row, scores accumulate in `f64` per row, and the
final order is total (score, then id), so the order rows are visited in is irrelevant. What
changes is the cost of writing.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A commit writes only what changed (Priority: P1)

An application adds documents to a large existing index and commits. The dense stage
appends the new rows and updates the tombstone set; the bytes written are proportional to
the change, not to the index. Search over the committed index returns exactly what the
current implementation would.

**Why this priority**: it is the feature; the on-device ingest case depends on it.

**Independent Test**: add 10 passages to a 100k-row index and commit; the bytes written to
`dense/` are under 100 KB (today: the whole file, ~150 MB at dim 384); the search results
for a fixed query set are bit-identical to the results of the same sequence under the
current format (goldens generated before the change).

**Acceptance Scenarios**:

1. **Given** a committed index of N rows, **When** k rows are added and committed, **Then**
   the vector file grows by exactly k rows' bytes, the tombstone set is unchanged, and
   `len()` is N + k.
2. **Given** a committed index, **When** a document is deleted or replaced and committed,
   **Then** no existing byte of the vector file changes; the old row's index is in the
   tombstone set; a replacement's new row is appended; `vector(id)` returns the new vector
   (or `None` after a delete); search never returns the dead row.
3. **Given** any interleaving of add / replace / delete / commit, **When** compared with the
   current implementation on the same inputs, **Then** every search result (ids, scores,
   order) is bit-identical, with and without an `allowed` filter.

---

### User Story 2 - `merge` compacts the dense file (Priority: P1)

After many incremental commits the file carries dead rows. `merge` (the pipeline's existing
compaction call) rewrites the dense file without them — one file, live rows only, in
ascending id order — exactly the shape a shipped artefact has today. Results before and
after are bit-identical.

**Why this priority**: without compaction dead rows accumulate forever; with it the shipped
artefact keeps its compact form.

**Independent Test**: after a sequence with deletes and replaces, `merge`; the file's row
count equals `len()`, the tombstone set is empty, and the search goldens still match.

---

### User Story 3 - Crash safety, and one format (Priority: P1)

A crash at any point during a commit leaves an index that opens to the previous committed
state. There is one dense format, version 2: a version-1 file is refused at open with the
existing version error, and every artefact the repository relies on — the fixture index and
its goldens, the demo slices, the shipped Wikipedia index — is regenerated in version 2 with
the same vectors, so every existing golden and parity check passes unchanged.

**Why this priority**: the crash guarantee is a contract (Principle VI: half-states are hard
errors, never silent); one format keeps the reader simple; the goldens prove the
regeneration changed nothing but the bytes' arrangement.

**Independent Test**: for every prefix length of the bytes a commit writes, truncate there
and reopen — the index equals the previous committed state; the regenerated fixture index
passes `expected.json` bit for bit (Features 007/011); the regenerated Wikipedia artefact
passes the host goldens (`wikidemo measure`, 800/800 bits) and the iOS device test's rule;
a version-1 file is refused with the version error naming both versions.

---

### User Story 4 - Compaction on a threshold, when asked (Priority: P2)

An application that never wants to schedule `merge` itself sets a dead-row share in the
index configuration; a commit that pushes the share over it compacts the dense file within
that commit. The default is off, so nothing changes for callers who do not ask.

**Why this priority**: the option gives a phone app a no-maintenance mode; the default keeps
commit cost predictable for everyone else.

**Independent Test**: with the threshold set to 0.25, a sequence whose deletes bring the
dead share to 20 %, then exactly 25 %, then 26 %: no compaction on the first two commits
(the comparison is strict — `dead / rows > threshold`), compaction on the third (file rows
= live rows, tombstones empty); with it unset, the same sequence never compacts until
`merge`.

---

### Edge Cases

- A commit with only deletes: no rows appended; the tombstone set and the manifest change.
- Deleting an id that is not committed and not pending: a no-op (as today).
- Replacing an id twice before committing: one appended row (the pending map already
  collapses this).
- The tombstone set names a row index beyond the committed row count, or the vector file is
  longer than the manifest says (a crash after the append): the tail is ignored at open and
  overwritten by the next append; a manifest that disagrees with the file's length in the
  other direction (file shorter) is `Corrupt`.
- A read-only (memory-mapped) open of a directory another handle is appending to: outside
  the contract, as today (the precondition documented on `open_mapped`); this feature does
  not add multi-writer support.
- `merge` on an index with no dead rows and rows already in ascending order: a no-op that
  writes nothing.
- The fixture under `swift/Xtriever/Tests/Fixtures` is regenerated in version 2 by its
  existing generator; its `expected.json` is *not* regenerated — it must still pass, which
  is the proof the vectors are the same.
- The automatic compaction threshold set to 0 (compact on every commit that has a dead row)
  is legal and costs what it says; a threshold of 1 never fires; values outside `0..=1` are a
  configuration error at open.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `commit` MUST write bytes proportional to the pending change: the appended
  rows, the tombstone set, and a fixed-size manifest — never the existing rows.
- **FR-002**: A row, once written, MUST never be modified in place; deletes and replacements
  MUST be expressed as tombstones (dead row indices) plus, for replacements, an appended row.
- **FR-003**: Search MUST visit every live row and skip dead rows; results (ids, scores, and
  their `(score DESC, id ASC)` order) MUST be bit-identical to the current implementation
  for the same committed content, with and without an `allowed` set.
- **FR-004**: `vector(id)` MUST return the newest live row for `id` and `None` otherwise;
  `len()` MUST be the live row count.
- **FR-005**: The pipeline's `merge` MUST compact the dense file as it merges the lexical
  segments: live rows only, ascending id order, empty tombstone set; results MUST be
  bit-identical before and after (the pipeline's existing merge-determinism tests extend to
  the dense file).
- **FR-006**: A crash at any byte boundary during `commit` or `merge` MUST leave an index
  that opens to either the previous committed state or the fully committed new one — the
  manifest rename is the switch — never to a partial state; a partially appended tail MUST
  be ignored.
- **FR-007**: The new layout is dense format version 2 with its own ADR; version 1 is not
  read — a version-1 file MUST be refused at open with the existing version error (naming
  the file's version and this build's). Every artefact the repository relies on MUST be
  regenerated in version 2 with unchanged vectors: the fixture index (its committed
  `expected.json` goldens unchanged and passing), the demo slices as needed, and the shipped
  Wikipedia artefact (its host goldens unchanged and passing).
- **FR-008**: The memory-mapped read path (feature `mmap`, ADR-0007) MUST work on version 2,
  including after this handle's own appends (the mapping is refreshed after each commit, as
  the buffer is today); the precondition against external writers is unchanged.
- **FR-009**: The `VectorIndex` trait in `xtriever-core` MUST NOT change (Rule 2): `add`,
  `delete`, `commit`, `search`, `len` keep their signatures; compaction is an inherent
  method on `FlatIndex` the pipeline calls from `merge` (and from `commit` under FR-013).
- **FR-013**: The index configuration MUST gain an optional dead-row share
  (`0..=1`, default unset = off): when set, a `commit` whose resulting dead-row share exceeds
  it compacts the dense file within that commit — synchronously, no thread, no timer
  (Principle III) — and the pipeline's descriptor records the setting. The setting MUST be
  reachable from the FFI as an additive optional field with the same default, so existing
  Python and Swift callers are unaffected.
- **FR-010**: Tests first, committed failing: search goldens generated from the current
  implementation over scripted sequences (add / replace / delete / commit / compact, with
  and without filters, all three metrics); a property test over random sequences comparing
  the two implementations' results bit for bit; the truncation test; the version-1 open and
  conversion tests; the bytes-written test.
- **FR-011**: Performance MUST be measured (Principle IV): a `criterion` benchmark of the
  scan over version 2 against version 1 at 100k rows × 384 dims (budget: within 5 % of
  version 1 for an unfiltered scan; stated in the plan), and the bytes written and wall time
  of a 10-row commit into a 100k-row index against the current implementation.
- **FR-012**: The regenerated artefacts MUST pass their existing parity checks unchanged
  (the fixture goldens, the host goldens, the demo `measure`); no change under
  `crates/xtriever-core`, `deny.toml`, any baseline, `apps/`; the only changes under
  `python/src` and `swift/` are the additive configuration field of FR-013 and the
  regenerated fixture index bytes.

### Key Entities

- **Vector file**: append-only rows `{id, norm, vector}` in commit order; the committed
  length is what the manifest says.
- **Tombstone set**: the dead row indices (a `roaring` bitmap, the workspace's bitset),
  rewritten whole each commit (tens of KB at most for the full Wikipedia index).
- **Manifest**: format version, dim, metric, fingerprint, committed row count, live count,
  and which tombstone file is current — written last, atomically, so it is the truth.
- **Compaction**: the rewrite `merge` performs — the current commit's algorithm, moved —
  also run by `commit` when the configured dead-row share is exceeded.
- **Dead-row share**: `dead rows / committed rows` after a commit; the configured threshold
  in `0..=1`, unset by default.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Adding 10 passages to a 100k-row index and committing writes under 100 KB to
  `dense/` and takes under 50 ms of write time (today: ~150 MB and hundreds of ms); the
  numbers for both formats are in the PR.
- **SC-002**: Bit-identical search results against the current implementation on the
  scripted goldens and on 1,000 random sequences (property test), all metrics, filtered and
  unfiltered; bit-identical before and after `merge`.
- **SC-003**: Every truncation point of a commit's writes reopens to the previous committed
  state (the test enumerates them); the regenerated fixture index passes its committed
  goldens bit for bit; the regenerated Wikipedia artefact passes the host goldens 800/800;
  a version-1 file is refused at open.
- **SC-006**: With the dead-row share set, compaction fires on exactly the commit that
  crosses it (asserted by file rows and tombstone count) and never with it unset.
- **SC-004**: The unfiltered scan over version 2 is within 5 % of version 1 at 100k × 384
  (criterion, both in the PR); RSS of a mapped version-2 index is not above version 1's.
- **SC-005**: `git diff --stat main -- crates/xtriever-core deny.toml apps/ specs/*/baselines`
  is empty; under `python/src` and `swift/` only the additive field and the fixture bytes
  change; the ADR is in `docs/adr/`.

## Assumptions

- **Row layout and file split are plan decisions** (interleaved rows in one file vs.
  columns in three appendable files; measured in the plan's benchmark, not fixed here).
- **DocIds are assigned monotonically and never recycled** by the pipeline's id map
  (`assign` reuses a slot only for the same external id; `remove` reserves the slot), so an
  appended row's id is either new (higher than any committed id) or a replacement of a
  committed one.
- **The pipeline already treats `merge` as compaction** (it commits, then merges the lexical
  segments); this feature gives it the dense half.
- **Regenerating the shipped Wikipedia artefact need not re-embed.** Its 427,947 vectors are
  the pinned embedder's output and do not change; a one-off conversion that copies them from
  the version-1 file into version 2 (a throwaway script, not shipped in the engine) produces
  the same artefact as an 11-hour rebuild, and the host goldens (800/800 bits) prove it. The
  fixture index is regenerated by its existing generator (seconds); the demo slices by their
  builds (~15 min each) or dropped, since they are not committed.
- **No branch or commit by the agent**; the owner creates `024-incremental-dense-commits`
  off `main` and commits at the checkpoints.
