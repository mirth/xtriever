"""The Feature 008 contract chunker — the default `--chunker contract` (Feature 023).

This is the contract's reference implementation from `reference/gen_008_fixtures.py`
(`specs/008-wiki-corpus/contracts/chunker.md`) — the independent Python the Rust chunker's
goldens were minted from — carried here unchanged since Feature 019 so the recipe is readable
inside the demo and the demo's passages are the shipped index's. The tests replay
`reference/fixtures/008/chunk_a.json` (48 cases) and `chunk_b.json` (nine real articles) and
require byte-identical passages, so this copy cannot drift from the contract without the
suite saying so; `measure --against` a Rust-built slice checks the whole build.

Pricing: content word-pieces of the embedder's own tokenizer (`token_count(unit) − 2`),
budget `256 − token_count(title)` so `[CLS] title body [SEP]` fits the 256-position window —
which is why a contract passage can never exceed it.
"""

from __future__ import annotations

import xtriever

from .chunking import WINDOW, BuildError, Window, passage_text
from .record import CONTRACT_CHUNKER

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
    """Whitespace runs collapsed to one space."""
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


# ---------------------------------------------------------------- pricing and documents


class ContractChunker:
    """The contract over the embedder's tokenizer: unit costs `token_count(unit) − 2`,
    memoised per build (the reference generator does the same)."""

    block = CONTRACT_CHUNKER
    label = "008 contract (256 - token_count(title))"

    def __init__(self, window: Window):
        self._window = window
        self._costs: dict[str, int] = {}

    def cost(self, unit: str) -> int:
        c = self._costs.get(unit)
        if c is None:
            c = self._window.token_count(unit) - 2
            self._costs[unit] = c
        return c

    def documents_for(self, article: dict) -> tuple[list[xtriever.Document], list[int]]:
        """The article's passages with their provenance, as the pipeline ingests them
        (`chunking.rs::documents_for` / `source_document`), and each passage's position count."""
        title, body, article_id = article["title"], article["text"], article["id"]
        title_positions = self._window.token_count(title)
        if title_positions >= WINDOW:
            raise BuildError(f"article {article_id} ({title!r}): the title alone needs {title_positions} positions, the window is {WINDOW}")
        budget = WINDOW - title_positions
        docs, positions = [], []
        for ordinal, p in enumerate(chunk(body, budget, self.cost)):
            passage = passage_text(title, p["text"])
            docs.append(
                xtriever.Document(
                    external_id=f"{article_id}#{ordinal}",
                    fields={
                        "title": xtriever.FieldValue.TEXT(title),
                        "text": xtriever.FieldValue.TEXT(passage),
                    },
                    chunk=xtriever.ChunkInfo(parent=article_id, ordinal=ordinal, byte_start=p["byte_range"][0], byte_end=p["byte_range"][1]),
                )
            )
            positions.append(self._window.token_count(passage))
        return docs, positions
