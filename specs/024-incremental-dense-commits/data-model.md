# Data Model: Incremental Dense Commits

## Dense directory (format version 2)

```text
dense/
├── manifest.bin          # magic XTDENSE2 · hdr_len u64 · JSON header · roaring tombstones
└── vectors.<g>.bin       # rows: id u32 · norm f32 · vector dim×f32 — appended in commit order
```

### Manifest header (JSON, key order = on-disk order)

| Field | Type | Meaning / constraint |
|---|---|---|
| format_version | u32 | `2`; any other value → `Corrupt` naming both versions |
| dim | usize | ≥ 1 |
| metric | `cosine` \| `dot` \| `euclidean` | as version 1 |
| fingerprint | string | the embedder fingerprint (agreement checks unchanged) |
| generation | u64 | names the current row file `vectors.<generation>.bin` |
| rows | u64 | committed rows in the row file (≤ `u32::MAX`); file length ≥ `rows × row_bytes`, else `Corrupt` |
| live | u64 | `rows − tombstones.len()`; what `len()` returns |
| tombstones_len | u64 | bytes of the serialised bitmap that follow the header |

### Row

`row_bytes = 4 + 4 + dim × 4`. Row `r` at offset `r × row_bytes`: `id` (a `DocId`), `norm`
(Euclidean, `f32`, as version 1), `vector`. Rows are immutable once committed.

### Tombstone set

`RoaringBitmap` of dead row indices: deleted rows and superseded rows (an earlier row for
an id that was replaced). Invariant: for every live id there is exactly one live row; the
newest row for an id is live unless the id was deleted.

## In-memory state (`FlatIndex`)

| Field | Purpose |
|---|---|
| header | the manifest header |
| rows (bytes) | the row file, buffered or mapped, at its committed length |
| dead | the tombstone bitmap |
| rows_by_id | `Vec<u32>` indexed by id, `u32::MAX` = no live row |
| pending | `BTreeMap<DocId, Option<Vec<f32>>>` — unchanged |
| compaction_threshold | `Option<f32>` in `0..=1`; `None` = only on `compact()` |

## Transitions

| Operation | Row file | Tombstones | Manifest |
|---|---|---|---|
| `add` / `delete` | — | — | — (pending only) |
| `commit` | append k rows | + superseded + deleted | rows += k; live; rename |
| `commit` crossing the threshold | as above, then as `compact` | | |
| `compact` | new file `g+1`, live rows ascending by id | cleared | generation g+1, rows = live; rename; old file removed |
| writable `open` | truncated to `rows × row_bytes` if longer; stale files swept | loaded | read |
| read-only `open` | longer tolerated; shorter → `Corrupt` | loaded | read |

## Pipeline configuration

| Field | Where | Type | Default |
|---|---|---|---|
| `dense_compact_dead_share` | `HybridConfig`, descriptor (`serde(default)`), FFI `IndexConfig` (`uniffi(default = None)`) | `Option<f32>`, `0..=1` else `Error::Schema` | `None` |
