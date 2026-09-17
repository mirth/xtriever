# 021 chonky wiki chunking

`wikidemo build` now splits articles with the [chonky](https://github.com/mirth/chonky)
paragraph splitter — a fine-tuned token-classification model that returns the text as
contiguous slices at predicted paragraph breaks — instead of the demo's 250-line copy of the
Feature 008 contract chunker, which is deleted along with its fixture replays. Every
non-empty slice is one passage with its byte range; the split must partition the text
(checked on every article at build time and in the tests); passages over the embedder's
256-position window are added whole (the engine embeds their first 256 word-pieces, the
lexical index sees all of it) and counted. The recipe reads: verify, exclude, split, add,
commit, merge.

**The model is pinned like the engine's**: `reference/models/manifest-chonky.json`
(`mirth/chonky_distilbert_base_uncased_1` at `01d8aae…`, six files with sizes and sha256),
fetched by the existing `scripts/fetch-model.sh` (PASS), loaded from disk only (a 20-article
build with `HF_HUB_OFFLINE=1` succeeds; a missing directory is refused with the fetch command
in 0.07 s before `torch` is imported).

**The slice** (`specs/021-chonky-wiki-chunking/runs/`): 2,000 articles → 1,970 selected,
**8,487 passages**, **784 over the window (9.2 %)**, positions median 82 / p90 239 / max
10,367; split 140 s, build 16.9 min. The corpus identity changes (the sidecar's chunker block
names chonky and the revision), so a demo-built index is no longer the Rust build's — the
README says so and stops proposing `measure --against` the Rust slice; `measure` over the
shipped artefact against the phone's goldens is unchanged.

**Tests** (red first): the Wikipedia demo suite **74 passed** (53 model-free) — the offset
arithmetic on a stub splitter, the partition enforced, the over-window count, the real
splitter on the snapshot's first article, the synthetic-snapshot build through chonky, the
new input and its refusal. Dependencies: `chonky==0.1.7`, `transformers==5.17.0`,
`torch==2.14.0` (the resolver's versions), `tokenizers==0.23.2` kept for the window count.

**Two targets reported as missed, not adjusted**: the split took 140 s on the slice (the spec
said under a minute — the 60-article rate did not hold on long articles); `chunking.py` is
82 lines (the spec said under 80).

No change under `crates/`, `swift/`, `python/src`, `apps/python-minimal-demo`,
`reference/fixtures` or any baseline; the Rust gate unchanged; no CI job.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
