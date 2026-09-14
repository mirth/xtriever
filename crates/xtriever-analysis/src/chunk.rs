//! Chunking: a body of text into passages under a caller-supplied cost (Feature 008,
//! `specs/008-wiki-corpus/contracts/chunker.md`). Pure and `std`-only: the cost function is
//! the only thing that knows about tokenizers.
//!
//! The contract, in eight steps, each a paragraph here:
//!
//! 1. **Paragraphs** — the body is split on blank lines (`\n`, then any run of space, tab or
//!    carriage return, then `\n`); each paragraph is trimmed of whitespace and its internal
//!    whitespace runs are collapsed to one space; empty paragraphs are dropped.
//! 2. **Sentences** — only when a paragraph does not fit alone: split after `.`, `!` or `?`
//!    followed by whitespace, the terminator staying with the sentence; each sentence trimmed
//!    and collapsed. No abbreviation handling — `Mr. Smith` is two sentences, by definition.
//! 3. **Words** — only when a sentence does not fit alone: the maximal non-whitespace runs.
//! 4. **Fragments** — only when a single word does not fit alone: split at the first UTF-8
//!    character boundary at or after half its bytes, recursively, until every fragment fits;
//!    a one-character fragment is emitted even if it does not.
//! 5. **Packing** — one current passage shared across levels; a unit that does not fit alone
//!    is replaced by its finer units in order; a unit that fits alone but not in the current
//!    passage closes it and starts the next. Nothing reorders, nothing looks ahead.
//! 6. **Text** — a passage's units are joined by one space, except that a unit beginning a
//!    paragraph is joined to a non-empty passage by `\n`.
//! 7. **Ranges** — `byte_range` runs from the first unit's first byte to one past the last
//!    unit's last byte, in the body; ranges are increasing and non-overlapping; only
//!    whitespace lies between them.
//! 8. **Cost** — a passage's cost is the sum of its units' costs; budget 0 or an empty body
//!    yields nothing.
//!
//! "Whitespace" is exactly [`char::is_whitespace`] (Unicode `White_Space`); the Python
//! reference in `reference/gen_008_fixtures.py` uses the same code-point list.

/// One passage of a body: the text the index stores and the range it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passage {
    /// The units joined per step 6.
    pub text: String,
    /// `(start, end)` byte offsets in the body — step 7.
    pub byte_range: (u64, u64),
    /// The summed unit cost — step 8.
    pub cost: usize,
}

/// A unit at any level, with its text and its slice of the body.
struct Unit {
    text: String,
    start: usize,
    end: usize,
    /// The paragraph itself, or the first sentence / word / fragment derived from it.
    starts_paragraph: bool,
}

#[derive(Clone, Copy)]
enum Level {
    Paragraph,
    Sentence,
    Word,
    Fragment,
}

/// Split `body` into passages whose summed unit cost is at most `budget`.
///
/// `cost` prices one unit of text (a paragraph, sentence, word or word fragment). For the
/// tiling and bound guarantees it should be additive over whitespace-joined units — true of
/// word counts and of WordPiece content pieces (BERT pre-tokenises on whitespace and
/// punctuation); the caller verifies finished passages against its real limit regardless.
#[must_use]
pub fn chunk(body: &str, budget: usize, cost: &dyn Fn(&str) -> usize) -> Vec<Passage> {
    if budget == 0 || body.is_empty() {
        return Vec::new();
    }
    let mut packer = Packer {
        body,
        budget,
        cost,
        passages: Vec::new(),
        current: Vec::new(),
        current_cost: 0,
    };
    for (start, end) in paragraphs(body) {
        let text = collapse(&body[start..end]);
        packer.place(
            Unit {
                text,
                start,
                end,
                starts_paragraph: true,
            },
            Level::Paragraph,
        );
    }
    packer.flush();
    packer.passages
}

struct Packer<'a> {
    body: &'a str,
    budget: usize,
    cost: &'a dyn Fn(&str) -> usize,
    passages: Vec<Passage>,
    current: Vec<Unit>,
    current_cost: usize,
}

