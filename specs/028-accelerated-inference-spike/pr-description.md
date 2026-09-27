## 028 — accelerated inference spike: the Apple GPU wins on the phone

A measuring spike: what Apple's matrix library (`accelerate`) and the Apple GPU (`metal`) — both
opt-in features of the pinned candle 0.9.2 — are worth for the embedder and the re-ranker, on the
iPhone 16e and the MacBook Pro, against the same day's CPU runs. Nothing is on by default; the
verdict proposes a follow-up feature.

### Verdict

| device | path | re-ranked phase | peak memory | repeatable | quality | verdict |
|---|---|---|---|---|---|---|
| iPhone 16e | **Metal** | 1,316 → 435.5 ms (**−67%**) | 423–472 MB (< 600) | yes | unchanged | **go** |
| iPhone 16e | Accelerate | 1,316 → 1,086.5 ms (−17%) | 335–336 MB | yes | unchanged | no-go (< 30%) |
| MacBook Pro | **Metal** | 875 → 204 ms (−77%); build ×4.1 | 483–493 MB | yes | unchanged | **go** |
| MacBook Pro | **Accelerate** | 875 → 467.5 ms (−47%); build ×3.9 | 428–445 MB | yes | unchanged | **go** |
| both | any path, batched | slower, or over 600 MB on the phone | 572–1,298 MB (phone) | one run | — | no-go |

- **Quality**: `hybrid-rerank-v3` on SciFact, NFCorpus and FiQA with Metal and with Accelerate,
  every corpus embedded afresh: nDCG@10 and Recall@100 identical to Feature 026's CPU records to
  six decimals (0.721936 / 0.362470 / 0.389641).
- **Numbers**: every dense score bit-identical to the host goldens on every path and device;
  re-rank scores within 8.2e-6; on the laptop the hit order is identical at depths 0/5/10/20; a
  GPU-built SciFact index searched on the CPU scores identically.
- **Costs of Metal**: the first search after an install compiles the GPU kernels — 35.5 s on the
  phone, then 0.4 s on every later launch (the system caches them); about 90–135 MB more memory.
- **Batching the re-ranker** (all pairs of a query in one pass, padded) loses: slower on the
  phone's CPU paths, and every batched phone variant's harness run exceeds 600 MB.

The follow-up the report proposes: Metal for both models on iOS and macOS, with a CPU fallback,
an identity decision (the evidence favours leaving the path out of it), a way to pre-pay the
first-launch compilation, a `criterion` bench, Accelerate as a macOS option, and the removal of
this spike's `spike-*` features and labels.

Report: [`specs/028-accelerated-inference-spike/report.md`](report.md) · records:
[`runs/`](runs/) (38 JSON records and the three golden-suite logs; no device or team identifier in any).

### What the spike added (all off by default, labelled spike code)

- `xtriever-dense`, `xtriever-rerank`: features `spike-accelerate` / `spike-metal` (mutually
  exclusive, `compile_error!`) choosing the device the two loaders build on, and
  `spike::COMPUTE_PATH`; `xtriever-rerank`'s `spike-batch` scoring a query's pairs in one
  forward pass. Model identity unchanged on every path. `xtriever-ffi` forwards the features.
- The three measurement harnesses (the Python demo's `measure`, the Swift package's device
  test, the iOS demo's measured run) record `computePath`, `rerankBatch` and a hits digest (one
  definition in Python and Swift, pinned by a shared test vector).
- `scripts/build-ios-package.sh --spike-compute / --spike-batch` (and `compute-path.json` staged
  on every build); `scripts/spike-028-host.sh` for the host runs.
- **Size, stated plainly**: 834 changed lines of code, tests and scripts (Rule 3 asks for about
  800), 1,389 lines of spec documents and report, 522 of `Cargo.lock` (the optional Metal and
  Accelerate crates), and 27,424 lines of run records. About 2,200 lines outside the records and
  the lock file, like Feature 027's accepted exception; the owner decides whether it stays one
  pull request. (An earlier draft of this description said "about 445": that was the plumbing
  commit's count, not the pull request's.)

### Tests and gate

- Red first (T002–T007, committed failing): the compute-path names, the batched re-ranker
  against one pair at a time, the digest and labels in Python and both Swift targets.
- The existing golden suites of both model crates pass on every path: 205 of 205 on CPU,
  Accelerate and Metal (`runs/goldens-*.txt`).
- The default build is unchanged: the whole local gate passes, including `check-demos.sh`; wasm32
  fails as tracked. Each spike feature: clippy with warnings denied, `xtriever-ffi` checked for
  iOS, the two paths refused together. After the gate, `scripts/spike-028-host.sh` changed
  (below) and the review round touched `scorer.rs`, the demo harness and two test comments:
  re-run were fmt, clippy with warnings denied (default, and each path with the batch), the two
  model crates' tests (139 passed), the batched suite on all three paths, `xtriever-ffi` for iOS
  with Metal and the batch, the demo's spike tests on the simulator, the stub check and the
  Python demo's 101 tests.
- One existing test changed: `apps/python-wiki-demo/tests/test_measure.py` checks the record's
  exact key set, which now includes the three spike fields.

### Corrected along the way

- **Batching failed on Accelerate and Metal at first**: a batch's CLS rows are strided, which the
  default CPU matmul accepts and the other two refuse. Fixed in the spike's code with a
  contiguous copy; candle untouched.
- **The host script's build step** ran `hybrid-rerank-v3` on an empty cache, which the harness
  refuses; it now embeds with `dense-baseline-v1` first. The contract says so.
- **The two quality passes** first shared one build directory, where each `cargo run` could
  rebuild the other's harness with the wrong features between its build and its run. Stopped
  before anything was written and restarted with separate `CARGO_TARGET_DIR`s.
- **The Wikipedia build projection** in the build records divides by SciFact's rate; the
  harness embeds about 4.6× slower per passage than the command line does, so the report scales
  today's 12.4 h by each path's ratio instead (~3.0–3.2 h).

### Review round (`/code-review` on the finished spike)

- **The batched re-ranker ignored a non-zero time budget**: a pass that ran past the limit
  reported every pair as scored. It now discards such a pass (all or none), so the pipeline
  degrades as with the one-pair loop; test `a_batch_that_overruns_its_budget_scores_nothing`,
  written first and failing, passes on all three paths. No recorded run was affected (the demo's
  budget is 4 s, the slowest batched re-ranked phase 1.8 s; the harnesses set none).
- **`spike-028-host.sh` now refuses `--batch` for `build`, `mixed` and `quality`**, whose records
  do not name the batching mode and would have been overwritten.
- **The demo harness** reads the compute-path label once and digests the re-ranked response it
  bound in its guard.
- **The identity tests' comments** now say what they guard (a loader change) and what they do
  not (the arithmetic).
- **The report gains "Limits of the spike's builds"**: the label comes from the build, not the
  library; the label is narrower than what the features switch (the sparse encoder, Cargo's
  feature unification); identity does not follow the arithmetic in a spike build (deliberate,
  research D6); batching has no cap. Each is the follow-up's.
- Left for the follow-up: exposing the compute path from the engine, declaring the link
  frameworks in the package, sharing one `spike.rs` and the classifier head between the crates.

### Not measured

The phone's hit order at re-ranked depths (its harness compares scores by id); a second run of
each batched variant (they fail the rule before repeatability matters); why the eval harness
embeds more slowly than the command line.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
