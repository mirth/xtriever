# Data Model: Hygiene after 013–016

**Feature**: `017-post-015-hygiene` | **Date**: 2026-09-16

| entity | shape / rule |
|---|---|
| **Host goldens** `target/xt-wiki/expected.json` | `{generated_by, info (+ rerank_mode), queries: [{id, text, k: 10, depths: {"0": R, "5": R, "10": R, "20": R}}]}`; `R` = the 007 golden response shape (hits with bit strings incl. `rerank_combined_bits`, stages). Depth-0 responses byte-identical to the previous file's |
| **Device run record** | the 008/009 `RunRecord` (schema 2): device, build, load path `mmap`, threads default, `feature` (`008-wiki-corpus` — the harness's corpus label), per-query per-depth latency for depths 0 / 5 / 10 / 20, footprint samples and peak, parity `{queriesCompared, lexicalBitIdentical, fusedOrderIdentical, denseMaxAbsDiff, rerankMaxAbsDiff, toleranceAbs 1e-3, verdict}`, verdicts vs the 600 MB ceiling; committed as `runs/<device>-<UTC timestamp>-mmap-threadsdefault.json` |
| **Budget test contract** | for each fixture query under a short budget in degrading mode: `Ok`, `!time_limit_ignored`, scored hits first; across queries: `partial ≥ 1` (adaptive budget 200 → 100 → 50 → 25 → 10 ms until satisfied); under a 60 s budget: no query partial or skipped |
| **Reference test helpers** | `reference/tests_0NN/helpers_0NN.py` holding the suite's shared constants; `conftest.py` and tests import from it; no test imports `conftest` by name |
| **Docs** | `apps/ios-wiki-demo/README.md`, `crates/xtriever-cli/src/wiki/mod.rs`: the default, α, depth, upgrade behaviour (ADR-0012), the override, the pointer to the 015 report |

Invariants: no baseline changes; nothing under `crates/` but the two named files; the device
identifiers never in a tracked file.
