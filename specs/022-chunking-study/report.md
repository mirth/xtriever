# Report: The Chunking Study

**Feature**: 022 · **Branch**: `022-chunking-study` · **Status**: in progress

## Environment (T001)

`reference/requirements-022.in` = the 012 pins + `chonky==0.1.7`. The plan said the lock would
be hashed; it is not: the 012 lock turned out to be pinned but unhashed (0 hashes — so
`scripts/setup-reference-venv.sh`'s `--require-hashes` cannot have produced `.venv-012`
either), an online `pip-compile --generate-hashes` downloaded more than 1 GB of torch wheels in
30 minutes without finishing, and `--no-index` cannot see torch. `requirements-022.txt` is
therefore the 012 lock verbatim plus `chonky==0.1.7`, with a header saying so;
`reference/.venv-022` was made with `uv venv` + `uv pip install -r` (seconds, from the cache)
and the `xtriever` wheel from the local build: xtriever 0.1.0, torch 2.14.0, transformers 5.17.0.
Research D1, the plan and the quickstart corrected to match.

## Red checkpoint (2026-09-17)

`reference/.venv-022/bin/python -m pytest reference/tests_022 -q` at checkpoint C1: **5 errors
during collection** — every test file fails on `import chunking_study` (the script does not
exist). Files: `test_join`, `test_splitters`, `test_maxp`, `test_runs`, `test_decide`, with
`helpers_022.py` (the constants, a words-cost stub, a stub splitter) and `conftest.py`.


## The script (T010–T011)

`reference/chunking_study.py`: 27 tests green (`reference/tests_022`), and the four reference
suites collect together (115 passed) after `test_decide.py` was renamed `test_decide_022.py` —
pytest's import mode clashed with `tests_016/test_decide.py` of the same basename. The
50-document smoke (`chonky-bounded`, SciFact): 124 passages, `over_window` 0, `under_16` 0,
20 s; the limited smoke's run file first landed under `runs/` under a real cell name, against
the contract — limited runs now go under `target/xt-chunking-study/runs.limitN/`.
`ChunkInfo` needs `byte_start` / `byte_end` explicitly (`None` here: the study keeps no
byte offsets).

## The anchor (US1)

