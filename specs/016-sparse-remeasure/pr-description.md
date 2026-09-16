# 016 sparse re-measurement — 012's GO withdrawn

**The question**: 012 said GO to an inference-free sparse stage on +1.6 mean nDCG@10 over a
0.468 pipeline. The pipeline is now at 0.4913 (013 joined lexical field, 015 interpolated
re-rank). Does the sparse list still earn its cost — 150–255 MB of postings on the phone and a
corpus encoder that cannot run in Rust?

**Method** (offline, from artefacts on disk, no Rust): the engine's own lexical-v2 and dense
candidate lists (014's explain exports) fused with 012's doc-v3 dot list under the engine's
RRF (ties by corpus position); re-ranked under the engine's interpolating rule (014's code)
with the engine's cross-encoder scores where the exports cover the pair (92–97 % of head
pairs) and the 006 torch reference otherwise — whose agreement with the engine on covered
pairs is 1.2 × 10⁻⁵ at worst. Two anchors reproduced exactly: `lex2+dense` = `hybrid-baseline-v2`
list for list; re-ranked = `hybrid-rerank-v3` list for list with zero reference pairs.

## Result (nDCG@10; Δ vs the matching anchor)

| row | scifact | nfcorpus | fiqa | mean |
|---|---|---|---|---|
| hybrid-baseline-v2 (anchor) | 0.7144 | 0.3535 | 0.3692 | 0.4790 |
| lex2+dense+dot, un-re-ranked | 0.7234 (+0.0091) | 0.3544 (+0.0009) | 0.3884 (+0.0192) | 0.4887 (+0.0097) |
| hybrid-rerank-v3 (anchor) | 0.7207 | 0.3622 | 0.3910 | 0.4913 |
| **lex2+dense+dot, re-ranked** | 0.7230 (+0.0023) | 0.3624 (+0.0002) | 0.3968 (+0.0058) | **0.4941 (+0.0028)** |
| dense+dot, re-ranked | 0.7136 (−0.0071) | 0.3511 (−0.0112) | 0.4101 (+0.0192) | 0.4916 (+0.0003) |
| lex2+dot, re-ranked | 0.7152 (−0.0055) | 0.3599 (−0.0023) | 0.3489 (−0.0420) | 0.4747 (−0.0166) |

## Decision (rule fixed in the spec before any run)

Specify the stage only if the re-ranked three-way mean ≥ 0.4913 + 0.005 = 0.4963 with no
dataset > 0.005 below v3. It reaches **0.4941** — nothing hurt, floor not met. **012's GO is
withdrawn.** 013 and 015 took the gain by cheaper means: the joined field removed the NFCorpus
contribution, the interpolating re-ranker already reads the cross-encoder's view of the head.

Reopening conditions (in `runs/decision.json` and the report): a FiQA-shaped corpus wanting a
lexical-free configuration (`dense+dot` reaches 0.4101 there, +1.9 over v3, while losing on
the titled sets); an encoder runnable at index time in Rust; a head need the cross-encoder
does not cover; a change to the fused signal.

## Changes

- `reference/sparse_remeasure.py` (+ `reference/tests_016/`, 13 tests, committed red first);
  24 cells, `table.{json,md}`, `decision.json`, `report.md` under `specs/016-sparse-remeasure/`;
  pointers in the 012 and 014 reports.
- **Unchanged**: everything under `crates/`, every baseline, every earlier cell; the 012
  encodings and manifests stay for the reopening cases.

Gate: nothing under `crates/` changed (fmt, clippy 0, nextest 279/279, deny re-run once) ·
pytest 016 13/13, 014 57/57 · both anchors reproduced · no identifiers.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
