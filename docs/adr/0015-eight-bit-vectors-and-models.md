# ADR-0015: Eight-bit dense vectors (format 3) and eight-bit model artefacts

- **Status**: Accepted — 2026-09-20 (the owner chose to drop the float vectors rather than rescore, to keep both artefact formats loadable, and to pin the plain eight-bit file from each repository)
- **Date**: 2026-09-20
- **Deciders**: mirth (repository owner), 2026-09-20
- **Spec**: [026-eight-bit-precision](../../specs/026-eight-bit-precision/spec.md) FR-001–FR-014
- **Amends**: [ADR-0013](0013-dense-format-v2-append-tombstone-compact.md) — the row body only; the manifest, tombstones, generations and commit protocol it defined are unchanged
- **Blocks**: Principle V and Agent Operating Rule 2 rows in [plan.md](../../specs/026-eight-bit-precision/plan.md)

## Context

Principle V requires human review and a record for a change to the on-disk format or to error
semantics; Rule 2 says the same about touching the contract uninvited. This feature changes two
contracts at once, and it does so because of what the engine costs on a device.

For the shipped Simple English Wikipedia corpus the engine holds 661 MB of vectors and 174 MB of
model weights. [ADR-0010](0010-device-rss-ceiling-600mb.md) sets a 600 MB ceiling for the full
pipeline on a phone, and every measurement this project has taken over the full corpus has had
to report that ceiling "for comparison only", because the corpus does not fit inside it. The
laptop record is 1,013 MiB; the iPhone runs a smaller slice.

Three studies under `reference/`, run before this decision, measured what eight bits would cost.
They are simulations of a scheme, not of the engine's kernels, and they were treated as a
go/no-go signal rather than as the gate:

| Study | Result |
|---|---|
| vectors, SciFact | nDCG@10 0.6451 → 0.6445, Recall@100 unchanged, candidate agreement 99.5 % at depth 100 |
| vectors, NFCorpus | nDCG@10 0.3159 → 0.3153, Recall@100 unchanged |
| vectors, rescored top 20 | the float ranking recovered exactly, on both datasets |
| embedder weights, per output channel | nDCG@10 0.6451 → 0.6471 (noise); per tensor → 0.6398, not viable |
| cross-encoder weights | nDCG@10 unchanged at 0.6865 |

## Decision

### 1. Dense format version 3

A row becomes `id u32 · norm f32 · scale f32 · codes dim×i8` — 396 bytes at dimension 384,
against 1,544 — with `scale = max|component| / 127` per vector, floored at the smallest normal
`f32`, and `code = round(component / scale)` (half away from zero) clamped to [−127, 127].
Scoring accumulates the integer products and dequantises once per row. The stored `norm` is the
norm of the row as stored, `sqrt(Σ code²) × scale`, so that cosine — over the quantised query's
norm and the row's — is the cosine of the two vectors actually compared, bounded by one. The
manifest header names the scheme (`i8-symmetric-per-vector`), so a future scheme is a header
change rather than a guess.

Everything ADR-0013 established stays: the manifest is the truth, tombstones are an embedded
bitmap, generations advance on compaction, and commits are write, sync, rename. Only the row body
changes, and fixed-width rows keep the mapped-file arithmetic the design depends on.

Version 1 and version 2 files are refused at open by name, with the rebuild instruction, exactly
as version 1 is refused today.

### 2. The float vectors are not kept

**The owner's decision.** A rescoring pass over the top twenty recovers the exact float ranking —
measured — but it needs the float rows on disk, growing the shipped corpus from 661 MB to 830 MB.
The owner chose the smaller file and accepted the measured 0.0006 nDCG@10, an eighth of the
0.005 threshold the specification holds this feature to.

The consequence is stated rather than hidden: an eight-bit index does **not** rank identically to
a float one. Determinism is untouched — the same index, query and configuration give the same
scores — but "the same as before" is no longer a promise the stage makes, and the specification's
requirements were loosened from identity to measured agreement when this was decided.