**SciFact — PASS.** `whole` (5,183 documents, 3,681 over the window), built in 544 s
(~105 ms per document); `whole-d0@100` = nDCG@10 **0.714369** / Recall@100 **0.955** —
`hybrid-baseline-v2` exactly; `whole-d20@100` = **0.720711** / **0.955** — `hybrid-rerank-v3`
exactly; every query within 1e-6. Costs as measured: the fused pass 31 s for 300 queries; the
re-ranked pass 1,003 s (~3.3 s per query, ~165 ms per pair — twice the estimate, which came
from ≤ 256-token Wikipedia passages; whole abstracts run to the cross-encoder's 512).

**NFCorpus — PASS.** `whole` (3,633 documents), built in 382 s; `whole-d0@100` = **0.353510** /
**0.321648** and `whole-d20@100` = **0.362246** / **0.321648** — the two baselines exactly,
every query within 1e-6. The re-ranked pass 948 s for 323 queries (~2.9 s per query).

## US2 — SciFact, the four variants at the study's depth

Build records (`target/xt-chunking-study/*.scifact/build.json`): `whole` 5,183 passages
(3,681 over the window), embed 542 s · `contract` 9,870 passages, 0 over, split 8 s, embed
1,000 s · `chonky` 11,430 passages, **2,311 over the window (20 %)**, split 225 s, embed
1,187 s · `chonky-bounded` 13,768 passages, 0 over, 11 under-16 remainders kept, embed
1,420 s (its split reuses chonky's cache). Median 2 passages per document for every chunker.
Re-ranked passes: `whole@300` 984 s, `contract` 543 s, `chonky` 596 s, `chonky-bounded`
445 s (shorter passages, cheaper pairs).

| variant | passages | over window | fused nDCG@10 | Δ vs whole@300 | re-ranked nDCG@10 | Δ | Recall@100 | Δ |
|---|---|---|---|---|---|---|---|---|
| whole@100 (anchor) | 5,183 | 3,681 | 0.7144 | | 0.7207 | | 0.9550 | |
| whole@300 | 5,183 | 3,681 | 0.7134 | | 0.7220 | | 0.9650 | |
| contract | 9,870 | 0 | 0.7149 | +0.0016 | 0.7303 | **+0.0083** | 0.9543 | −0.0107 |
| chonky | 11,430 | 2,311 | 0.7396 | +0.0263 | **0.7513** | **+0.0293** | 0.9627 | −0.0023 |
| chonky-bounded | 13,768 | 0 | 0.7275 | +0.0142 | 0.7469 | **+0.0249** | 0.9560 | −0.0090 |

The depth alone (`whole@100` → `whole@300`): re-ranked nDCG@10 +0.0013, Recall@100 +0.0100 —
so the chunkers are read against `whole@300`. No query returned fewer than 100 documents in
any cell.

Reading: every chunker lifts SciFact's re-ranked nDCG@10, chonky the most (+2.9 points) and
by a wide margin over the contract chunker (+0.8) — despite one fifth of its passages being
embedded from their head. Bounding those passages costs 0.4 points against raw chonky but
recovers none of the recall. Recall@100 falls for all three chunkers (three-quarters of a
point for contract, a fifth for chonky): passages compete with each other for the 300 slots.

## US2 — NFCorpus, the four variants at the study's depth

Build records: `whole` 3,633 passages (2,864 over), embed 380 s · `contract` 7,279, 0 over,
embed 715 s · `chonky` 11,546, **1,027 over (9 %)**, split 152 s, embed 1,131 s ·
`chonky-bounded` 12,039, 0 over, 12 under-16 remainders kept, embed 1,203 s. Re-ranked
passes: `whole@300` 975 s, `contract` 325 s, `chonky` 495 s, `chonky-bounded` 323 s.

| variant | passages | over window | fused nDCG@10 | Δ vs whole@300 | re-ranked nDCG@10 | Δ | Recall@100 | Δ |
|---|---|---|---|---|---|---|---|---|
| whole@100 (anchor) | 3,633 | 2,864 | 0.3535 | | 0.3622 | | 0.3216 | |
| whole@300 | 3,633 | 2,864 | 0.3550 | | 0.3621 | | 0.3242 | |
| contract | 7,279 | 0 | 0.3536 | −0.0014 | 0.3627 | +0.0005 | 0.3240 | −0.0002 |
| chonky | 11,546 | 1,027 | 0.3535 | −0.0015 | 0.3597 | −0.0024 | 0.3191 | −0.0051 |
| chonky-bounded | 12,039 | 0 | 0.3543 | −0.0007 | 0.3587 | −0.0034 | 0.3193 | −0.0048 |

The depth alone: re-ranked nDCG@10 −0.0001, Recall@100 +0.0025. No short queries.

Reading: on NFCorpus chunking changes nothing the rule would notice — the contract chunker is
flat, the two chonky variants lose a quarter to a third of a point of nDCG and half a point
of recall. NFCorpus's abstracts are as long as SciFact's (79 % over the window), so the
window is not what decides it here; its queries are short medical terms whose answers sit
in the abstract's first sentences, which the whole-document embedding already covers.

## US4 — the provisional decision (two-way: SciFact + NFCorpus)

`decide` (the rule of spec US4; constants 0.005 / 0.005 / 0.005):

| variant | mean re-ranked nDCG@10 | whole@300 | Δ mean | per dataset | Δ recall | verdict |
|---|---|---|---|---|---|---|
| contract | 0.5465 | 0.5421 | +0.0044 | scifact +0.0083, nfcorpus +0.0005 | −0.0054 | not recommended (mean gain 0.0044 < 0.005; recall −0.0054 < −0.005) |
| chonky | **0.5555** | 0.5421 | **+0.0134** | scifact +0.0293, nfcorpus −0.0024 | −0.0037 | **RECOMMENDED** |
| chonky-bounded | 0.5528 | 0.5421 | +0.0108 | scifact +0.0249, nfcorpus −0.0034 | −0.0069 | not recommended (recall −0.0069 < −0.005) |

Best chunker: **chonky** — the leader for FiQA. Both misses are narrow: contract's mean gain
is 0.0006 short and its recall 0.0004 over the line; chonky-bounded's recall 0.0019 over.
The verdict is provisional until FiQA's `whole` (the third anchor) and FiQA's chonky cells.
