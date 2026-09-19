# Contract: dense format version 2 and `FlatIndex`'s write protocol

## Files

See [data-model.md](../data-model.md). `manifest.bin` is the truth; `vectors.<g>.bin` holds
`rows` committed rows and possibly a tail beyond them that is not part of the index.

## `FlatIndex` (public, `xtriever-dense`)

Unchanged signatures: `create(dir, dim, metric, fingerprint)` (a failed create leaves the directory empty for a retry), `open(dir)`,
`open_mapped(dir)` (feature `mmap`), `open_for` / `open_mapped_for(dir, embedder)`, `dir()`,
`vector(id) -> Option<Vec<f32>>`, and the `VectorIndex` impl (`dim`, `metric`, `fingerprint`,
`add(id, vector)`, `delete(ids)`, `commit()`, `search(query, allowed, k)`, `len()`).

New:

- `open_read_only(dir)`, `open_read_only_for(dir, embedder)` and, under `mmap`,
  `open_mapped_read_only(dir)`, `open_mapped_read_only_for(dir, embedder)`: a read-only open
  touches nothing in the directory — no crashed tail is cut, no stale generation or manifest
  temporary swept — reads exactly the committed rows, and refuses `commit` / `compact` with `Error::Io` (permission denied, "read-only
  index"). `is_read_only()` tells. The pipeline uses these for `OpenOptions { read_only: true }`.
  The no-concurrent-writer precondition of every open (spec edge cases; `OpenOptions`) stands
  for read-only opens too.
- `compact(&mut self) -> Result<()>`: commits pending changes, then rewrites the row file
  with live rows only in ascending id order under a new generation; a no-op when there is
  nothing dead and the rows are already ascending. On a read-only handle → `Error::Io`
  "read-only index" (the pipeline refuses `merge` on a read-only open even earlier); on a
  directory that cannot be written the underlying `Error::Io` surfaces.
- `set_compaction_threshold(&mut self, share: Option<f32>) -> Result<()>`: `Error::Schema`
  unless `share` is `None` or in `0.0..=1.0`; when set, a `commit` whose resulting
  `dead / rows` would exceed it is performed *as a rewrite* (the live rows with the pending
  changes folded in, one manifest rename) instead of an append — one protocol, so it fails
  whole or succeeds whole, never as a durable append followed by a separate compaction.
- `stats(&self) -> DenseStats { rows, live, dead, generation }` for tests and records;
  `bytes_written(&self)`: bytes this handle wrote (each `write_all` counted once it
  succeeded, before the sync — a rolled-back append still wrote its rows); `is_sync_pending(&self)`; `is_read_only(&self)`.
- Every writable `commit` / `compact` first verifies the manifest on disk is the one this
  handle last saw (generation, rows) and refuses with `Error::Corrupt` "changed by another
  writer" otherwise.

Errors: a version-1 `index.bin` directory (no `manifest.bin`) or any other version →
`Error::Corrupt` naming the found and the expected version; a header whose `rows × (8 + dim
× 4)` overflows, a row file shorter than `rows × row_bytes`, or two live rows for one id →
`Error::Corrupt`; the agreement errors as before.

## Guarantees

1. `commit` writes exactly: the appended rows (`k × row_bytes`), one manifest
   (16 + header + tombstones bytes), and nothing else; no committed byte changes
   (`bytes_written` counts them).
2. Search results are bit-identical to what version 1 returned for the same committed
   content, filtered or not, all metrics; before and after `compact`.
3. A crash at any byte boundary of `commit` or `compact` reopens to either the previous
   committed state or the fully committed new one — the manifest rename is the switch — never
   to a partial state; consumers cannot rely on a rollback after the switch. An error from either leaves the handle coherent with the disk: before the manifest
   rename nothing changed and the pending changes are kept for a retry; after it (only the
   directory sync can fail there) the new state is adopted, the error says so,
   `is_sync_pending()` is true, and no later `commit` or `compact` returns success until a
   directory sync has succeeded (each retries it first). On Unix targets (the engine's shipping targets) the directory is fsynced at the
   protocol's ordering points, so a power loss keeps the manifest and the row file it names
   consistent; on other targets that ordering is the filesystem's.
4. A mapped byte is never modified or truncated by this crate; the row file is only extended
   beyond every live mapping or replaced by rename (ADR-0013 amends ADR-0007 condition 2).

## Pipeline and FFI

- `HybridIndex::merge`: `commit`, then `dense.compact()`, then the lexical merge.
- `HybridConfig.dense_compact_dead_share: Option<f32>`; `HybridConfig::new` leaves it
  `None`; `validate` refuses values outside `0..=1`; the descriptor records it
  (`#[serde(default)]`, pipeline format version unchanged).
- FFI `IndexConfig.dense_compact_dead_share: Option<f32>` (`#[uniffi(default = None)]`),
  mapped 1:1; `IndexInfo` unchanged.
