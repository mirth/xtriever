# Feature 003 Report: The Evaluation Harness

**Branch**: `003-eval-harness` | **Closed**: 2026-09-12 | **Spec**: [spec.md](./spec.md) |
**Plan**: [plan.md](./plan.md)

## Verdict

`xtriever-eval` measures any `LexicalIndex` on BEIR SciFact, NFCorpus and FiQA-2018 with nDCG@10
and Recall@100 that agree with `pytrec_eval` to machine precision, on datasets pinned by size and
SHA-256. The lexical stage's baseline is recorded for all three datasets against commit
`94ddbe67f926badf962b93e8bd29d687e300189a`, every number cross-checked by the reference, and the
SciFact smoke is a blocking CI job. **ADR-0006 condition 2 is discharged.**

| | |
|---|---|
| acceptance suite | **25 / 25** offline (`cargo nextest run -p xtriever-eval`) + **4 / 4** dataset-backed (`--run-ignored only`) |
| workspace suite | 98 / 98 |
| metric goldens vs `pytrec_eval 0.5` | 11 / 11 cases, every per-query value and mean within 1e-6 (measured: ≤ 2.2e-16) |
| real baselines vs `pytrec_eval` (`--verify-run`) | 3 / 3 datasets: means agree to ≤ 2.2e-16, BEIR-rounded values identical, query counts identical |
| gate | fmt ✓ · clippy `-D warnings` ✓ · deny ✓ · iOS / iOS-sim / Android `cargo check` ✓ · no stubs ✓ · library graph pure (no `tantivy`, no C/C++) ✓ |
| `xtriever-core`, `xtriever-lexical`, `deny.toml` | **unchanged** (`git diff --stat main -- …` empty; FR-027, SC-010) |
| eval delta | **this feature establishes the baseline (ADR-0006 condition 2 discharged)** |

## The baseline — `lexical-baseline-v1`

Configuration (FR-016): `title` and `text` indexed under `standard_en`, boosts 2.0 / 1.0, empty
titles omitted, query = `Match(None, text)`, `k = 100`, one `add` + one `commit`. Lexical stage at
`94ddbe6`; harness at the merge commit of this branch (recorded per report file).

| dataset | nDCG@10 | Recall@100 | BEIR-rounded | published BM25 nDCG@10 / R@100 (Thakur et al. 2021, Tables 2, 9) | FR-020 band ±0.10 | scored | no-relevant | dropped self-ids |
|---|---|---|---|---|---|---|---|---|
| SciFact | **0.627044** | 0.887556 | 0.62704 / 0.88756 | 0.665 / 0.908 | **inside** (Δ −0.038) | 300 | 0 | 0 |
| NFCorpus | **0.311523** | 0.247820 | 0.31152 / 0.24782 | 0.325 / 0.250 | **inside** (Δ −0.014) | 323 | 0 | 0 |
| FiQA-2018 | **0.250238** | 0.551775 | 0.25024 / 0.55178 | 0.236 / 0.539 | **inside** (Δ +0.014) | 648 | 0 | 0 |

Files: [`baselines/lexical-baseline-v1.{scifact,nfcorpus,fiqa}.json`](./baselines/), each with
per-query values, dataset hashes and counts. The published system is Anserini BM25 (k1 = 0.9,
b = 0.4, separate title/passage fields); ours is tantivy BM25 (k1 = 1.2, b = 0.75, non-configurable)
with the `standard_en` chain. That all three land within ±0.04 of published — FiQA above it — is a
harness-validity result (FR-020), not a claim of equivalence.

**Reproducibility (SC-004)**: SciFact run twice on the same commit → byte-identical reports.

**SciFact end to end (SC-008)**: 2.0 s wall for `cargo run --release … run --dataset scifact`
(index 5,183 documents, 300 queries, score) on the development host. FiQA: 10.3 s.

## FiQA observations (FR-018, SC-009) — recorded, not budgeted

| observation | value | method |
|---|---|---|
| index directory | **18,370,560 bytes** (17.5 MiB) for 57,638 documents ≈ 319 bytes/document | `du -sk target/xt-eval-index/fiqa` after the run |
| peak RSS of the whole `beir run` process | **343,441,408 bytes** (327.5 MiB) | `/usr/bin/time -l`, "maximum resident set size", macOS 25.6 / Apple Silicon |

The RSS number is the **harness process**, not the index: it holds the parsed 48 MB corpus, the
built `Document`s (a second copy of every text), the query set and the results, on top of the
backend's 15 MB writer arena and the memory-mapped index. It is the first measured data point on the
curve Feature 002 deferred (its FR-038), and it says the on-disk index is small — 17.5 MiB at 57.6k
documents versus the 1.2 GB naive extrapolation 001 recorded — while leaving the *in-process index
footprint* at scale still unmeasured. That measurement belongs to a spec with an on-device
configuration; this feature records the number and its method and claims nothing more.

## Findings

### F-001 — The identical-id rule belongs at scoring time (design corrected)

