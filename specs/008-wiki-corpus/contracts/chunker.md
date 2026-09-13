# Contract: the chunker (`xtriever_analysis::chunk`)

**Feature**: `008-wiki-corpus` | pure crate, `std`-only | golden-tested against `reference/gen_008_fixtures.py`

```rust
/// One passage of a body: the text the index stores and the range it came from.
pub struct Passage { pub text: String, pub byte_range: (u64, u64), pub cost: usize }

/// Split `body` into passages whose summed unit cost is at most `budget`.
///
/// `cost` prices one unit of text (a paragraph, sentence, word or word fragment) and MUST be
/// additive over whitespace-joined units for the tiling guarantee to hold; the caller verifies
/// the finished passages against its real limit (FR-006).
pub fn chunk(body: &str, budget: usize, cost: &dyn Fn(&str) -> usize) -> Vec<Passage>;
```

## The algorithm (normative — the Python reference implements exactly this)

1. **Paragraphs**: split `body` on blank lines — a `\n` followed by any run of horizontal
   whitespace (` `, `\t`, `\r`) and another `\n`. Each paragraph is trimmed of leading and
   trailing whitespace and its internal whitespace runs are collapsed to one space; an empty
   result is dropped. A paragraph's byte range is that of its trimmed text in `body`.
2. **Sentences** (used only when a paragraph does not fit alone): split after `.`, `!` or `?`
   when followed by at least one whitespace character; the terminator stays with the sentence;
   each sentence is trimmed. No abbreviation handling — `Mr. Smith` is two sentences, by
   definition, on both sides of the oracle.
3. **Words**: split on whitespace runs.
4. **Fragments** (a single word with `cost > budget`): split the word at the UTF-8 character
   boundary nearest its byte midpoint; recurse on each half until every fragment fits.
5. **Packing**: walk units in order at the coarsest level; keep a current passage and its
   cost; a unit is appended if `current_cost + cost(unit) ≤ budget`, else the current passage
   is closed and the unit starts a new one. A unit that does not fit alone is replaced by its
   finer-level units and packing continues at that level for that unit only (a paragraph's
   sentences may share a passage with preceding units of the same level only if they fit —
   packing never reorders and never looks ahead).
6. **Text**: units of one passage are joined by one space, except that a paragraph boundary
   inside a passage is a single `\n`.
7. **Ranges**: `byte_range.0` is the start of the passage's first unit, `byte_range.1` is one
   past the end of its last unit, both in `body`. Across a body, ranges are strictly increasing
   and non-overlapping; the bytes not covered are whitespace only.
8. **Budget 0 or empty body**: an empty `Vec`.

## Costs used

| Where | `cost` | `budget` |
|---|---|---|
| Golden fixtures, set A | number of whitespace-separated words | 1, 3, 8, 40 |
| Golden fixtures, set B; the build | `MiniLmEmbedder::token_count` (word-pieces incl. `[CLS]`/`[SEP]`) — for the Python side `tokenizers` 0.23.2 with the pinned `tokenizer.json`, truncation off | `254 − token_count(title)` |

Set A exercises every branch with no model; set B pins the production behaviour. Both are
compared byte for byte (text and ranges and cost).

## Properties (property-tested in `xtriever-analysis`)

- Tiling: for any body and budget ≥ 1, ranges are increasing, non-overlapping, and every
  non-whitespace byte of `body` lies inside exactly one range.
- Bound: every passage has `cost ≤ budget` unless it is a single fragment of an over-budget
  word, in which case it is the smallest fragment that fits.
- Determinism: `chunk` is a pure function of its inputs.
- Idempotence of text: re-chunking a passage's `text` with the same cost and budget yields one
  passage with the same text.
