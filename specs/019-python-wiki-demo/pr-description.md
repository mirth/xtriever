# 019 python wiki demo — PR B: build from the raw snapshot, slice parity

`wikidemo build --limit N --out DIR` turns the raw Simple English Wikipedia snapshot into a
searchable 008-shaped artefact through the `xtriever` package alone — verify the snapshot
against its manifest, drop the disambiguation pages by the manifest's rules, cut each article
into passages that fit the embedder's window (the 008 contract chunker, priced by the pinned
tokenizer), `add` in batches of 4,096 (the engine embeds), `commit`, `merge`, write
`corpus.json` / `ATTRIBUTION.txt` / `wiki-build.json`, rename `<out>.partial` into place —
and `wikidemo measure --against DIR2` checks a demo-built slice against the Rust build of the
same slice. No engine, FFI, format, package-wire or baseline change.

**The check** (`specs/019-python-wiki-demo/runs/slice-MacBookPro18,3-…json`): the first
2,000 articles built by both — 1,970 selected, **8,529 passages** each, the same corpus
identity, `corpus.json` identical, and `dense/index.bin` **byte-identical**. The twenty
measurement queries at depths 0 / 5 / 10 / 20: **the same ids in the same order at every
depth, 800/800 hits identical on every score bit**; identity, counts and document counts
equal. The Python build took 14.6 min (102 ms per passage — the Rust build's own rate on a
cache miss); the full corpus (≈ 11 h) is documented with its cost and, by the owner's
decision, not run this iteration.

**Tests** (committed red first: two import errors, build 2 failed / 4 errors): the demo
suite **70 passed** (50 model-free) — the chunker replays `reference/fixtures/008/`
byte for byte (48 + 9 cases, no tokenizer needed), the rules (character window, first match
wins), and a synthetic three-article snapshot built end to end with its sidecar, attribution
and record, searched, and the four refusals (existing output, hash mismatch, `--limit 0`,
a URL that is not the derived one — nothing left on disk).

**Docs**: the demo README's build section (the recipe module by module; the 013 note — one
joined `contents` field measured +5.9 SciFact / +1.1 NFCorpus, with the one-line change for
a new corpus — and why this build keeps 008's schema: the Rust build is its oracle);
pointers from `apps/ios-wiki-demo/README.md` and the 009 spec/report.

**Gate**: fmt, clippy, deny unchanged; `git diff --stat main -- crates/ swift/ python/src
specs/*/baselines` empty; no identifiers in any record (a first slice record carried the
absolute artefact path — caught by the gate's grep, `measure` now records repository-relative
paths, the run repeated).

**Review round 2** (2 comments, both taken): the README in run order (fetch → install →
build a slice → search → explain → about → measure); the per-file test counts corrected
(chunking 5, rules 3; 70 in all).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