The plan (D3) placed BEIR's `ignore_identical_ids` pop in the runner. The `identical_ids` golden —
generated with the pop at evaluation, which is where BEIR's `evaluate()` performs it — disagreed with
a runner-side implementation by construction. Moved to `metrics::score_queries`, which now also
counts `dropped_identical`. Consequences: the `Run` is a faithful record of the retriever's output
(what `--verify-run` needs, since the Python side pops for itself), and the runner has no
BEIR-specific rule in it. `dropped_identical` is 0 on all three baselines: none of FiQA's 55
colliding ids was retrieved for its own query.

### F-002 — A dedupe-blind test (test bug)

`metrics::order_given_is_order_scored` used nine copies of one filler id, which the "repeated id
counts once" rule collapses to a single rank. Fixed to distinct fillers; the metric was right.

### F-003 — `fetch-beir.sh` self-heals a tampered file

The script re-extracts from the verified archive before checking files, so a corrupted extracted
file is silently restored rather than reported. That is acceptable for the *script* (the archive
hash is what it guards); the hard failure SC-003 requires lives in the Rust loader, which rejects
a tampered file naming both hashes before any parsing (`dataset::tampered_file_fails_naming_both_hashes`,
and demonstrated on the real SciFact qrels: `hash mismatch … expected 5389/0864bb…, actual 5390/…`).

## Measured facts worth keeping (research D1–D5, confirmed by the run)

- All three `qrels/test.tsv` files are CRLF with a header line; the loader strips `\r`.
- FiQA has no titles; SciFact and NFCorpus have one on every document. `omit_empty_fields` makes
  FiQA a single-field index.
- NFCorpus is the only dataset with a grade above 1 (576 pairs at grade 2); the linear-gain rule
  is exercised by the real baseline, not only by the `graded` golden.
- Zero dangling references in all three datasets; zero queries without a relevant document.

## CI

`eval-smoke` job added to `.github/workflows/ci.yml`: path-filtered to the ranking crates (plus
the manifest and baselines), `actions/cache` keyed by the manifest hash, `scripts/fetch-beir.sh
scifact`, then `beir smoke` against the committed SciFact baseline. Blocking. FR-023's escape hatch
is quoted in the workflow so a demotion has to edit it in place.

**T042 — first push (2026-09-12)**: the path filter worked (`eval-smoke` ran on a branch touching
`crates/xtriever-eval/`), the cache missed as expected on a first run, and the job **failed at the
download**: `curl: (7) Failed to connect to public.ukp.informatik.tu-darmstadt.de port 443 after
1594 ms`. The host answered in 75 ms from the development machine at the same time, so the runner
either hit a transient outage or the university's firewall does not admit GitHub's egress ranges.
Response (research R4): `fetch-beir.sh` now retries six times with backoff and a 20 s connect
timeout and reports an unreachable source as a *download* failure, never as a hash failure. The
Hugging Face copy of BEIR was checked as a mirror and rejected: its corpus and queries are Parquet
conversions, not the pinned bytes (only `qrels/test.tsv` is byte-identical). If the host proves
permanently unreachable from GitHub runners, FR-023's escape hatch applies — a commit demoting the
job to non-blocking for one feature cycle, stating this reason and the re-blocking point — and the
smoke keeps running locally under Rule 5. **Second push (PR #3, `pull_request` event)**: the download step was never reached — `dorny/paths-filter`
failed with `Resource not accessible by integration`: on pull-request events it lists changed
files through the GitHub API, and the repository's restricted default token lacked
`pull-requests: read` (the first run was a `push` event, where the action uses `git diff` and
needs no API). Fixed by granting the job `permissions: { contents: read, pull-requests: read }`.
Third-push outcome: *(to be recorded)* — this is the run that will actually exercise the download
retries.

## Success criteria → evidence

| SC | evidence |
|---|---|
| SC-001 metrics agree on every golden | `metrics::every_golden_case_agrees_with_the_reference` (11 cases, 1e-6); `--verify-run` on all three real baselines |
| SC-002 three datasets fetched, verified, counted | `dataset_real::*` (3 tests); `beir verify scifact nfcorpus fiqa` |
| SC-003 tampered file rejected before scoring | `dataset::tampered_file_fails_naming_both_hashes`; real-cache demonstration above |
| SC-004 baseline exists and reproduces | three baseline files; SciFact re-run byte-identical; `baseline_real` |
| SC-005 delta table | `report::delta_table_and_adr_trigger`; `beir delta` over all three |
| SC-006 smoke fails on decrease, passes otherwise | `report::smoke_fails_on_any_decrease_and_passes_otherwise`; `beir smoke` exit 0 vs raised baseline exit 2 |
| SC-007 offline suite needs no network | 25 tests, no cache required (`fixtures_valid`, `metrics*`, `dataset`, `run`, `report`) |
| SC-008 SciFact under a minute | 2.0 s |
| SC-009 FiQA observations present | `observations` in `lexical-baseline-v1.fiqa.json` |
| SC-010 targets, purity, core untouched | gate: iOS / iOS-sim / Android; `cargo tree -e normal` has no `tantivy`; `git diff main -- core lexical deny.toml` empty |

## Known costs (stated, not claimed small)

- The harness holds the corpus, the built documents and the results in memory at once; FiQA's
  327 MiB peak is mostly that. A streaming builder would halve it — not needed for these sizes.
- `Dataset::load` hashes every file (48 MB for FiQA) on every load — a fraction of a second, and
  the price of never scoring unverified data.
