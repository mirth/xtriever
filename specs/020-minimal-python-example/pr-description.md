# 020 minimal python demo

The smallest demonstration of the pipeline: `apps/python-minimal-demo/demo.py`, **79
lines**, self-contained — ten short documents written in the file, one `contents` field
(Feature 013's layout), `create` → `add` → `commit` → `merge` in a temporary directory, one
search shown twice (the fused stage at depth 0, the re-ranked stage at depth 10), the
directory removed. It needs the `xtriever` wheel and the two pinned models and nothing
else; nothing from the Wikipedia demo is imported. A run takes 1.8 s.

**The oracle** (tests first — 5 failed at the red checkpoint): the script's hits equal a
direct use of the package on the same documents — ids in order and identical score bits at
both depths — and a second run gives the same; model-free tests pin the 80-line budget, the
usage exit (no model loads), the printed lines and the corpus's shape. **5 passed.**

**Docs**: a README with the inputs, the two suggested queries and what the two lists show,
and what is left out with the demo that has it; one pointer line in the Wikipedia demo's
README.

No change under `crates/`, `swift/`, `python/src`, `apps/python-wiki-demo/wikidemo` or any
baseline; the Rust gate and the 019 suite (70) unchanged. No CI job (standing rule).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
