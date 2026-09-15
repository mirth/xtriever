# Feature 012 — Sparse Expansion Spike (Python, no engine change)

**Written for**: the reviewer of the `012-sparse-spike` branch.

A de-risking spike before any Rust: can learned sparse expansions from an inference-free
document encoder, living in the inverted index, improve retrieval — measured on the three
BEIR sets with the 003 reference scorer against the engine's own exported runs. **Verdict:
GO** by the rule the spec fixed before the runs. Report: [report.md](./report.md); every
number: [runs/summary.json](./runs/summary.json).

## What is in the branch

- `reference/sparse_spike.py` (~700 lines): `pin` / `encode` / `export` / `score` / `all` /
  `summary` — the model card's recipe, shard caches under `target/`, the engine's runs
  exported through the harness and re-scored, fourteen scoring variants, the FR-011 verdict.
- `reference/requirements-012.{in,txt}` (the 003/004 pins + scipy, snowballstemmer),
  `reference/tests_012/` (16 checks: the scorer probe, nine engine-run reproductions to 1e-6,
  the recipe against the model card's own example, hand-computed BM25/RRF/top-k).
- `reference/models/manifest-sparse-doc-v{2,3}.json` — pinned revisions and file hashes
  (Apache-2.0; weights git-ignored, fetched by the existing `scripts/fetch-model.sh`).
- `specs/012-sparse-spike/runs/` — the summary and the two costs records; the report.
- Nothing under `crates/`, `swift/`, `apps/`, `python/`, `.github/`, `deny.toml`.

## Headline (nDCG@10)

| | SciFact | NFCorpus | FiQA | mean |
|---|---|---|---|---|
| engine BM25 | 0.627 | 0.312 | 0.250 | 0.396 |
| engine hybrid (BM25 + dense, RRF) | 0.690 | 0.345 | 0.369 | 0.468 |
| sparse v3, dot product, alone | 0.708 | 0.345 | 0.357 | 0.470 |
| BM25 + dense + sparse v3 (RRF) | 0.714 | 0.351 | 0.388 | **0.484** |

The model cards' numbers reproduce to the third decimal. Quantising document weights to
×100 integers costs ≤ 0.03 points. The "expansions as a plain BM25 field" shortcut fails on
FiQA (0.199) — the dot-product scorer is required.

## What 013 inherits

The pin (v3 @ `babf71f3…`), the recipe (report "Verdict", verbatim), ×100 quantisation, RRF
three ways; the cost picture: ~239 non-zeros per passage → ~102 M postings / 150–255 MB
mapped for Wikipedia; the encoder is ~5× the MiniLM embedder's FLOPs, so corpus encoding
belongs outside the Rust build (report F-004). Two side findings for other features: the
engine's BM25 is 6 points behind a one-field Snowball BM25 on SciFact (F-002), and the
masked-LM logits dominate memory — batch small (F-001).

## Review round 1

Copilot, four comments, all taken (report table): per-shard cost metadata and a full
re-encode (the throughput figures changed, 27–34 docs/s; the metrics did not), effective
quantisation scale in the reports' provenance, required CLI flags, an exact expected-id
assertion in the recipe check.

## Gate

`pytest reference/tests_012` 16 / 16 · `cargo nextest run --workspace` 263 / 263 (unchanged) ·
guarded-paths diff against `main` empty · no weights, no `target/` files, no identifiers
committed. CI untouched (nothing here runs in CI).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
