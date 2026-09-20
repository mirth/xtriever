# Contract: dense format version 3 (Feature 026)

What a version-3 dense directory is, and what the stage promises about it. Everything not stated
here is unchanged from [version 2](../../024-incremental-dense-commits/contracts/dense-format-v2.md):
the manifest is still the truth, the tombstones are still an embedded bitmap, generations still
advance on compaction, and the commit protocol is still write, sync, rename.

## Files

```text
dense/
├── manifest.bin          magic · header length · JSON header · tombstone bitmap
└── vectors.<g>.bin       fixed-width rows for generation <g>
```

## The header

`format_version` is `3`. The header additionally names the quantisation scheme, so that reading
code never has to infer it. Everything else — dimension, metric, fingerprint, generation, row
count, live count, ordering, tombstone length — keeps its version-2 meaning.

## A row

`id u32 · norm f32 · scale f32 · codes dim×i8`, little-endian, fixed width, 396 bytes at
dimension 384.

- `scale` is strictly positive.
- `codes` lie in [−127, 127]; −128 is never written.
- `component ≈ code × scale`. The stage does not promise exactness and does not keep the floats.

## Refusals

| Situation | Behaviour |
|---|---|
| `format_version` 1 or 2 | refused by name at open, with the rebuild instruction |
| an unknown quantisation scheme | refused by name |
| a row count inconsistent with the file length | refused as corrupt, as in version 2 |
| a scale of zero | refused as corrupt: it cannot have been written by this engine |

## What the stage promises

- **Determinism.** The same index, query and configuration give the same scores and the same
  order, ties broken by ascending identifier.
- **Quality within a stated bound.** Against the same corpus stored as floats, the ranking agrees
  on at least 99 % of the first hundred candidates, and the evaluation metrics stay within 0.005
  of the committed baselines. The stage does **not** promise an identical ranking: the float
  vectors are not kept (spec FR-003).
- **Size.** A file no larger than a third of the float file for the same corpus.
- **Everything version 2 promised** about durability: a crash at any byte boundary leaves either
  the previous committed state or the new one.
