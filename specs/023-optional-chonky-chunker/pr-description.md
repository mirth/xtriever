## 023 — chonky as an optional chunker for the Wikipedia demo build

`wikidemo build --chunker {contract,chonky}`, default **`contract`**: the Feature 008
contract chunker is back as the demo's default (the 019 copy restored verbatim from
`802cf72` into `wikidemo/contract.py`, with its 48 + 9 fixture-replay tests), so a
demo-built slice is the shipped recipe again; chonky (Feature 021) moves behind the flag
into `wikidemo/chonky_chunker.py`, and `chonky` / `transformers` / `torch` become the
optional extra `.[chonky]` — a chonky build without it is refused with the install command
before the snapshot is opened, and a default build never imports them. Follows the 022 study
(no chunking variant recommended; corpus-dependent).

**Parity** (`runs/parity-…`): the default 2,000-article slice vs the Rust-built slice —
identity `20949fb4…` equal, 8,529 documents equal, **800/800 hits identical on every score
bit**, order identical at every depth: **PASS**. Build 14.3 min (019: 14.6).

**The option** (`runs/slice-chonky-200-…`): 200 articles → 950 passages, 87 over the
window (9.2 %), 117 s; the chonky block in the sidecar.

**Tests**: 90 passed (65 model-free; a `chonky` marker skips with the reason when the extra
or its model is absent). Red checkpoint committed first (2 failed, 5 collection errors).

**Gate**: `fmt` / `clippy` / `deny` unchanged (no Rust touched); `git diff --stat main --
crates/ swift/ python/src apps/python-minimal-demo reference/ specs/*/baselines` empty.
No eval deltas — nothing ranking-affecting in the engine.

**Size**: `apps/` 884 + / 255 − over 15 files, of which `contract.py` is the restoration
(`diff <(git show 802cf72:apps/python-wiki-demo/wikidemo/chunking.py) apps/python-wiki-demo/wikidemo/contract.py`
→ the header and the class wrapper only); plus the spec documents and three run records.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
