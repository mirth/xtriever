# Feature 010 fixtures — the id map's byte-identity golden

`ids-golden.json` is the **reference implementation's output**: it was written by the
pre-change `IdMap` (`crates/xtriever-pipeline/src/ids.rs` at commit `1d45490`, format version 2)
after replaying the 30 operations of `ids-golden-script.json` in order, on 2026-09-15, via a
throwaway `#[ignore]` unit test (`golden_gen`) that called `IdMap::assign` / `IdMap::remove` and
then `IdMap::write` — deleted before the commit.

- sha256: `3b0fcf65df444ea6b8a8e50cf6b068ed22292629795e0ee190317a26389baa9c`, 1,230 bytes
- 25 slots (2 deleted), 14 chunk entries over 5 parents plus 5 single-chunk parents, chunk keys
  in string order (`"10","12",…,"8","9"`), a `null` byte range, a non-ASCII id kept as UTF-8, a
  `"` escaped, a 117-character id.

Consumers: `crates/xtriever-pipeline/tests/ids_golden.rs` (replay through `HybridIndex` and
reopen, public API) and the `ids.rs` unit tests (replay through `IdMap`, read → write). Both
must reproduce the file byte for byte; the change under test (Feature 010) must not alter it.
