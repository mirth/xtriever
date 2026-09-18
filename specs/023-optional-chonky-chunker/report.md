# Report: Chonky as an Optional Chunker for the Wikipedia Demo Build

**Status**: in progress — red checkpoint reached.

## Red checkpoint (C1, 2026-09-18)

`apps/python-wiki-demo/.venv/bin/pytest apps/python-wiki-demo/tests -q --continue-on-collection-errors`:
**2 failed, 53 passed, 5 errors**. The five collection errors are the new / renamed names
(`wikidemo.contract`, `wikidemo.chonky_chunker`, `chunking.make_chunker`,
`record.CONTRACT_CHUNKER` / `CHONKY_CHUNKER`); the two failures are `test_cli`'s
`--chunker` option and the `needs_for` list. The untouched files (`test_about`,
`test_hits`, `test_inputs`, `test_measure`, `test_render`, `test_rules`, `test_search`)
pass.
