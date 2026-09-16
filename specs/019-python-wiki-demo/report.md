# Report: The Python Wikipedia Demo

**Feature**: 019 · **Branch**: `019-python-wiki-demo` · **Status**: PR A done (search, about,
measure, the host record); PR B (build, slice parity) follows the merge.

## Verdict (PR A)

The second demo runs from one command over the shipped Wikipedia index through the 011
package, prints the fused list, then the re-ranked list with what moved, the eight explained
features, the engine's stage report and the attribution — and its `measure` command checks
the laptop against the phone's host goldens: **parity PASS, 800 of 800 hits identical on
every score bit** at depths 0 / 5 / 10 / 20. Medians on this laptop: fused **246 ms**,
re-ranked at the demo default (depth 10) **1,005 ms**, total 1,251 ms; depth 20 1,683 ms.
No engine, FFI, format, package-wire or baseline change (`git diff --stat main -- crates/
swift/ python/src specs/*/baselines` empty).

## Red checkpoint A (2026-09-17)

`apps/python-wiki-demo/.venv/bin/pytest apps/python-wiki-demo/tests -q` at commit A1:
**7 errors during collection** — every test file fails on import (`No module named
'wikidemo.hits'` and the like); only the package skeleton (`__init__`, `__main__`, a `cli.main`
returning 2) exists. Test files: `test_hits`, `test_record`, `test_inputs`, `test_render`,
`test_search` (models), `test_about` (models), `test_measure`.

One test example was corrected while going green: `test_marks_follow_the_ios_rule` had
`c` at fused rank 3 and re-ranked rank 4 marked `same`; the iOS rule (and the port) says
`down(1)`. The example now uses `[b, a, c, e]` so it covers `up`, `down`, `same`, `new` and
a drop. Nothing in the rule moved.

## US1 — search (green)

`test_search.py`: the fused call equals the 007 goldens' `without_reranker` lists and the
re-ranked call their `with_reranker` lists on ids, fused f64 bits, bm25 / dense / re-rank
f32 bits, re-rank rank and combined bits for all eight queries. The shipped index, first command of a process:

```
opened …/target/xt-wiki (427,947 passages, format 2) · embedder 89 ms · re-ranker 86 ms · open 308 ms · mmap
warm-up: the first search of a process pages the vectors in

fused (lexical + dense), 10 hits, 1609 ms
 1. Sky blue  834076#0  passage 1
    https://simple.wikipedia.org/wiki/Sky%20blue
    Sky blue is a shade of cyan. …
 2. Sky  2004#0  passage 1
 …
re-ranked (interpolate α 0.5, depth 10), 10 hits, 869 ms
 1. ↑1   Sky  2004#0  passage 1
 2. ↓1   Sky blue  834076#0  passage 1
 3. =    Sky  2004#1  passage 2
 …
stages: lexical 100 · dense 100 · re-rank 10 candidates, 10 scored · time limit ignored: no · engine 868 ms
wall: fused 1609 ms · re-ranked 869 ms · total 2,478 ms · peak resident 987 MB
```

The 1.6 s fused time is the cold first query (the vectors paging in — 008 measured the same
on the phone); warm, the fused stage is ~250 ms (the record below).

**The "no passages found" path is unreachable on a non-empty index**: the dense stage
always returns candidates (an empty query gives `lexical 0 · dense 100` and ten hits). The
line and the exit-0 path exist for an empty index or a degraded dense stage with no lexical
hit; it is exercised by a model-free test that stubs an empty fused response (review
round 1 — the earlier test searched the fixture for such a query and skipped).

A missing input is named with its producer before any model loads
(`wikidemo: missing the Wikipedia artefact: /nonexistent/index/xtriever-pipeline.json` /
`produce it with: …`), exit 1, in well under a second (`test_cli_exits_1_on_a_missing_artefact`).

## US2 — the pipeline visible (verified on the shipped index)

- `--explain --depth 20 "who painted the Mona Lisa"`: eight features per hit; on the fused
  list the three re-rank features read "not seen by this stage"; on the re-ranked head
  `rerank.combined` is the ordering score.
- `--budget-ms 300 "what is the capital of Australia"`: `stages: lexical 100 · dense 100 ·
  re-rank 10 candidates, 2 scored · time limit ignored: no · engine 467 ms` — the budget
  cut the re-ranker to 2 of 10 pairs; the engine reports it as `scored < candidates`, the
  list still arrives, exit 0.
- the same with `--strict`: `wikidemo: BudgetExhausted: dense stage: 338 ms elapsed > 300 ms
  limit`, exit 1 (the cold first call spent the budget in the dense stage).
- `--mode replace`: `re-ranked (replace, depth 10)`, the head ordered by `rerank.score`,
  `rerank.combined` "not seen"; with `-k 3` two hits are `new` (re-ranked from fused
  positions 4–10, outside the printed fused top 3 — the iOS semantics).
- `--depth 0`: one block, no marks, `re-rank none`.

## US4 — about (green)

