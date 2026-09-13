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

**Whitespace** throughout means exactly the Unicode `White_Space` code points Rust's
`char::is_whitespace` accepts: U+0009–U+000D, U+0020, U+0085, U+00A0, U+1680, U+2000–U+200A,
U+2028, U+2029, U+202F, U+205F, U+3000. Both sides use this list, never a library's notion.

1. **Paragraphs**: split `body` on blank lines — a `\n`, then any run of ` `, `\t`, `\r`, then
   another `\n`. Each paragraph is trimmed of leading and trailing whitespace; its **range** is
   that of the trimmed slice in `body`; its **text** is the slice with every whitespace run
   collapsed to one space. An empty result is dropped.
2. **Sentences** (only when a paragraph does not fit alone): within the paragraph's slice,
   split after `.`, `!` or `?` when the next character is whitespace; the terminator stays
   with the sentence; each sentence is trimmed; range and text as for paragraphs. No
   abbreviation handling — `Mr. Smith` is two sentences, by definition, on both sides.
3. **Words** (only when a sentence does not fit alone): the maximal non-whitespace runs of the
   sentence's slice; range exact; text = the slice.
4. **Fragments** (only when a single word does not fit alone): split the word's slice at the
   first UTF-8 character boundary at or after `floor(len / 2)` bytes; recurse on each half
   until every fragment fits. A one-character fragment is emitted even if it does not fit.
5. **Packing**: one current passage (units, cost) shared across levels. `place(unit)`: if
   `cost(unit.text) > budget`, replace the unit by its finer units and `place` each in order;
   else if `current.cost + cost(unit.text) ≤ budget`, append; else close the current passage
   and start a new one with the unit. Units are never reordered; nothing looks ahead. A
   refined paragraph's first sentences may therefore join a passage that already holds earlier
   whole paragraphs.
6. **Text**: units of one passage are joined by one space, except that a unit which begins a
   paragraph (the paragraph itself, or the first sentence / word / fragment derived from it)
   is joined to a non-empty passage by `\n`. Fragments of one word are thus joined by a space.
7. **Ranges**: `byte_range.0` is the start of the passage's first unit, `byte_range.1` is one
   past the end of its last unit, both in `body`. Across a body, ranges are strictly increasing
   and non-overlapping; the bytes not covered are whitespace only.
8. **Cost**: a passage's `cost` is the sum of its units' costs (not the cost of its joined
   text). **Budget 0 or empty body**: an empty `Vec`.

## Costs used

| Where | `cost(text)` | `budget` |
|---|---|---|
| Golden fixtures, set A | number of maximal non-whitespace runs (words) | 1, 3, 8, 40 |
| Golden fixtures, set B; the build | **content** word-pieces: `token_count(text) − 2` (the pinned embedder's tokenizer, truncation off, minus `[CLS]`/`[SEP]`) — additive over whitespace-joined units because BERT pre-tokenises on whitespace and punctuation before WordPiece | `256 − token_count(title)` = `254 − pieces(title)`, so `[CLS] title body [SEP]` fits 256 |

Set A exercises every branch but fragments (a word costs 1, so it always fits at budget ≥ 1);
set B covers fragments with a real long token. Both are compared byte for byte (text, ranges,
cost). Set B's fixture carries a `unit_costs` table so the Rust golden test needs no tokenizer.

## Properties (property-tested in `xtriever-analysis`)

- Tiling: for any body and budget ≥ 1, ranges are increasing, non-overlapping, and every
  non-whitespace byte of `body` lies inside exactly one range.
- Bound: every passage has `cost ≤ budget` unless it is a single fragment of an over-budget
  word, in which case it is the smallest fragment that fits.
- Determinism: `chunk` is a pure function of its inputs.
- Idempotence of text, modulo the paragraph joiner: re-chunking a passage's `text` (whose
  cost is within budget) with the same cost and budget yields exactly one passage whose text
  is the original with each `\n` replaced by a space — a lone `\n` is not a blank line, so
  the joined paragraphs read back as one.
