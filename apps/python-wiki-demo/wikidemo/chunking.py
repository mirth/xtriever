"""Article → passages with the chonky splitter (Feature 021, `specs/021-chonky-wiki-chunking`).

`chonky` (https://github.com/mirth/chonky) is a fine-tuned token-classification model that
returns a text as contiguous slices at predicted paragraph breaks; every non-empty slice is
one passage with its byte range, and a result that is not a partition of the text is a build
error. The model is pinned like the engine's (`reference/models/manifest-chonky.json`) and
loaded from disk. The splitter has no length bound: on Wikipedia about a tenth of the chunks
exceed the embedder's 256-position window (median 77, p90 248, max seen 4,745 tokens); such a
passage is added whole — the engine embeds its first 256 word-pieces, the lexical index sees
all of it — and counted. The Rust build keeps the 008 contract chunker, so a demo-built
index is not the shipped one.
"""

from pathlib import Path

import xtriever

WINDOW = 256  # the embedder's window in positions (`[CLS]` … `[SEP]`), the pinned model's `max_tokens`


class BuildError(Exception):
    """A build that must stop: the message names what and where."""


class Splitter:
    """The chonky paragraph splitter, loaded from a local model directory."""

    def __init__(self, model_dir: Path):
        from chonky import ParagraphSplitter  # imports torch — only the build pays for it

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


class Window:
    """The embedder's own tokenizer, for counting positions against `WINDOW`."""

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


def documents_for(article: dict, splitter: Splitter, window: Window) -> tuple[list[xtriever.Document], list[int]]:
    """The article's passages with their provenance, and each passage's position count."""
    title, text, article_id = article["title"], article["text"], article["id"]
    try:
        ranges = splitter.chunks(text)
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
            tokens.append(window.token_count(passage))
        byte_start = byte_end
    return docs, tokens
