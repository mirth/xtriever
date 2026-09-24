# Report: Eight-bit Precision (Feature 026)

**Feature**: 026 · **Branches**: `026-eight-bit-precision` (PR A, #30), `026-eight-bit-precision-b`
(PR B, #31), `026-eight-bit-precision-c` (PR C) · **Status**: done — eight-bit vectors and
eight-bit model artefacts end to end, quality within the gate on all three datasets, parity on
every platform measured; the phone measured after the feature merged (2026-09-24, below)

## Verdict

The shipped Wikipedia corpus is 585 MB where it was 1,076 MB, its dense vectors 169 MB where
they were 661 MB, and the two models 50 MB on disk where they were 182 MB. Ranking quality is
unchanged to within a thousandth on the full pipeline and within the 0.005 gate everywhere.

Two decisions shaped the result, both the owner's and both taken on measurements recorded here.
The eight-bit weights are **expanded to `f32` at load and multiplied by the float kernel**:
candle's eight-bit kernel was 3.7× slower, and the first choice, `f16`, held on the host but
broke cross-platform parity. And the memory the device ceiling is about is **`phys_footprint`**,
which the host record did not measure until this feature; on it the full corpus peaks at 425–441
MB against the 600 MB ceiling. On resident size, the measure SC-005 is worded on, it is over.

Under `f32` the models occupy the same memory at run time as the float models did. The feature
saves disk, bundle and download size, and the vectors' share of the page cache; it does not
save the models' working memory, and this report does not claim it does.

## Success criteria

| criterion | result | evidence |
|---|---|---|
| SC-001 ranking agreement ≥ 99 % at depth 100 | **PASS** — 99.56 % on SciFact, top hit unchanged on 99.33 % | PR A, on the engine's own rows |
| SC-002 dense file under 200 MB | **PASS** — 660,750,511 → 169,467,012 bytes | `target/xt-wiki/wiki-build.json` |
| SC-003 both model artefacts under 60 MB | **PASS** — 181,738,974 → 50,302,642 bytes (the two GGUF files and the borrowed pooler) | the four manifests |
| SC-004 nDCG@10 and Recall@100 within 0.005 of baseline | **PASS** — largest fall 0.00333 | the table below |
| SC-005 peak resident size below the 600 MB ceiling | **FAIL as worded** — resident 617–639 MB; the ceiling's own measure, `phys_footprint`, is under at 425–441 MB — see below | `runs/measure-…T014434Z….json` |
| SC-006 a pre-feature index or artefact is refused by name | **PASS** — format 1 and 2 refused naming both versions; a directory holding both weights files, or neither, refused naming both | PR A and PR B tests |

**SC-005 needs the owner's reading.** The criterion says "the peak resident size … falls below
the project's 600 MB device ceiling". The ceiling itself (ADR-0010) is defined on the device's
`phys_footprint` — the counter iOS enforces, which the device tests read as their ledger and
which excludes clean file-backed pages. On the host with `f32`:

| measure | peak | vs 600,000,000 bytes |
|---|---|---|
| `phys_footprint`, the ceiling's measure | 424,526,904 – 440,927,312 bytes | under by 159–175 MB |
| resident size (`ru_maxrss`), the criterion's words | 617,136,128 – 639,434,752 bytes | over by 17–39 MB |

Resident size counts the memory-mapped index as it is paged in: it rises steadily through the
80 searches of a run while the footprint does not, and it moved by 22 MB between four idle
runs. The criterion's wording predates the distinction. **As worded, SC-005 fails**, and the
record says so: its `underCeiling` judges resident size and reads `false`, and its
`footprintUnderCeiling` judges `phys_footprint` and reads `true`. Rewording SC-005 onto the
ceiling's measure is the owner's decision; this feature does not make it by changing which
number the verdict reads.

## The quality gate (T024, `compute=f32`)

| configuration | dataset | nDCG@10 base | new | Δ | Recall@100 base | new | Δ |
|---|---|---|---|---|---|---|---|
| dense-baseline-v1 | scifact | 0.64508 | 0.64631 | +0.00123 | 0.92500 | 0.92167 | −0.00333 |
| dense-baseline-v1 | nfcorpus | 0.31667 | 0.31537 | −0.00130 | 0.31145 | 0.31106 | −0.00039 |
| dense-baseline-v1 | fiqa | 0.36867 | 0.36900 | +0.00033 | 0.70606 | 0.70930 | +0.00324 |
| hybrid-rerank-v3 | scifact | 0.72071 | 0.72194 | +0.00123 | 0.95500 | 0.95500 | +0.00000 |
| hybrid-rerank-v3 | nfcorpus | 0.36225 | 0.36247 | +0.00022 | 0.32165 | 0.32099 | −0.00066 |
| hybrid-rerank-v3 | fiqa | 0.39096 | 0.38964 | −0.00132 | 0.70711 | 0.70590 | −0.00121 |

Baselines are the float models' (Features 004 and 015). The same gate under `f16` passed too,
differing from these in the fourth decimal; the arithmetic on top of the artefact's own
rounding does not move quality, which is what the SciFact comparison predicted when it was
chosen.

## PR A — format 3 (merged #30)

A dense row is `id u32 · norm f32 · scale f32 · codes dim×i8`, 396 bytes at dimension 384
against 1,540; queries are quantised by the same scheme, dot products accumulate in `i32`, and
cosine divides by the norms of the two quantised vectors. The floats that were added are not
kept (owner's decision): `vector(id)` returns the row as it recovers. Scan benchmark, 100,000 ×
384: 4.04 ms against 31.5 ms for the version-1 shape — recorded, not claimed. Seven review rounds
and Copilot's.

## PR B — the eight-bit loaders (merged #31)

Both models load from the owner-pinned GGUF artefacts (`leliuga/all-MiniLM-L6-v2-GGUF`,
`cstr/ms-marco-MiniLM-L-6-v2-GGUF`) beside the float models' `config.json` and `tokenizer.json`;
the re-ranker's artefact lacks the pooler, which is cut byte for byte out of the pinned float
weights into a pinned `pooler.safetensors` (owner's decision). A shared BERT encoder over the
GGUF tensors and a shared header reader are byte-identical in the two stage crates, pinned so by
a test. Every consumer defaults to the eight-bit directories. Three review rounds and Copilot's.

## PR C — the arithmetic, the regeneration and the records

**`f32`, not `f16`** (owner's decision, 2026-09-22; ADR-0015 records both). `f16` was chosen
first for half the models' memory. On the Android emulator — the same arm64 architecture as the
host — it disagreed with macOS-minted goldens on 36 of 800 hits and by 0.0134 in a re-rank
score, where the float models had agreed on all 800 within 5.2e-6 (Feature 025). Under `f32` the
emulator agrees on all 800, dense scores identical to the bit, re-rank within 6.7e-6. The cost is
39 MB of the two models' memory; speed, disk and bundle are unchanged.

**Everything minted under `f16` was regenerated**: the three evaluation caches and the gate, the
fixture index and its goldens, the Android slice and its goldens, the Wikipedia corpus (745.9
minutes) and its host goldens, the host measurement.

**Parity on every platform measured**:

| platform | record | result |
|---|---|---|
| host, full corpus | `runs/measure-…T014434Z….json` | 800 of 800 hits bit-identical |
| Android emulator, library fixture | instrumented tests | 6 of 6 |
| Android emulator, 2,000-article slice | `runs/android-sdk_gphone64_arm64-….json` | 800 of 800, re-rank within 6.7e-6 |
| iOS device, full corpus | `runs/iPhone17,5-20260924T053922-….json` | measurement PASS; no parity comparison in the demo's run |
| iOS device, 40-document fixture | the demo's `DemoModelTests` | ids, order, lexical, dense and fused bits identical; re-rank within 3.8e-6 (F-007) |
| iOS simulator, fixture and 3,922-article slice | the demo's tests | bit-identical, 21 of 21 |
| Android emulator, 2,000-article slice, after Feature 027 | `runs/android-sdk_gphone64_arm64-20260924T054727Z-….json` | 800 of 800, re-rank within 6.7e-6 again |

The iOS rows were added on 2026-09-24, after the feature merged: there was no device while it
was built.

**The host record carries the ceiling's own counter beside resident size** (the demo's
`measure`, Feature 019's record contract amended): `peakBytes` and `underCeiling` keep their
meaning, the resident peak and its verdict; `footprintPeakBytes`, `footprintMethod` and
`footprintUnderCeiling` add the lifetime peak `phys_footprint` and a verdict on it. Every figure
prints in decimal megabytes, the ceiling's unit.

## Size, memory and time

| | format 2, float models | format 3, eight-bit models (`f32`) |
|---|---|---|
| Wikipedia artefact | 1,076,413,167 B | 585,130,234 B |
| dense vectors | 660,750,511 B | 169,467,012 B |
| model weights on disk | 181,738,974 B | 50,302,642 B |
| host peak `phys_footprint` | not recorded | 424.5 – 440.9 MB |
| host peak resident size | 1,062,453,248 B | 617.1 – 639.4 MB |
| host median latency, fused / depth 10 / depth 20 | 250 / 958 / 1,685 ms | 123–142 / 841–989 / 1,514–1,750 ms |
| Wikipedia build | 11.1 h (4 threads) | 12.4 h |

The fused search roughly halves, consistent with the scan reading a quarter of the bytes. The build is no faster:
it embeds one text at a time on about four cores, which neither format nor precision changes.

**On the phone** (iPhone 16e, the demo app in Release, re-rank depth 10, 20 measurement
queries; measured 2026-09-24 against Feature 018's float-model record on the same phone):

| | float models, format 2 (018) | eight-bit models, format 3 | the same, 3,922-article slice |
|---|---|---|---|
| record | `specs/018-…/runs/iPhone17,5-20260916T192434-….json` | `runs/iPhone17,5-20260924T053922-….json` | `runs/iPhone17,5-20260924T103751-….json` |
| passages | 427,947 | 427,947 | 19,998 |
| peak `phys_footprint` (600 MB ceiling) | 305.0 MB | 335.4 MB | 296.5 MB |
| median fused / re-ranked / total | 341.5 / 1,369 / 1,704.5 ms | 200.5 / 1,213 / 1,408.5 ms | 156 / 1,287 / 1,452.5 ms |
| index open | 760 ms | 736 ms | 452 ms |

The phone agrees with the host: the fused search is 41% faster, and the whole search 17%, under
the ceiling with 265 MB to spare. The slice (the demo's `--with-wiki-slice`) opens faster and
fuses faster; the re-ranker's ten calls, which do not depend on the corpus's size, are most of
every search.

## Findings

- **F-001 — `f16` activations break cross-platform parity.** Eleven significant bits turn the
  seventh-decimal differences between two platforms' kernels into third-decimal ones, and six
  blocks amplify them into rank flips among near ties. A precision choice is a parity choice.
- **F-002 — The host footprint measured the wrong quantity, in the wrong unit.** The demo
  recorded `ru_maxrss`, which counts clean mapped index pages the ceiling excludes, and printed
  mebibytes as "MB" beside a ceiling in millions of bytes. That mislabelling fed an estimate that
  `f32` would stay under the ceiling on resident size; it did not, and the owner chose `f32` on
  that estimate. The demo now records `phys_footprint` beside resident size, each with its own
  verdict, in decimal megabytes; the mebibyte figures quoted elsewhere in this feature were
  corrected. A first version of the fix made the verdict read `phys_footprint` instead of
  resident size, which would have turned SC-005 from FAIL to PASS without amending it; review
  caught it, and the verdict judges the criterion as worded.
- **F-003 — Eight-bit artefacts save disk, not run-time model memory, under `f32`.** The expanded
  matrices are the float models' size. The vectors' saving is page cache: with the index mapped,
  vectors are clean pages.
- **F-004 — The re-ranker artefact lacks the pooler.** Borrowed from the pinned float weights,
  pinned by hash, named in the model identity.
- **F-005 — candle's eight-bit kernel is built for one token at a time**: 442 ms per 256-token
  embedding against 125 ms for the float model. A kernel of our own is out of scope (Principle I).
- **F-006 — The corpus build is no faster**: 12.4 h, embedding one passage at a time.
- **F-007 — On the phone, the eight-bit re-ranker's scores differ from the host's in the last
  bits** (found 2026-09-24, the first device run since this feature): at most 3.8e-6 on the
  40-document fixture, with ids, order and every other score bit-identical. The same size as the
  Android emulator's 6.7e-6. The demo's golden test demanded identical bits everywhere and failed
  on the phone; by the owner's decision it now allows 1e-5 on re-rank scores on a device, and
  stays bit-exact on the simulator. The package's device measurement already allowed 1e-3.

## Deliberately not done

No rescoring pass over float vectors (owner: the measured 0.0006 was accepted for a small file),
no four-bit, no neural accelerator, no arbitrary user-supplied models, no eight-bit kernel of our
own. No device measurement while the feature was built — there was no device; the phone was
measured afterwards (above).
