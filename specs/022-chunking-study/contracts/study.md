# Contract: `reference/chunking_study.py`

Run with `reference/.venv-022/bin/python`. Every subcommand is idempotent and resumable.

```
chunking_study.py build   --variant V --dataset D [--limit N]     # split (cached for chonky), index under target/xt-chunking-study/V.D/, write build.json
chunking_study.py search  --variant V --dataset D --depth {0,20} --k {100,300}   # run file under specs/022-chunking-study/runs/ (skipped if present)
chunking_study.py score   --dataset D [--variant V]              # scores for every run file present
chunking_study.py check   --dataset D                            # whole-d0@100 vs hybrid-baseline-v2, whole-d20@100 vs hybrid-rerank-v3 → PASS / the first difference
chunking_study.py table                                          # the markdown table of every scored cell with deltas vs whole@300 and the @100/@300 difference
chunking_study.py decide  [--owner-decision FILE]                # the rule; prints the verdict and writes owner-decision.json when asked
chunking_study.py all     --dataset D                            # whole build → check → the chunked builds → all cells → score
```

Variants: `whole`, `contract`, `chonky`, `chonky-bounded`, `chonky-if-long` (the owner's
amendment of 2026-09-18: chonky only for over-window documents). Datasets: `scifact`, `nfcorpus`,
`fiqa`. `--limit N` (first N documents) is for smoke tests only and names the index
`V.D.limitN` — never a committed cell.

Files:

- `target/xt-chunking-study/<V>.<D>/index/…` + `build.json`; `target/xt-chunking-study/chonky.<D>.jsonl` (the cached split: `{"_id", "chunks": [...]}` per document).
- `specs/022-chunking-study/runs/<V>-d<depth>@<k>.<D>.jsonl` (run) and `.json` (scores); `runs/build-records.json` (every `build.json`, keyed `V.D`).
- `specs/022-chunking-study/owner-decision.json`.

Exit codes: 0; 1 on a failed anchor check, a non-partition, a missing input (the message names the producer); 2 on usage.

The rule's constants — `MEAN_GAIN = 0.005`, `MAX_DROP = 0.005`, `RECALL_DROP = 0.005`, `MIN_POSITIONS = 16`, `WINDOW = 256` — are module literals asserted by `reference/tests_022/`.
