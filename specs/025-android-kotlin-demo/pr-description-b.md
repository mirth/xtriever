## 025 (PR B) — the Android demonstration: Wikipedia on the phone, offline, with the pipeline on screen

`apps/android-wiki-demo/` is the third demonstration of the same engine over the same corpus as
the iOS app and the Python command line: a Compose application that searches the 2,000-article
Wikipedia slice on the device, offline, and shows the pipeline working — the fused list first,
then the re-ranked order with a mark per hit, each hit's eight explained features under the
engine's names, the engine's stage report, the corpus identity and the licence attribution, and
a re-rank depth that is remembered.

The application owns no retrieval logic. Its only arithmetic is the change marks, which compare
two lists the engine produced. The manifest asks for no permissions at all, which is the
simplest way to check the offline claim.

**Bundled and prepared once.** The package carries both pinned models (174 MB) and the corpus
(24 MB) as uncompressed assets, and the first launch copies them into application storage with
progress: the engine memory-maps the vectors and the weights, and an asset inside a package is
not a file the system can map. Each file is written to a temporary name and renamed, and a
completion marker is written last, so a launch interrupted by a kill re-extracts instead of
opening a half-written index.

**The measured run** (`runs/sdk_gphone64_arm64-…-mmap-threads4.json`) — twenty measurement
queries at re-rank depths 0, 5, 10 and 20 against host goldens for the same corpus, produced by
`xtriever wiki expected`:

| | |
|---|---|
| parity | **PASS**, 800 hits compared |
| identifiers, order, lexical bits, fused bits | identical to the host's |
| dense scores | within 1.2e-7 |
| re-rank scores | within 5.3e-6 |
| median latency, depth 0 / 5 / 10 / 20 | 247 / 1,154 / 2,043 / 3,908 ms |
| peak resident | 474 MB, against the project's 600 MB phone ceiling |

Those latency and memory figures are an **emulator's**, on four cores of a laptop. The record
says so in its own `claims` field, and they must not be read beside the iPhone's numbers. A
physical-device run is deliberately out of scope for this feature.

**Tests**: 26 instrumented tests — the two lists and their marks, the fused answer published
before the re-ranked one, a stale search that never overwrites a newer one, the title, passage and article
link rules, an empty query, a cancelled search, a spent budget degrading and again in strict
mode, preparation and its recovery from an interrupted extraction, a refusal when storage is
short, settings persistence across a restart, and About's facts against what the engine and the
corpus sidecar report, including the recorded dense compaction share.

**Two build facts worth knowing.** Compose 2026.09 requires compiling against API 37, so both
modules do. And `kotlinx-coroutines-test` must match the coroutines core that the lifecycle
libraries resolve, or every coroutine test fails at run time with `NoSuchMethodError`.

**Not ranking-affecting.** No crate changes: `git diff main -- crates/ deny.toml` is empty.

Gate: fmt, clippy with warnings denied, `cargo nextest run --workspace` 322 passed, deny, the
cross-target checks, and both instrumented suites on the emulator — 6 in the module, 24 in the
application, plus the measurement run driven directly so the record survives.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
