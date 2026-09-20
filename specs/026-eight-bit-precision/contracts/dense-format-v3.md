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

`format_version` is `3`. The header additionally names the quantisation scheme — the key
`scheme`, whose value for this feature is `i8-symmetric-per-vector` — so that reading code never
has to infer it. Everything else — dimension, metric, fingerprint, generation, row count, live
count, ordering, tombstone length — keeps its version-2 meaning. The dimension is at most
132,104, the widest row whose integer dot product fits the accumulator even when a byte on disk is −128.

## A row

`id u32 · norm f32 · scale f32 · codes dim×i8`, little-endian, fixed width, 396 bytes at
dimension 384.

- `scale` is `max|component| / 127`, floored at the smallest normal `f32` (about 1.2e-38) so
  that it is always a normal, strictly positive number: a denormal peak would otherwise store a
  zero scale that no reader could tell from corruption.
- `codes` are `component / scale` rounded **half away from zero** and clamped to [−127, 127];
  −128 is never written. A reader takes a row byte of −128 as the code −128 and scores it: the
  format carries no checksum over the codes, so a damaged code byte is indistinguishable from a
  written one whatever its value, and refusing the one value the engine never writes would
  cost a pass over every row to catch one damage pattern in 256. The dimension bound below
  counts that byte, so the integer accumulator cannot overflow on it.
- `norm` is the norm of the row **as stored** — `sqrt(Σ code²) × scale` — not of the floats that
  were added, which are not kept. Cosine divides by it and by the same norm of the quantised
  query, so it is the cosine of what is actually compared: a row's cosine with itself is one,
  and no cosine exceeds one beyond the final rounding to `f32`.
- `component ≈ code × scale`, within half a scale. The stage does not promise exactness and does
  not keep the floats.
- A vector whose every component is below half the scale floor is zero at eight-bit precision.
  Under Cosine it is refused at `add`, as a zero vector is; under Dot and Euclidean it is stored
  as all-zero codes.

## Refusals

| Situation | Behaviour |
|---|---|
| `format_version` 1 or 2 — by the magic (`XTDENSE1`, `XTDENSE2`) or by the header | refused by name at open, with the rebuild instruction |
| an unknown quantisation scheme, or none | refused by name at open, naming the scheme this build reads |
| a dimension beyond 132,104 | refused at create and at open |
| a row count inconsistent with the file length | refused as corrupt at open, as in version 2 |
| a scale that is not a normal positive number (zero, denormal, negative, infinite, NaN) | refused as corrupt **by the first reader that reaches the row** — a search, or `vector(id)` on that row: it cannot have been written by this engine, and a NaN score would make the order arbitrary |
| a norm that is not finite, or under Cosine not positive | the same |
| a damaged code byte | **not detected** (no checksum): the row scores with the damaged code, including −128 |

The last two are caught by the readers rather than at open because an open reads nothing beyond
the manifest (Feature 024): a read-only open of a shipped index must not page in every row. A
search whose filter never reaches the damaged row is unaffected, and `vector(id)` on an intact
row still recovers it.

## What the stage promises

- **Determinism.** The same index, query and configuration give the same scores and the same
  order, ties broken by ascending identifier.
- **Quality within a stated bound.** Against the same corpus stored as floats, the ranking agrees
  on at least 99 % of the first hundred candidates, and the evaluation metrics stay within 0.005
  of the committed baselines. The stage does **not** promise an identical ranking: the float
  vectors are not kept (spec FR-003). **How it is measured**, since the index holds no floats:
  the evaluation harness keeps the embedder's own output beside each dataset's cache
  (`vectors.f32.bin`, `beir run --config dense-baseline-v1`), and
  `reference/int8_vectors_study.py` ranks a dataset's judged queries over those floats and over
  the engine's stored rows — after first checking that the rows are the scheme, byte for byte —
  and reports the agreement, the top-10 kept, and both rankings' nDCG@10 and Recall@100.
- **Size.** A file no larger than a third of the float file for the same corpus.
- **Everything version 2 promised** about durability: a crash at any byte boundary leaves either
  the previous committed state or the new one.
