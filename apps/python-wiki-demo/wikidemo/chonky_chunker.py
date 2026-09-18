"""The chonky chunker — `--chunker chonky` (Feature 021, optional since Feature 023).

`chonky` (https://github.com/mirth/chonky) is a fine-tuned token-classification model that
returns a text as contiguous slices at predicted paragraph breaks; every non-empty slice is
one passage with its byte range, and a result that is not a partition of the text is a build
error. The model is pinned like the engine's (`reference/models/manifest-chonky.json`) and
loaded from disk; the library and what it runs on (`transformers`, `torch`) are the demo's
`chonky` extra, checked before the snapshot is opened and never imported otherwise. The
splitter has no length bound: on the 2,000-article slice 9.2 % of the passages exceed the
embedder's 256-position window (median 82, p90 239, max 10,367 —
`specs/021-chonky-wiki-chunking/runs/`); such a passage is added whole and counted. The Rust
build keeps the 008 contract chunker, so a chonky-built index is not the shipped one.

(This module is `chonky_chunker`, not `chonky`, so `from chonky import …` below can never
resolve to itself.)
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

import xtriever

from .chunking import BuildError, Window, passage_text
from .record import CHONKY_CHUNKER

INSTALL = "uv pip install --python apps/python-wiki-demo/.venv/bin/python -e 'apps/python-wiki-demo[chonky]'"


def ensure_extra() -> None:
    """Refuse a chonky build whose extra is not installed — decided without importing it."""
    if importlib.util.find_spec("chonky") is None:
        raise BuildError(f"--chunker chonky needs the demo's chonky extra; install it with: {INSTALL}")


class Splitter:
    """The chonky paragraph splitter, loaded from a local model directory."""

    def __init__(self, model_dir: Path):
        from chonky import ParagraphSplitter  # imports torch — only a chonky build pays for it

        self._splitter = ParagraphSplitter(model_id=str(model_dir), device="cpu")

    def chunks(self, text: str) -> list[tuple[int, int]]:
        """The splitter's slices as character ranges; they must partition `text`."""
        pieces = list(self._splitter(text))
        if "".join(pieces) != text:
            raise BuildError("the splitter did not return a partition of the text")
        ranges, start = [], 0
        for piece in pieces:
            ranges.append((start, start + len(piece)))
            start += len(piece)
        return ranges


class ChonkyChunker:
    """One passage per non-empty chonky slice, with its byte range and position count."""

    block = CHONKY_CHUNKER
    label = f"chonky ({CHONKY_CHUNKER['model']}, revision {CHONKY_CHUNKER['revision'][:7]}…)"

    def __init__(self, model_dir: Path, window: Window):
        ensure_extra()
        self._splitter = Splitter(model_dir)
        self._window = window

    def documents_for(self, article: dict) -> tuple[list[xtriever.Document], list[int]]:
        """The article's passages with their provenance, and each passage's position count."""
        title, text, article_id = article["title"], article["text"], article["id"]
        try:
            ranges = self._splitter.chunks(text)
        except BuildError as e:
            raise BuildError(f"article {article_id} ({title!r}): {e}") from None
        docs, tokens, byte_start = [], [], 0
        for start, end in ranges:
            chunk = text[start:end]
            byte_end = byte_start + len(chunk.encode("utf-8"))
            if chunk.strip():
                passage = passage_text(title, chunk.strip())
                provenance = xtriever.ChunkInfo(parent=article_id, ordinal=len(docs), byte_start=byte_start, byte_end=byte_end)
                fields = {"title": xtriever.FieldValue.TEXT(title), "text": xtriever.FieldValue.TEXT(passage)}
                docs.append(xtriever.Document(external_id=f"{article_id}#{len(docs)}", fields=fields, chunk=provenance))
                tokens.append(self._window.token_count(passage))
            byte_start = byte_end
        return docs, tokens
