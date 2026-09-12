# ADR-0008: Pipeline on-disk format version 2 — every hybrid index stores its passage text

- **Status**: Accepted — 2026-09-13 (option (b) chosen by the repository owner; lands with the Feature 006 PR that bumps the version)
- **Date**: 2026-09-13
- **Deciders**: mirth (repository owner), 2026-09-13
- **Spec**: [006-rerank-stage](../../specs/006-rerank-stage/spec.md) FR-010, FR-022; user
  decision Q2 = A (2026-09-13)
- **Blocks**: Principle V row in [plan.md](../../specs/006-rerank-stage/plan.md)

## Context

Principle V: "the on-disk format … unchanged — or the change has human review plus an ADR".
Feature 005 defined the pipeline's format version 1: `xtriever-pipeline.json` (descriptor),
`ids.json` (id map), `lexical/`, `dense/` ([005 data-model](../../specs/005-hybrid-pipeline/data-model.md)).

The re-rank stage needs the passage *text* of a candidate at query time. Neither stage can
return it — `LexicalIndex` has no fetch method and Feature 006 may not add one (spec FR-022,
Agent Operating Rule 2) — so the pipeline must keep it. The user chose (spec Q2 = A) that
**every** hybrid index keeps it, whether or not a re-ranker is attached: hits become
self-describing for RAG callers and any index can gain a re-ranker later without a rebuild.

A directory written by version 1 has no text store. There are two ways to admit that:

- **(a)** keep version 1, make the store optional at open, and error only when a re-ranker is
  attached to an index without one — two shapes of index, the very thing Q2 = A rejected, and
  a store that `add` could not keep complete on a directory that started without it;
- **(b)** bump the pipeline format to version 2, in which the store is mandatory, and refuse
  version 1 directories at open by the existing version check.

## Decision

**(b).** `xtriever_pipeline::FORMAT_VERSION` becomes **2**. A version-2 directory additionally
contains `passages.bin` (research D7 in the 006 plan: magic `XTPASS01`, JSON header, an offset
table, a UTF-8 text block; position = internal id; whole-file `.tmp` + `rename` at commit; read
on demand) and the descriptor gains `rerank_depth`. `Descriptor::read` refuses any other
version naming both numbers and saying "rebuild the index". **No migration is written**: the
only version-1 directories in existence are the harness's throwaway index directories and test
temporary directories; a migration would be code with no user.

Commit order becomes lexical → dense → passages → id map → descriptor under the same
`commit.pending` marker; `open`'s count check gains the store's count (which must equal the id
map's length).

## Consequences

**Positive**

- One shape of index; `HybridHit` carries its text; the re-ranker reads exactly the text the
  embedder saw.
- The version check that already exists does the refusing — no new failure mode.

**Negative**

- Disk: the corpus text once more (FiQA: ~48 MB beside a 103 MB index; measured in the 006
  report). RSS is unaffected: the text is read per hit, never loaded whole.
- Commit rewrites the store whole, like the dense stage's `index.bin` — acceptable at the
  current scale, measured on FiQA, and a segmented store is a later feature if it is not.
- Any external tool that read version-1 directories must be rebuilt; none exists.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| (a) optional store under version 1 | Two index shapes; an index created without the store can never become complete; contradicts Q2 = A |
| Text inside `ids.json` | 48 MB of JSON parsed whole at open and rewritten per commit; the id map is meant to stay small (8.8 B/doc) |
| Read stored fields back from tantivy | Requires a `LexicalIndex` / `TantivyIndex` API change — a core-trait or stage change this feature is forbidden to make |
| One file per document | 100k files; no atomic commit |

## Review trigger

Revisit when a consumer outside this repository holds version-2 directories, at which point
version bumps need a migration path rather than a refusal.