### 3. Both model artefact formats load

The engine accepts the as-published float weights and an eight-bit quantised artefact, with the
model manifest deciding which an installation uses. Each fingerprint names the artefact it was
loaded from, so an index records which weights produced it and a mismatch is caught at open.

The artefacts pinned by this feature are supplied by the owner, as all model weights are:
`all-MiniLM-L6-v2.Q8_0.gguf` from `leliuga/all-MiniLM-L6-v2-GGUF` and
`ms-marco-MiniLM-L-6-v2-q8_0.gguf` from `cstr/ms-marco-MiniLM-L-6-v2-GGUF`. Both were inspected
on 2026-09-20: eight-bit weight matrices with float norms, biases and token types, and the
re-ranker carries its classification head, without which a cross-encoder produces embeddings
rather than relevance scores. Nothing is quantised in this repository.

The tokenizer continues to come from the pinned float model directory, because neither quantised
repository ships one. The artefact supplies weights only.

**The re-ranker's pooler is borrowed too** (owner's decision, 2026-09-21). The published
cross-encoder scores `classifier(tanh(pooler(CLS)))`; the pinned eight-bit file carries the
classifier bit for bit but not the 384×384 pooler that feeds it (103 tensors against the float
file's 105). Fed as published it would compute a function the head was never trained for. The
two pooler tensors are copied byte for byte out of the pinned float weights into
`pooler.safetensors` by `scripts/extract_tensors.py`, pinned by size and hash in the manifest and
staged by the fetch script beside the artefact, and the identity string names them. Nothing is
converted: the bytes are the pinned float model's, which is the same rule the tokenizer follows.
The alternatives were a different artefact that carries its pooler, or keeping the re-ranker
float; the owner chose the borrow.

## Alternatives rejected

- **A single global scale for the vectors** measured marginally better (0.6449 and 0.3157) and
  saves four bytes a row, but it ties every future corpus to one distribution, with no signal
  when a different corpus quantises worse.
- **Keeping the float rows for rescoring**: exact, and rejected on size by the owner.
- **Four bits**: not measured, and small encoders lose more to aggressive quantisation than large
  language models do. Not proposed.
- **A neural accelerator** (the Apple Neural Engine, a vendor delegate): a second model
  implementation to maintain, and outside the pinned inference engine. Its own decision, later.
- **Converting the weights here** rather than taking published artefacts: ruled out by the owner,
  and it would leave a file with no provenance while the fingerprint still claimed one.

## Consequences

**Everything is rebuilt once.** The format changes and the embedder changes, so every artefact —
the Wikipedia corpus, the fixture index, the demo slices, every evaluation cache, every golden
that records a score — is regenerated within this feature. That is why the two halves are one
feature rather than two.

**The quality gate is stated before the numbers are known**: no dataset may fall more than 0.005
below its committed baseline on nDCG@10 or Recall@100, on any of SciFact, NFCorpus and FiQA. A
larger drop stops the feature and is reported. The threshold follows Feature 022's existing
decision rule and is not to be widened (Rule 6).

**No speed claim is made.** A kernel probe outside the repository scanned a Wikipedia-sized
matrix in 3.7–7.8 ms against 16 ms for a fair float kernel, but the owner waived a probe against
the engine's own kernels, so this record claims size and quality only. The benchmark is recorded
for the change, whatever it says.

**Provenance is weaker than for the float weights.** Both artefacts are third-party conversions,
not publications by the model authors; the embedder's repository has around 415,000 downloads,
the re-ranker's around 1,100. They are pinned by revision and checksum like everything else, and
the three-dataset gate is what decides whether they ship.

## When to revisit

If the dense stage ever needs exact float scoring — a corpus where the measured agreement falls
below the threshold would be the signal — the rescoring pass is the known answer and this record
should be revised rather than worked around. If a four-bit scheme is ever measured on all three
datasets, it belongs here too. If an accelerator path is adopted, the model half of this record
is superseded.
