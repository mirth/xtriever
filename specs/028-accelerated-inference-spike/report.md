# Report: Accelerated Inference Spike (Feature 028)

**Feature**: 028 · **Branch**: `028-accelerated-inference-spike` · **Status**: measured —
done — **go** for the Apple GPU on the iPhone and on the laptop, and for Apple's matrix
library on the laptop; **no-go** for the matrix library on the iPhone and for every batched
variant

## Verdict

**Adopt the Apple GPU (Metal) for the embedder and the re-ranker on Apple devices, as a
follow-up feature.** On the iPhone 16e it cuts the re-ranked phase of a search from 1,316 ms to
435.5 ms (−67%) and the first list from 217.5 to 115 ms, keeps peak memory at 423–472 MB under
the 600 MB ceiling, repeats exactly, moves no dense score and no re-rank score by more than
8.2e-6, and leaves nDCG@10 and Recall@100 unchanged to six decimals on SciFact, NFCorpus and
FiQA. On the laptop it cuts the re-ranked phase by 77% and builds an index 4.1× faster. Its costs
are a one-time kernel compilation on the first search after an install (35.5 s on the phone) and
about 90–135 MB of memory. **Apple's matrix library (Accelerate)** passes on the laptop (−47%
re-ranking, ×3.9 build throughput, the same unchanged quality) but gives the phone only −17%,
under the 30% the rule asks. **Batching the re-ranker is rejected on every path**: slower on the
phone's CPU paths, and over the memory ceiling on every phone variant's harness run.

For the follow-up's spec, verbatim: *compute path `metal` (candle 0.9.2's `metal` feature,
`Device::new_metal(0)` in both model loaders, one pair at a time) for the embedder and the
re-ranker on iOS and macOS; measured on an iPhone 16e (iOS 26.6.2) at −66.9% median re-ranked
phase, peak 422.9–471.5 MB, re-rank scores within 8.2e-6 of the CPU goldens, dense scores
bit-identical, nDCG@10 / Recall@100 unchanged on all three BEIR datasets, first search after
install 35.5 s (kernel compilation, cached by the system after). The follow-up must add: a
fallback to the CPU path when no GPU device can be opened (Principle VI); a decision on whether
the compute path joins the embedder's identity — the measurements favour leaving it out (dense
scores identical, a GPU-built index searched on the CPU scores identically); a way to pay the
first-launch compilation where the person does not wait for it; a `criterion` bench; the
Accelerate path as an option for macOS hosts and builds; and the removal of the `spike-*`
features and harness labels this spike added (FR-012).*

## What was measured

Three compute paths for the embedder and the re-ranker — today's CPU path, the CPU path through
Apple's matrix library (`spike-accelerate`) and the Apple GPU (`spike-metal`) — each also with the
re-ranker scoring a query's pairs in one batch (`spike-batch`), on the iPhone 16e (`iPhone17,5`,
iOS 26.6.2) and the 2021 MacBook Pro (`MacBookPro18,3`, M1 Pro), against the whole Simple English
Wikipedia index (427,947 passages) and its 20 measurement queries. Every run record is under
[`runs/`](runs/) and names its path inside it. Every build, link and run succeeded: there is no
`runs/errors.md`. All phone runs were at thermal state `nominal`.

The spike's CPU runs are the reference, measured the same day: the phone's re-ranked phase took
1,316 ms, 8% more than Feature 026's 1,213 ms record three days earlier on the same phone and
code — run-to-run spread, which is why every comparison below is against the same day's CPU run.

## Latency (User Story 1)

**iPhone 16e, the demo's measured run** (median over 20 queries; re-rank depth 10, the app's):

| path | batch | fused list | re-ranked phase (max) | whole search (max) | vs CPU re-ranked |
|---|---|---|---|---|---|
| CPU | — | 217.5 ms | 1,316 ms (1,595) | 1,529.5 ms (1,809) | — |
| Accelerate | — | 196 ms | 1,086.5 ms (1,371) | 1,279 ms (1,556) | **−17.4%** |
| Metal | — | 115 ms | 435.5 ms (1,220) | 542.5 ms (1,332) | **−66.9%** |
| Metal, relaunched (runs 2–3) | — | 104–105 ms | 415.5–416 ms (509–534) | 513.5–520 ms (612–627) | −68.4% |
| CPU | batch | 201 ms | 1,638.5 ms | 1,830.5 ms | +24.5% |
| Accelerate | batch | 194.5 ms | 1,526.5 ms | 1,721.5 ms | +16.0% |
| Metal | batch | 105.5 ms | 763 ms | 869.5 ms | −42.0% |