`test_about.py` (2 passed) against the shipped artefact: every field equals `info()` and
`corpus.json`; the attribution block equals `target/xt-wiki/ATTRIBUTION.txt` byte for byte
(`diff` empty). The engine's depth and the demo's are adjacent, labelled:
`re-rank depth (engine default): 20` / `re-rank depth (demo default): 10`.

## US5 — the laptop agrees with the phone (record committed)

`specs/019-python-wiki-demo/runs/MacBookPro18,3-20260916T202930Z-mmap-threadsdefault.json`
(machine named by hardware model; no hostname or user name — grepped):

| | this laptop (019, depth 10) | iPhone 16e (018, depth 10) | iPhone 16e (009, depth 20) |
|---|---|---|---|
| fused median | **246 ms** (max 288) | 341.5 ms | 339 ms |
| re-ranked median | **1,005 ms** (max 1,245) | 1,369 ms | 2,288 ms |
| total median | **1,251 ms** (max 1,483) | 1,704.5 ms | 2,631 ms |
| re-ranked at depth 20 | 1,683 ms (max 1,934) | 2,295.65 (017 harness) | — |
| per-depth medians 0 / 5 / 10 / 20 | 246 / 629 / 1,005 / 1,683 | 410 / 877 / 1,393 / 2,296 (017 harness) | |
| open / embedder / re-ranker / warm-up | 347 / 88 / 118 / 1,563 ms | 760 / 172 / — ms | |
| peak resident | 1,029 MB (`ru_maxrss`; includes the mmapped 1 GB index) | 305 MB (ledger) | 536 MB |
| parity | **PASS** — lexical 20/20, fused order 20/20, dense max |Δ| 0, re-rank max |Δ| 0, **800/800 all bits** | PASS (1e-3) | PASS |

Threads: 10 (`os.cpu_count`, candle's default). `test_measure.py` (13 passed): the
comparison rule on synthetic truths — missing query, depth-set mismatch, hit count, unknown
id, score on one side only, lexical bits exact, fused order only at depth 0, the 1e-3
tolerance — and the 007 goldens through the same comparison → PASS, all bits identical.

## Success criteria (PR A)

| SC | Result |
|---|---|
| SC-001 | fused median 246 ms ≤ 1 s; re-ranked (depth 10) 1,005 ms ≤ 3 s — **met** |
| SC-002 | `test_search.py`: ids and every score bit equal the 007 goldens — **met** |
| SC-003 | `test_about.py`: fields equal `info()` / sidecar; attribution byte-identical — **met** |
| SC-004 | the record above: PASS at 0 / 5 / 10 / 20, footprint recorded — **met** |
| SC-005 | PR B |
| SC-006 | first search's fused list within 5 s: open 0.3 s + cold fused 1.6 s ≈ 2 s — **met**; missing input reported in < 1 s (tested) — **met** |
| SC-007 | `git diff --stat main -- crates/ swift/ python/src specs/*/baselines` empty — **met** |

## Review round 1 (Copilot, 8 comments — all taken)

1. `build` reached an uncaught `ModuleNotFoundError` — the subcommand is not offered until
   PR B (`invalid choice`, exit 2; tested).
2. `--against` checked order only at depth 0 (the device rule) while FR-014 wants the same
   order at every depth — `compare(..., order_at_every_depth=True)` for the slice mode;
   the device rule for the host goldens is unchanged (both tested).
3. `--against` exited 0 on unequal identity / counts / documents — the `against` block now
   carries its own verdict and the exit is 1 unless both verdicts pass.
4. The empty-result test could skip (and did) — replaced by a model-free test that stubs an
   empty fused response through `cmd_search`: the message, the stage line, exit 0, and no
   re-ranked call.
5. `--budget-ms -1` failed inside the FFI conversion — validated as non-negative (exit 2).
6. The fused block was printed only after both calls — `run_search` is a generator that
   yields the fused stage before starting the re-ranked call, and the CLI flushes the fused
   block first (the two headers are ~1 s apart on the shipped index; tested at the
   generator level).
7. `underCeiling` was strict; the device tests are inclusive — `<=` (tested at the boundary).
8. `--mode interpolate` passed `None` (the index's recorded mode) — now explicit
   `INTERPOLATE(alpha=0.5)`; the search label is the requested mode, About's line reads
   `re-rank mode (recorded): …`.

The demo suite after the round: **54 passed, 0 skipped** (41 model-free). The record and
the Rust gate are unaffected (no engine value changed; the goldens tests still pass on bits).

## Gate (PR A)

`cargo fmt --check` ok; `cargo clippy --workspace --all-targets` clean; `cargo nextest run
--workspace` 279 passed, 60 skipped; `cargo deny check` ok; the demo suite **54 passed** (41 model-free; after review round 1); the package suite `python/tests` **33 passed** (untouched); no
eval deltas — nothing ranking-affecting changed.

## Deliberately not done (PR A)

- No `build` yet (PR B); the README section points there.
- No CI job (research D18): a model-free job would need the wheel built in CI for a
  rendering/chunker replay the local gate already runs.
- No persisted settings, no REPL, no `--json`: flags are the settings.
- No Wikipedia helpers added to the `xtriever` package; the demo carries them, tested
  against the same eleven URL cases and the iOS mark rule.
