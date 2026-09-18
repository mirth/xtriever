# 022 chunking study

**Question**: the engine indexes each BEIR document whole with its embedding cut at 256
positions, and 71 % of SciFact and 79 % of NFCorpus documents are longer — does chunking
them into passages change nDCG@10 / Recall@100, and which splitter is best?

**Method** (`reference/chunking_study.py`, `reference/tests_022/`, the 014/016 pattern):
each corpus indexed through the Python package five ways — `whole`, the 008 `contract`
chunker, `chonky`, `chonky-bounded`, and (the owner's amendment after the first results)
`chonky-if-long` — searched at k = 300 / depth 300 at re-rank depths 0 and 20, folded to
documents by MaxP, scored with the 003 scorer. **The anchor reproduced the committed
baselines to 1e-6 on every query of all three datasets** before any variant ran.

**Result**: no variant is recommended by the rule fixed in the spec (≥ +0.005 mean nDCG@10,
no dataset > 0.005 below, recall ≥ −0.005):

| variant | scope | Δ mean nDCG@10 | scifact | nfcorpus | fiqa | Δ recall |
|---|---|---|---|---|---|---|
| contract | two-way | +0.0044 | +0.0083 | +0.0005 | — | −0.0054 |
| chonky | three-way | −0.0148 | **+0.0293** | −0.0024 | −0.0713 | −0.0201 |
| chonky-bounded | two-way | +0.0108 | +0.0249 | −0.0034 | — | −0.0069 |
| chonky-if-long | three-way | −0.0059 | +0.0167 | −0.0034 | −0.0308 | −0.0085 |

Chunking is corpus-dependent: a large gain where documents are long and the answer lies
past the window (SciFact), nothing where it sits up front (NFCorpus), a loss where documents
are short or a split post loses its context (FiQA — chonky cut 120-token posts into three
passages each). The demos and docs keep indexing whole; the shipped Wikipedia index is
untouched; no pre-split input for the Rust build is opened. 32 cells with scores and the
build records are committed under `runs/`; `owner-decision.json` records the verdict.

**Tests first** (red at C1); 30 tests in `tests_022`, 118 across the four reference suites in
one collection. Two defects found and fixed on the way: `best_chunker` compared means of
different scopes (now only recommended variants compete); the chonky split cache was read
with `splitlines()`, which breaks on U+2028 inside FiQA posts (now `\n`-lines).

**Cost**: ~16 h of laptop time in eight unattended blocks. No change under `crates/`,
`swift/`, `python/src`, `apps/` or any baseline; the Rust gate unchanged; no CI job.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
