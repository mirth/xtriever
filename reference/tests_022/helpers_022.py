"""Shared constants and stubs for the Feature 022 checks (a plain module — no `conftest` name)."""

import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "reference"))

# The study's constants, as the spec fixes them (specs/022-chunking-study/spec.md, US4/FR-004).
WINDOW = 256
MIN_POSITIONS = 16
MEAN_GAIN = 0.005
MAX_DROP = 0.005
RECALL_DROP = 0.005
K_STUDY = 300
K_ANCHOR = 100


def words_cost(unit: str) -> int:
    """A stub unit cost: one position per word (the contract chunker's set-A pricing)."""
    return len(unit.split())


def words_positions(text: str) -> int:
    """A stub `token_count`: words + the two special positions."""
    return len(text.split()) + 2


class StubSplitter:
    """Stands in for chonky: yields the pieces given for a text (they must partition it)."""

    def __init__(self, pieces_by_text: dict[str, list[str]]):
        self.pieces_by_text = pieces_by_text

    def __call__(self, text: str):
        yield from self.pieces_by_text[text]
