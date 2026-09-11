# Raw device runs — Feature 001

Verbatim `DeviceRunRecord` JSON emitted by `DeviceMeasurementTests`, one file per run. Committed
unedited so every number in `report.md` can be traced to its source (SC-010: a reviewer without an
iPhone must not have to take a measurement on trust).

| file | device | build | verdict |
|---|---|---|---|
| `2026-09-11-run1.json` | iPhone 16e (iPhone17,5), iOS 26.6.1 | Release | PASS |
| `2026-09-11-run2.json` | iPhone 16e (iPhone17,5), iOS 26.6.1 | Release | PASS |
| `2026-09-11-run3.json` | iPhone 16e (iPhone17,5), iOS 26.6.1 | Release | PASS |
| `2026-09-11-run4.json` | iPhone 16e (iPhone17,5), iOS 26.6.1 | Release | PASS |

**Runs 1–2 and 3–4 are not wall-time comparable.** Runs 3–4 were taken after SHA-256 and dtype
verification of the weights was added (F-011 #5), which costs ~34 ms per model load and is inside
the `embed` timings. Memory is unaffected — the hash streams in 1 MiB chunks, and the mmap
footprint delta moved only 2.556 → 2.572 MB.

Runs 3–4 are also the first taken under the stricter verdict gate: `PASS` now requires every
prerequisite (Release, not Simulator, `RAYON_NUM_THREADS=1`, model bundled, valid `task_info`), so
their empty `notes` array is itself evidence.
