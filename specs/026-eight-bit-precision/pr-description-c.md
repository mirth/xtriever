## 026 (PR C) — `f32` over the eight-bit weights, the regenerated corpus, and a footprint measured on the ceiling's own counter

PR B loaded both models from their eight-bit artefacts and multiplied in `f16`. This pull
request replaces `f16` with `f32`, regenerates everything minted under `f16`, and finishes the
feature: the quality gate on three datasets, the Wikipedia corpus, the host measurement, the
demonstrations' checks, the documentation and the report
([`report.md`](./report.md)).

**Why `f32`** (owner's decision, 2026-09-22; [ADR-0015](../../docs/adr/0015-eight-bit-vectors-and-models.md)
records both choices). `f16` held on the host — the host's goldens agreed bit for bit at 1, 4
and 10 threads — and failed across platforms: the Android emulator, the same arm64 architecture,
disagreed with macOS-minted goldens on **36 of 800** hits and by 0.0134 in a re-rank score, where
the float models had agreed on all 800 within 5.2e-6 (Feature 025). An `f16` activation carries 11
significant bits; the seventh-decimal differences between two platforms' kernels become
third-decimal ones and six blocks amplify them into rank flips among near ties. Under `f32` the
emulator agrees on **all 800**, dense scores identical to the bit, re-rank within 6.7e-6. The code
change is one line in each encoder and the fingerprints' `compute=f32`; the cost is 39 MB of the
two models' memory, with speed, disk and bundle unchanged.

**The quality gate, `compute=f32`, against the float baselines:**

| configuration | dataset | nDCG@10 Δ | Recall@100 Δ |
|---|---|---|---|
| dense-baseline-v1 | SciFact | +0.00123 | −0.00333 |
| dense-baseline-v1 | NFCorpus | −0.00130 | −0.00039 |
| dense-baseline-v1 | FiQA | +0.00033 | +0.00324 |
| hybrid-rerank-v3 | SciFact | +0.00123 | +0.00000 |
| hybrid-rerank-v3 | NFCorpus | +0.00022 | −0.00066 |
| hybrid-rerank-v3 | FiQA | −0.00132 | −0.00121 |

All within 0.005; the reports are under `runs/`.

**The corpus**: 427,947 passages, **585,130,234 bytes** (1,076,413,167 in format 2), dense vectors
169,467,012 bytes (660,750,511), verification PASS, host goldens re-minted. The fixture index,
the Android slice and both sets of goldens were re-minted too.

**The footprint, and a measurement that was wrong.** The demo's `measure` recorded `ru_maxrss`
and printed it in mebibytes labelled MB beside a ceiling of 600,000,000 bytes. Resident size
counts the memory-mapped index's clean pages, which iOS does not count and ADR-0010's ceiling
excludes: the ceiling is defined on `phys_footprint`, the counter the device tests read as their
ledger. A timeline of a full run shows the difference — resident size climbs through the 80
searches as the index is paged in, the footprint does not:

| host, full corpus, `f32` | peak |
|---|---|
| `phys_footprint` (the ceiling's measure) | 424.5 – 440.9 MB — under by 159–175 MB |
| resident size (SC-005's wording) | 617.1 – 639.4 MB — over by 17–39 MB, varying between runs |

`measure` now records `phys_footprint` beside resident size, each with its own verdict:
`peakBytes`/`underCeiling` keep their meaning (resident size, as in every earlier record and in
each query's `peakBytesAfter`), and `footprintPeakBytes`/`footprintMethod`/`footprintUnderCeiling`
add the ceiling's measure (macOS's `proc_pid_rusage`; `null` where a platform has none).
Everything prints in decimal megabytes; Feature 019's record and command contracts say so, and
the mebibyte figures quoted as MB elsewhere in this feature are corrected. **SC-005 is worded on
resident size and fails as worded** — the committed record's `underCeiling` is `false` — while
the ceiling's own measure passes; rewording it is the owner's call, and this pull request does
not make it by changing which number the verdict reads.

**Under `f32` the eight-bit models save disk, not run-time memory**: the expanded matrices are the
float models' size. The report says so rather than claiming a memory saving.

**Parity**: host 800/800 bit-identical; Android library 6/6 and the demo's measurement 800/800 on
the emulator. **Not run: iOS** — no device (owner).

**Latency, host medians** over three idle runs: fused 123–142 ms (250 in format 2), depth 10
841–989 ms (958), depth 20 1,514–1,750 ms (1,685). Recorded, not claimed.

**Also**: the demo's `--help` quotes its defaults from the defaults table (it named the float
directories); the crate docs, the artefact contract and every README quoting sizes updated
(T030, T031); a rustdoc link to a private item fixed.

**Gate** (the full local gate, 2026-09-23): fmt clean; clippy with warnings denied clean;
`cargo nextest run --workspace` **365 passed**; iOS, iOS simulator and Android cross-checks pass
(wasm fails as tracked); `cargo deny check` advisories, bans, licenses and sources ok; goldens
and oracle, 0 differences; the Python surface on a rebuilt wheel **34 passed**, the Wikipedia
demo **96 passed** (the chonky tests run), the minimal demo 6 passed; the re-ranker's
model-backed suite **35 passed**. Leak scan clean.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
