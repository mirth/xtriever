# Contract: the pinned model artefacts (Feature 026)

What an installation may load for each model, how it is pinned, and what the engine refuses.

## The two supported forms

| | Float | Eight-bit |
|---|---|---|
| File | as-published float weights | a quantised file naming its architecture and shape |
| Chosen by | the model manifest | the model manifest |
| Fingerprint | names the float artefact | names the eight-bit artefact |
| Tokenizer | from the model directory | **also from the float model directory** |

Both remain loadable (spec FR-012). Which one an installation uses is a property of the manifest
it fetched, not a runtime switch, so an index's fingerprint always names exactly what produced
it.

## Pinning

Each artefact is named in the manifest by repository, revision and per-file checksum, exactly as
the float weights are today, and `scripts/fetch-model.sh` fetches what the manifest names. A file
whose bytes do not match is refused with the mismatch named. Nothing is converted locally: the
artefacts enter this project as published files, which is what makes a fingerprint a claim anyone
else can verify.

The artefacts this feature pins:

| Model | Repository | File | Size |
|---|---|---|---|
| embedder | `leliuga/all-MiniLM-L6-v2-GGUF` | `all-MiniLM-L6-v2.Q8_0.gguf` | 25.0 MB |
| re-ranker | `cstr/ms-marco-MiniLM-L-6-v2-GGUF` | `ms-marco-MiniLM-L-6-v2-q8_0.gguf` | 24.7 MB |

## What the engine checks at load

1. **The checksum**, before anything is parsed.
2. **The architecture and shape** the file declares — block count, embedding length, head count,
   context length — against what the stage expects.
3. **The classification head**, for the re-ranker only: a cross-encoder without one produces
   embeddings rather than relevance scores, and must be refused rather than used.
4. **The tokenizer**, still fetched and checksummed from the float model directory.

Any of these failing is a named error, never a warning and never a fallback.

## Fingerprints

Each fingerprint gains the artefact it was loaded from, so that two installations using different
precisions are distinguishable by the strings their indexes record. An index whose recorded
embedder fingerprint differs from the loaded embedder's is refused at open, as it is today.

## What this contract does not promise

Quality for any model other than the pinned pair. Every committed baseline in this repository
belongs to those two artefacts. Loading a different model is out of scope for this feature and is
recorded in the specification as the next one.
