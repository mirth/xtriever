#!/usr/bin/env python3
"""Golden fixtures for the Feature 008 chunker (specs/008-wiki-corpus/contracts/chunker.md).

An *independent* implementation of the contract's eight steps — written from the contract, not
from the Rust — producing two fixture sets the Rust golden test compares byte for byte:

* **set A** (``chunk_a.json``): twelve hand-written bodies, cost = number of words, budgets
  1 / 3 / 8 / 40. Every branch but fragments (a word costs 1, so it always fits).
* **set B** (``chunk_b.json``): eight real Simple English Wikipedia articles, cost = *content*
  word-pieces of the pinned embedder's tokenizer (``token_count − 2``), budget
  ``256 − token_count(title)``, plus one synthetic article with a small explicit budget to reach
  step 4 (fragments): with the real cost, fragments are unreachable — BERT pre-tokenises on
  punctuation, and a pre-token over 100 characters becomes a single ``[UNK]`` piece, so no word
  can cost more than ~100 pieces against a ~250 budget. Covers long articles and the corpus's
  longest token (a 392-character URL, 218 pieces).
  Carries a ``unit_costs`` table so the Rust test needs no tokenizer, and re-tokenises every
  passage as ``title\\n\\nbody`` to assert it fits 256 positions — the FR-006 property.

Run via the pinned virtualenv (research D5):

    scripts/setup-reference-venv.sh 008
    reference/.venv-008/bin/python reference/gen_008_fixtures.py
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

from tokenizers import Tokenizer

REPO_ROOT = Path(__file__).resolve().parent.parent
OUT = REPO_ROOT / "reference/fixtures/008"
JSONL = REPO_ROOT / "reference/datasets/wiki/simple.jsonl"
TOKENIZER = REPO_ROOT / "reference/models/all-MiniLM-L6-v2/tokenizer.json"

# Contract "Whitespace": exactly Rust's `char::is_whitespace` (Unicode White_Space).
WHITESPACE = frozenset(
    [chr(c) for c in range(0x09, 0x0E)]
    + [" ", "", " ", " "]
    + [chr(c) for c in range(0x2000, 0x200B)]
    + [" ", " ", " ", " ", "　"]
)
BLANK_LINE_INNER = frozenset(" \t\r")
TERMINATORS = frozenset(".!?")


# ---------------------------------------------------------------- the contract, step by step

def is_ws(ch: str) -> bool:
    return ch in WHITESPACE


def trim(body: str, start: int, end: int) -> tuple[int, int]:
    """Byte-free char offsets: shrink [start, end) past leading/trailing whitespace."""
    while start < end and is_ws(body[start]):
        start += 1
    while end > start and is_ws(body[end - 1]):
        end -= 1
    return start, end


def collapse(s: str) -> str:
    out: list[str] = []
    in_ws = False
    for ch in s:
        if is_ws(ch):
            if not in_ws:
                out.append(" ")
            in_ws = True
        else:
            out.append(ch)
            in_ws = False
    return "".join(out)


def paragraphs(body: str) -> list[tuple[int, int]]:
    """Step 1: char ranges of trimmed paragraphs, split on `\\n[ \\t\\r]*\\n`."""
    out: list[tuple[int, int]] = []
    start = 0
    i = 0
    n = len(body)
    while i < n:
        if body[i] == "\n":
            j = i + 1
            while j < n and body[j] in BLANK_LINE_INNER:
                j += 1
            if j < n and body[j] == "\n":
                out.append((start, i))
                start = j + 1
                i = j + 1
                continue
        i += 1
    out.append((start, n))
    ranges = []
    for s, e in out:
        s, e = trim(body, s, e)
        if s < e:
            ranges.append((s, e))
    return ranges


def sentences(body: str, start: int, end: int) -> list[tuple[int, int]]:
    """Step 2: split after . ! ? when followed by whitespace; trimmed."""
    out: list[tuple[int, int]] = []
    s = start
    i = start
    while i < end:
        if body[i] in TERMINATORS and i + 1 < end and is_ws(body[i + 1]):
            out.append((s, i + 1))
            s = i + 1
        i += 1
    out.append((s, end))
    ranges = []
    for a, b in out:
        a, b = trim(body, a, b)
        if a < b:
            ranges.append((a, b))
    return ranges


def words(body: str, start: int, end: int) -> list[tuple[int, int]]:
    """Step 3: maximal non-whitespace runs."""
    out = []
    i = start
    while i < end:
        while i < end and is_ws(body[i]):
            i += 1
        if i >= end:
            break
        j = i
        while j < end and not is_ws(body[j]):
            j += 1
        out.append((i, j))
        i = j
    return out


def byte_len(s: str) -> int:
    return len(s.encode("utf-8"))


def fragments(body: str, start: int, end: int, budget: int, cost) -> list[tuple[int, int]]:
    """Step 4: halve at the first char boundary at or after floor(bytes/2) until each fits."""
    text = body[start:end]
    if cost(text) <= budget or len(text) <= 1:
        return [(start, end)]
    half = byte_len(text) // 2
    # first char boundary at or after `half` bytes
    acc = 0
    split = len(text)
    for k, ch in enumerate(text):
        if acc >= half:
            split = k
            break
        acc += byte_len(ch)
    if split == 0 or split == len(text):
        return [(start, end)]
    return fragments(body, start, start + split, budget, cost) + fragments(body, start + split, end, budget, cost)


def chunk(body: str, budget: int, cost) -> list[dict]:
    """Steps 5–8. Returns [{text, byte_range, cost}] with byte offsets."""
    if budget == 0 or not body:
        return []
    # char offset -> byte offset table
    byte_at = [0]
    for ch in body:
        byte_at.append(byte_at[-1] + byte_len(ch))

    passages: list[dict] = []
    cur_units: list[tuple[str, int, int, bool]] = []  # (text, start, end, starts_paragraph)
    cur_cost = 0

    def flush():
        nonlocal cur_units, cur_cost
        if not cur_units:
            return
        text = ""
        for t, _, _, starts_para in cur_units:
            if text:
                text += "\n" if starts_para else " "
            text += t
        passages.append({
            "text": text,
            "byte_range": [byte_at[cur_units[0][1]], byte_at[cur_units[-1][2]]],
            "cost": cur_cost,
        })
        cur_units, cur_cost = [], 0

    def place(text: str, start: int, end: int, starts_para: bool, level: str):
        nonlocal cur_cost
        c = cost(text)
        if c > budget:
            if level == "paragraph":
                subs = sentences(body, start, end)
                for k, (a, b) in enumerate(subs):
                    place(collapse(body[a:b]), a, b, starts_para and k == 0, "sentence")
            elif level == "sentence":
                subs = words(body, start, end)
                for k, (a, b) in enumerate(subs):
                    place(body[a:b], a, b, starts_para and k == 0, "word")
            elif level == "word":
                subs = fragments(body, start, end, budget, cost)
                if len(subs) == 1:
                    append(text, start, end, starts_para, c)
                    return
                for k, (a, b) in enumerate(subs):
                    place(body[a:b], a, b, starts_para and k == 0, "fragment")
            else:  # a one-character fragment that still does not fit: emitted as is
                append(text, start, end, starts_para, c)
            return
        append(text, start, end, starts_para, c)

    def append(text, start, end, starts_para, c):
        nonlocal cur_cost
        if cur_units and cur_cost + c > budget:
            flush()
        cur_units.append((text, start, end, starts_para))
        cur_cost += c

    for a, b in paragraphs(body):
        place(collapse(body[a:b]), a, b, True, "paragraph")
    flush()
    return passages


# ---------------------------------------------------------------- set A

def word_cost(s: str) -> int:
    return len(words(s, 0, len(s)))


SET_A = [
    ("single paragraph under budget", "The quick brown fox jumps over the lazy dog."),
    ("two paragraphs packed", "One two three.\n\nFour five six."),
    ("paragraph over budget splits to sentences", "First sentence here. Second sentence there! Third one? Fourth."),
    ("sentence over budget splits to words", "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda"),
    ("mr smith is two sentences", "Mr. Smith went to Washington. He came back."),
    ("blank lines with tab and cr runs", "Para one.\n \t\r\nPara two.\n\n\n\nPara three."),
    ("leading and trailing whitespace", "   \n  Leading spaces.  \n\n  Trailing too.   \n\n"),
    ("internal whitespace runs collapsed", "many    spaces\tand\ttabs nbsp　ideographic here"),
    ("empty body", ""),
    ("cjk and combining characters", "東京は日本の首都です。 人口が多い。\n\ncafé résumé naïve"),
    ("no terminator at the end", "This body just ends without a terminator and it is long enough to need splitting"),
    ("terminator not followed by whitespace", "Version 2.0 is out.Now what? e.g. this. Done"),
]
BUDGETS_A = [1, 3, 8, 40]


# ---------------------------------------------------------------- set B

SET_B_TITLES = [
    "Romania",              # > 5,000 words (id 2124)
    "Internet Explorer",    # > 6,000 words (id 3427)
    "Milena Djukic",        # a 392-character URL token → fragments (id 700210)
    "April",                # id 1, the first article
    "Alan Turing",
    "Tokyo",
    "Photosynthesis",
    "Chess",
]


def load_articles(titles: list[str]) -> list[dict]:
    wanted = set(titles)
    found: dict[str, dict] = {}
    with open(JSONL, encoding="utf-8") as fh:
        for line in fh:
            a = json.loads(line)
            if a["title"] in wanted:
                found[a["title"]] = a
                if len(found) == len(wanted):
                    break
    missing = wanted - set(found)
    if missing:
        raise SystemExit(f"gen_008_fixtures: articles not in the snapshot: {sorted(missing)}")
    return [found[t] for t in titles]


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)

    set_a = []
    for name, body in SET_A:
        for budget in BUDGETS_A:
            set_a.append({"name": name, "budget": budget, "body": body,
                          "passages": chunk(body, budget, word_cost)})
    (OUT / "chunk_a.json").write_text(json.dumps(set_a, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")

    tok = Tokenizer.from_file(str(TOKENIZER))
    tok.no_truncation()
    tok.no_padding()

    def token_count(s: str) -> int:
        return len(tok.encode(s, add_special_tokens=True).ids)

    unit_costs: dict[str, int] = {}

    def piece_cost(s: str) -> int:
        c = unit_costs.get(s)
        if c is None:
            c = token_count(s) - 2
            unit_costs[s] = c
        return c

    articles = load_articles(SET_B_TITLES)
    # Step 4 (fragments) is unreachable under the real cost at a real budget (see the module
    # doc), so one synthetic article with an explicit budget of 6 pieces is appended, labelled
    # as such: a 45-character medical term (many pieces) between two ordinary sentences.
    articles.append({
        "id": "synthetic-fragments",
        "title": "Fragments (synthetic)",
        "budget": 6,
        "text": "A short lead sentence before the word.\n\nPneumonoultramicroscopicsilicovolcanoconiosis is long. A tail sentence after it.",
    })

    set_b = []
    over = 0
    for a in articles:
        title, body = a["title"], a["text"]
        budget = a.get("budget", 256 - token_count(title))
        passages = chunk(body, budget, piece_cost)
        for p in passages:
            seen = token_count(title + "\n\n" + p["text"])
            if seen > 256:
                over += 1
                print(f"  OVER: {title} passage at {p['byte_range']} → {seen} positions", file=sys.stderr)
        set_b.append({"id": a["id"], "title": title, "budget": budget, "body": body, "passages": passages})
    if over:
        raise SystemExit(f"gen_008_fixtures: {over} passages exceed 256 positions — the contract or the additivity assumption is wrong")
    (OUT / "chunk_b.json").write_text(
        json.dumps({"cost": "content word-pieces: token_count(text) - 2, tokenizers 0.23.2, all-MiniLM-L6-v2 tokenizer.json, truncation off",
                    "articles": set_b, "unit_costs": unit_costs}, ensure_ascii=False, indent=1) + "\n",
        encoding="utf-8")

    def sha(p: Path) -> str:
        return hashlib.sha256(p.read_bytes()).hexdigest()

    (OUT / "manifest.json").write_text(json.dumps({
        "schema_version": 1,
        "generator_sha256": sha(Path(__file__)),
        "files": {"chunk_a.json": sha(OUT / "chunk_a.json"), "chunk_b.json": sha(OUT / "chunk_b.json")},
        "set_b_ids": [a["id"] for a in set_b],
    }, indent=1) + "\n", encoding="utf-8")
    print(f"gen_008_fixtures: set A {len(set_a)} cases, set B {len(set_b)} articles "
          f"({sum(len(a['passages']) for a in set_b)} passages, {len(unit_costs)} priced units), all within 256")
    return 0


if __name__ == "__main__":
    sys.exit(main())