**iPhone 16e, the Swift package harness** (mean per depth; two runs for the unbatched paths):

| path | batch | depth 0 | depth 5 | depth 10 | depth 20 |
|---|---|---|---|---|---|
| CPU | — | 246 / 232.75 | 756.3 / 753.3 | 1,266.15 / 1,281.65 | 2,294.5 / 2,641.55 |
| Accelerate | — | 210.3 / 177.4 | 631.8 / 595.25 | 985.2 / 990.05 | 1,761.3 / 1,760.55 |
| Metal | — | 1,251.65 / 114.15 | 335.85 / 262.45 | 420.35 / 433.1 | 752.15 / 751.85 |
| CPU | batch | 235.35 | 906.4 | 1,645.05 | 3,242.35 |
| Accelerate | batch | 227.8 | 867.1 | 1,608.05 | 3,080.65 |
| Metal | batch | 120 | 385.4 | 756.05 | 1,558.05 |

(ms. Metal's first depth-0 mean includes compiling the GPU kernels on the first search.)

**MacBook Pro, the Python demo's `measure`** (median over 20 queries; two runs unbatched):

| path | batch | fused list | re-ranked, depth 10 | re-ranked, depth 20 | whole search | vs CPU depth 10 |
|---|---|---|---|---|---|---|
| CPU | — | 130 / 129.5 | 894 / 856 | 1,637.5 / 1,592.5 | 1,030.5 / 979 | — |
| Accelerate | — | 81.5 / 80 | 457.5 / 477.5 | 811 / 818.5 | 541 / 558.5 | **−46.6%** |
| Metal | — | 48 / 47 | 202.5 / 206 | 361 / 368.5 | 252 / 254 | **−76.7%** |
| CPU | batch | 134 | 1,083.5 | 1,986 | 1,217.5 | +23.8% |
| Accelerate | batch | 85.5 | 635.5 | 1,209 | 728 | −27.4% |
| Metal | batch | 47 | 306.5 | 618.5 | 354 | −65.0% |

(ms; "vs CPU" against the mean of the two CPU runs, 875 ms.)

**The first search after installing the Metal build** compiles the GPU kernels: 35.5 s on the
phone after a fresh install (23.6 s in the first run, after an earlier Metal build had been
installed), 11.3 s on the laptop after a fresh build. Every later launch took 0.41–0.45 s on the
phone and 95 ms on the laptop: the system keeps the compiled kernels
([`…-metal-single-demo-launch{1,2,3}-….json`](runs/)).

## Cost: memory, numbers, repeatability (User Story 2)

| device | path | batch | peak memory | largest dense difference | largest re-rank difference | repeatable |
|---|---|---|---|---|---|---|
| iPhone 16e | CPU | — | 334.8–337.2 MB | 0 | 5.7e-6 | yes |
| iPhone 16e | Accelerate | — | 334.5–336.1 MB | 0 | 5.5e-6 | yes |
| iPhone 16e | Metal | — | 422.9–471.5 MB | 0 | 8.2e-6 | yes |
| iPhone 16e | CPU | batch | 595.1 / **886.7 MB** | 0 | 5.3e-6 | one run |
| iPhone 16e | Accelerate | batch | 571.8 / **835.5 MB** | 0 | 3.8e-6 | one run |
| iPhone 16e | Metal | batch | **817.9 / 1,298.4 MB** | 0 | 6.7e-6 | one run |
| MacBook Pro | CPU | — | 452–453 MB | 0 | 0 (800/800 hits bit-identical) | yes |
| MacBook Pro | Accelerate | — | 428–445 MB | 0 | 5.5e-6 | yes |
| MacBook Pro | Metal | — | 483–493 MB | 0 | 8.1e-6 | yes |
| MacBook Pro | batch (all) | batch | 1,380–2,022 MB | 0 | ≤ 7.2e-6 | one run |

- **Peak memory** is `phys_footprint` (the phone's ledger, the laptop's counter); the phone's
  ceiling is 600 MB (ADR-0010). Bold: over it.
- **Differences** are against the host's goldens (`target/xt-wiki/expected.json`), matched by
  id, all within the 1e-3 tolerance. On every path and device the lexical scores are
  bit-identical (20 of 20 queries), the fused order at depth 0 is identical (20 of 20), and every
  dense score matches the host's bit for bit.
- **Order**: on the laptop, the hit order of Accelerate and Metal equals the goldens' at depths
  0, 5, 10 and 20 for all 20 queries ([`order-host.json`](runs/order-host.json)). The phone's
  harness compares scores by id and the fused order at depth 0 only, so the phone's order at the
  re-ranked depths was not measured; its re-rank differences (≤ 8.2e-6) are the same size as the
  laptop's.
- **Repeatable**: two runs of the same build on the same device gave equal hits digests
  (every id and every score bit). Each batched variant was run once.
- **Batched vs one pair at a time**, same pairs (`spike_batch` test): within 4.3e-6 (CPU),
  5.7e-6 (Accelerate), 3.0e-6 (Metal).
- **Golden tests** (the existing model-backed suites of both crates, against the Python
  reference at their stated tolerances): 205 of 205 pass on each path on the laptop
  ([`goldens-*.txt`](runs/)).

## Quality (User Story 2, FR-008)

The full pipeline (`hybrid-rerank-v3`) evaluated on the laptop with each path that passed the
rule's first three conditions, every corpus embedded afresh on that path, against Feature 026's
CPU records ([`runs/{accelerate,metal}.hybrid-rerank-v3.*.json`](runs/)):

| dataset | CPU nDCG@10 | Accelerate | Metal | CPU Recall@100 | Accelerate | Metal |
|---|---|---|---|---|---|---|
| SciFact | 0.721936 | 0.721936 (±0) | 0.721936 (±0) | 0.955000 | 0.955000 (±0) | 0.955000 (±0) |
| NFCorpus | 0.362470 | 0.362470 (±0) | 0.362470 (±0) | 0.320988 | 0.320988 (±0) | 0.320988 (±0) |
| FiQA | 0.389641 | 0.389641 (±0) | 0.389641 (±0) | 0.705902 | 0.705902 (±0) | 0.705902 (±0) |

No change on any dataset, to the six decimals the harness prints. The two passes ran at the same
time, each with its own build directory (running them in one would let either rebuild the
other's harness between its build and its run); they record quality, not time.


## Building an index (User Story 3)

SciFact's 5,183 passages embedded on the laptop, the harness's `dense-baseline-v1` on a fresh
cache minus the same run warm ([`build-*.json`](runs/)):

| path | embedding time | passages per second | vs CPU | Wikipedia build, scaled from today's 12.4 h |
|---|---|---|---|---|
| CPU | 2,509.8 s | 2.07 | — | 12.4 h |
| Accelerate | 642.4 s | 8.07 | **×3.90** | ~3.2 h |
| Metal | 607.1 s | 8.54 | **×4.13** | ~3.0 h |

The records' own `projectedWikipediaHours` (57.6 / 14.7 / 13.9 h) divide by SciFact's rate and
are not used: the harness's embedding loop runs about 4.6× slower per passage than the command
line's Wikipedia build does (2.07 against about 9.6 passages per second, on the CPU). Why was not
investigated; only the ratio between paths on the same harness is used.

**The mixed case** — SciFact embedded and indexed on the GPU, then searched on the CPU path:
nDCG@10 **0.721936**, Recall@100 0.955, equal to the all-CPU record to every printed digit
([`mixed-scifact.json`](runs/mixed-scifact.json)).

## The decision rule (FR-010), applied

| device | path | batch | 1 · faster | 2 · under 600 MB | 3 · repeatable | 4 · within tolerance | quality | verdict |
|---|---|---|---|---|---|---|---|---|
| iPhone 16e | Metal | — | −66.9% ✓ | 422.9–471.5 MB ✓ | ✓ | 8.2e-6 ✓ | unchanged ✓ | **go** |
| iPhone 16e | Accelerate | — | −17.4% ✗ | 334.5–336.1 MB ✓ | ✓ | 5.5e-6 ✓ | unchanged | **no-go** (speed) |
| iPhone 16e | CPU | batch | +24.5% ✗ | 595.1 / 886.7 MB ✗ | one run | 5.3e-6 ✓ | — | **no-go** |
| iPhone 16e | Accelerate | batch | +16.0% ✗ | 571.8 / 835.5 MB ✗ | one run | 3.8e-6 ✓ | — | **no-go** |
| iPhone 16e | Metal | batch | −42.0% ✓ | 817.9 / 1,298.4 MB ✗ | one run | 6.7e-6 ✓ | — | **no-go** (memory) |
| MacBook Pro | Metal | — | −76.7%, build ×4.13 ✓ | (no ceiling) | ✓ | 8.1e-6 ✓ | unchanged ✓ | **go** |
| MacBook Pro | Accelerate | — | −46.6%, build ×3.90 ✓ | (no ceiling) | ✓ | 5.5e-6 ✓ | unchanged ✓ | **go** |
| MacBook Pro | CPU | batch | +23.8% ✗ | 2,016 MB | one run | 5.1e-6 ✓ | — | **no-go** |
| MacBook Pro | Accelerate / Metal | batch | −27.4% / −65.0%, but slower than unbatched | 2,022 / 1,380 MB | one run | ✓ | — | **no-go** (dominated) |

"Faster" is the median re-ranked phase against the same day's CPU run (or, on the laptop, build
throughput ×2). The phone's quality is the laptop's evaluation of the same path: the BEIR
datasets are evaluated on the host (FR-008), and the phone's scores differ from the host's by at
most 8.2e-6 with identical dense scores. The batched laptop variants that are faster than the CPU
are still slower than the same path unbatched and were run once, so they are not recommended.

## Limits of the spike's builds (review)

A code review of the finished spike found these; none changes a recorded number, and each is the
follow-up's to settle:

- **The compute-path label is the build's, not the library's.** The phone reads the label the
  packager staged beside the framework; the laptop's `measure` takes it from a flag. Both were
  set by the same command that chose the features, so this spike's records are right, but a
  plain `wikidemo measure` over a leftover spike wheel would record `cpu`. The engine does not
  report its own path through the bindings; adding it changes a binding-visible record, which
  the spike avoided (FR-011). The follow-up should expose the path from the engine.
- **The label is narrower than what it covers.** The sparse encoder stays on the CPU under
  `spike-metal`; and Cargo unifies features, so `spike-accelerate` on one model crate reroutes
  every candle matrix multiply in the build. Every run here enabled the same feature on both
  crates (the packager and the host script do), so the records are consistent.
- **Identity does not follow the arithmetic in a spike build.** A Metal-built index records the
  CPU build's fingerprint, against the rule in `quantised_bert.rs` that the arithmetic and the
  identity cannot drift apart. It is deliberate (research D6: a CPU-built index must open on the
  GPU path), and the evidence for the follow-up's decision is above — dense scores bit-identical
  on every path and the mixed case identical to the CPU's quality.
- **Batching has no cap.** All of a call's pairs go into one padded tensor; at re-rank depth 100
  with 512-token pairs the attention scores alone would be about 1.26 GB. The measured depth-20
  runs already put batching over the ceiling (F-004); the follow-up drops batching.
- **The batched time budget was only checked for zero** until this review: a pass that ran past a
  non-zero limit reported every pair as scored. It now discards the pass (all or none), with a
  test (`a_batch_that_overruns_its_budget_scores_nothing`). No recorded run was affected — the
  demo's budget is 4 s and the slowest batched re-ranked phase took 1.8 s (1,790 ms); the harnesses set no budget.

## Findings

- **F-001 — On the phone the GPU is the win; Apple's matrix library is not.** Metal cut the
  re-ranked phase by 67% and the fused list by 47%; Accelerate by 17% and 10%. On the laptop
  Accelerate cut the re-ranked phase by 47% — the M1 Pro gains far more from it than the A18 does.
- **F-002 — Metal's first search after an install compiles its kernels:** 35.5 s on the phone.
  The system keeps them: later launches paid 0.4 s. A shipping path must hide or pre-pay it.
- **F-003 — Metal costs the phone about 90–135 MB of memory**: peaks of 422.9–471.5 MB
  against the CPU path's 334.8–337.2 MB, the highest still 128.5 MB under the ceiling.
- **F-004 — Batching the re-ranker loses everywhere it matters.** Padding a query's pairs to the
  longest (up to 512 tokens) makes the phone slower on the CPU paths and takes every batched
  variant to 572–1,298 MB; four of six phone records exceed 600 MB.
- **F-005 — The numbers barely move.** Every dense score matched the host bit for bit on every
  path and device; re-rank scores stayed within 8.2e-6; the laptop's order was identical at every
  depth; the GPU-built index searched on the CPU gave the CPU's nDCG@10 exactly.
- **F-006 — Building an index is 4× faster on either path**, not more on the GPU: the embedder
  runs one passage at a time by design (the determinism rule of Feature 004), which leaves the
  GPU little to parallelise.
- **F-007 — The first batched attempt failed on both accelerated paths**: the CLS rows of a
  batch are strided, which candle's default CPU matmul accepts and Accelerate and Metal refuse.
  Fixed in the spike's code (a contiguous copy), not in candle.

## Deliberately not done

- No fallback from the GPU to the CPU, no change to any default, no identity change (spec).
- No Android, CUDA, Neural Engine or half-precision measurement (spec Assumptions).
- The phone's order at re-ranked depths (the harness does not record it; see Cost).
- A second run of each batched variant: every batched variant fails the rule on speed or memory
  before repeatability matters.
- Why the harness embeds 4.6× slower than the command line (Building an index).
