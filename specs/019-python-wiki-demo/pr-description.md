# 019 python wiki demo — PR A: search, about, measure

The second demo of the engine over the same corpus as the iOS one: a Python command line
at `apps/python-wiki-demo/` over the Feature 011 package. No engine, FFI, format,
package-wire or baseline change (`git diff --stat main -- crates/ swift/ python/src
specs/*/baselines` is empty). Spec, plan, research, contracts and quickstart under
`specs/019-python-wiki-demo/`; PR B (the build from the raw snapshot and the slice parity
check) follows.

**What it does**

- `wikidemo search "why is the sky blue"` — the fused (lexical + dense) list printed first,
  then the re-ranked list with a mark per hit (`↑n` / `↓n` / `=` / `new`, dropped hits
  named), each hit's title / id / position / article URL / passage derived from the engine's
  hit by the 008 convention (the Swift package's and the CLI's rules, the CLI's eleven URL
  cases in the tests); `--explain` for the eight features under the engine's names; the
  engine's stage report, wall clocks and peak resident size. Flags: `-k`, `--depth
  {0,5,10,20}` (default **10** — the 018 trade-off in the help), `--budget-ms`, `--strict`,
  `--mode {interpolate,replace}`, `--snippet`.
- `wikidemo about` — corpus identity and counts from `corpus.json`, `info()`'s fields, the
  engine's re-rank depth beside the demo's default, the attribution verbatim (byte-identical
  to `ATTRIBUTION.txt`, tested).
- `wikidemo measure` — the twenty measurement queries at depths 0 / 5 / 10 / 20 compared
  with the host goldens by the device test's rule, and a record with per-depth latency and
  footprint.

**The record** (`specs/019-python-wiki-demo/runs/MacBookPro18,3-…-mmap-threadsdefault.json`):
parity **PASS** — lexical bit-identical 20/20, fused order 20/20, dense and re-rank max |Δ|
0, **800 of 800 hits identical on every score bit**. Medians: fused **246 ms**, re-ranked at
depth 10 **1,005 ms**, total 1,251 ms; depth 20 1,683 ms (the iPhone 16e at depth 10, 018:
341.5 / 1,369 / 1,704.5 ms). Peak resident 1,029 MB — includes the memory-mapped 1 GB index;
the phone's 600 MB ceiling is recorded for comparison only.

**Tests** (Rule 4: committed red first, 7 collection errors at A1): the demo suite **48
passed, 1 skipped** — 37 model-free (rendering line for line, the eleven URL cases, the iOS
mark rule, the identity hash of the shipped corpus, inputs and producers, the comparison
rule on synthetic truths) and 12 model-backed against the 007 fixture and its goldens (ids
and every score bit). The skip is the "no passages found" path, unreachable while the dense
stage always returns candidates. Package suite `python/tests` 33 passed, untouched.

**Gate**: fmt, clippy, `nextest` (279 passed), deny — unchanged; no eval deltas because
nothing ranking-affecting changed.

**Not in this PR**: `build` (PR B), a CI job (research D18), persisted settings, `--json`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
