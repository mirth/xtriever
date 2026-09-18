"""Article → passages: what both chunkers share, and the choice between them (Feature 023,
`specs/023-optional-chonky-chunker`).

`wikidemo build --chunker {contract,chonky}` picks how an article is cut:

- `contract` (the default, `contract.py`): the Feature 008 contract chunker — the recipe the
  Rust CLI cuts the shipped index with, so a demo-built slice is the Rust build's, passage
  for passage (`measure --against` checks it);
- `chonky` (`chonky_chunker.py`, Feature 021): the chonky neural paragraph splitter — an
  optional extra (`pip install -e '.[chonky]'`) and a pinned model; its passages are not the
  shipped index's, and about a tenth of them run past the embedder's window.

Either chunker yields one `xtriever.Document` per passage with its byte range in the article,
plus each stored passage's position count under the embedder's own tokenizer: a passage over
`WINDOW` positions is added whole — the engine embeds its first 256 word-pieces, the lexical
index sees all of it — and counted (`passages_over_window`; always 0 for the contract).
"""

from __future__ import annotations

from pathlib import Path

WINDOW = 256  # the embedder's window in positions (`[CLS]` … `[SEP]`), the pinned model's `max_tokens`

CHUNKERS = ("contract", "chonky")
DEFAULT_CHUNKER = "contract"


class BuildError(Exception):
    """A build that must stop: the message names what and where."""


class Window:
    """The embedder's own tokenizer, for counting positions against `WINDOW` (and, for the
    contract chunker, pricing its units)."""

    def __init__(self, embedder_dir: Path):
        from tokenizers import Tokenizer  # the pinned 0.23.2 — the engine's version

        self._tok = Tokenizer.from_file(str(Path(embedder_dir) / "tokenizer.json"))
        self._tok.no_truncation()
        self._tok.no_padding()

    def token_count(self, s: str) -> int:
        return len(self._tok.encode(s, add_special_tokens=True).ids)


def passage_text(title: str, body: str) -> str:
    """The `text` field / stored passage: the title line, a blank line, the passage body."""
    return f"{title}\n\n{body}"


def make_chunker(name: str, paths, window: Window):
    """The chunker `--chunker NAME` asks for. Each has `documents_for(article)`, `block` (the
    sidecar's chunker block) and `label`. The imports sit inside the branches so a contract
    build never touches the chonky module or its extra."""
    if name == "contract":
        from .contract import ContractChunker

        return ContractChunker(window)
    if name == "chonky":
        from .chonky_chunker import ChonkyChunker

        return ChonkyChunker(paths.chonky, window)
    raise BuildError(f"unknown chunker {name!r}; choose one of {', '.join(CHUNKERS)}")