impl Packer<'_> {
    /// Step 5.
    fn place(&mut self, unit: Unit, level: Level) {
        let c = (self.cost)(&unit.text);
        if c <= self.budget {
            self.append(unit, c);
            return;
        }
        let body = self.body;
        match level {
            Level::Paragraph => {
                for (k, (s, e)) in sentences(body, unit.start, unit.end)
                    .into_iter()
                    .enumerate()
                {
                    self.place(
                        Unit {
                            text: collapse(&body[s..e]),
                            start: s,
                            end: e,
                            starts_paragraph: unit.starts_paragraph && k == 0,
                        },
                        Level::Sentence,
                    );
                }
            }
            Level::Sentence => {
                for (k, (s, e)) in words(body, unit.start, unit.end).into_iter().enumerate() {
                    self.place(
                        Unit {
                            text: body[s..e].to_owned(),
                            start: s,
                            end: e,
                            starts_paragraph: unit.starts_paragraph && k == 0,
                        },
                        Level::Word,
                    );
                }
            }
            Level::Word => {
                let parts = fragments(body, unit.start, unit.end, self.budget, self.cost);
                if parts.len() == 1 {
                    // A one-character word that still does not fit: emitted as is (step 4).
                    self.append(unit, c);
                    return;
                }
                for (k, (s, e)) in parts.into_iter().enumerate() {
                    self.place(
                        Unit {
                            text: body[s..e].to_owned(),
                            start: s,
                            end: e,
                            starts_paragraph: unit.starts_paragraph && k == 0,
                        },
                        Level::Fragment,
                    );
                }
            }
            // `fragments` only returns parts that fit or are one character; either way there
            // is nothing finer.
            Level::Fragment => self.append(unit, c),
        }
    }

    fn append(&mut self, unit: Unit, c: usize) {
        if !self.current.is_empty() && self.current_cost + c > self.budget {
            self.flush();
        }
        self.current.push(unit);
        self.current_cost += c;
    }

    /// Steps 6–8.
    fn flush(&mut self) {
        let (Some(first), Some(last)) = (self.current.first(), self.current.last()) else {
            return;
        };
        let byte_range = (first.start as u64, last.end as u64);
        let mut text = String::new();
        for unit in &self.current {
            if !text.is_empty() {
                text.push(if unit.starts_paragraph { '\n' } else { ' ' });
            }
            text.push_str(&unit.text);
        }
        self.passages.push(Passage {
            text,
            byte_range,
            cost: self.current_cost,
        });
        self.current.clear();
        self.current_cost = 0;
    }
}

/// Step 1: trimmed, non-empty paragraph slices.
fn paragraphs(body: &str) -> Vec<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut j = i + 1;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\r') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'\n' {
                push_trimmed(body, start, i, &mut out);
                start = j + 1;
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    push_trimmed(body, start, bytes.len(), &mut out);
    out
}

/// Step 2: trimmed, non-empty sentence slices of `body[start..end]`.
fn sentences(body: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut s = start;
    let slice = &body[start..end];
    let mut chars = slice.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if matches!(c, '.' | '!' | '?')
            && let Some(&(_, next)) = chars.peek()
            && next.is_whitespace()
        {
            let cut = start + i + c.len_utf8();
            push_trimmed(body, s, cut, &mut out);
            s = cut;
        }
    }
    push_trimmed(body, s, end, &mut out);
    out
}

/// Step 3: maximal non-whitespace runs of `body[start..end]`.
fn words(body: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let slice = &body[start..end];
    let mut run: Option<usize> = None;
    for (i, c) in slice.char_indices() {
        match (c.is_whitespace(), run) {
            (true, Some(s)) => {
                out.push((start + s, start + i));
                run = None;
            }
            (false, None) => run = Some(i),
            _ => {}
        }
    }
    if let Some(s) = run {
        out.push((start + s, end));
    }
    out
}

/// Step 4: halve at the first char boundary at or after half the bytes until each part fits.
fn fragments(
    body: &str,
    start: usize,
    end: usize,
    budget: usize,
    cost: &dyn Fn(&str) -> usize,
) -> Vec<(usize, usize)> {
    let text = &body[start..end];
    if cost(text) <= budget || text.chars().count() <= 1 {
        return vec![(start, end)];
    }
    let mut split = text.len() / 2;
    while split < text.len() && !text.is_char_boundary(split) {
        split += 1;
    }
    if split == 0 || split >= text.len() {
        return vec![(start, end)];
    }
    let mut out = fragments(body, start, start + split, budget, cost);
    out.extend(fragments(body, start + split, end, budget, cost));
    out
}

fn push_trimmed(body: &str, start: usize, end: usize, out: &mut Vec<(usize, usize)>) {
    let slice = &body[start..end];
    let trimmed = slice.trim_start();
    let lead = slice.len() - trimmed.len();
    let trimmed = trimmed.trim_end();
    if !trimmed.is_empty() {
        out.push((start + lead, start + lead + trimmed.len()));
    }
}

/// Whitespace runs collapsed to one space.
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}
