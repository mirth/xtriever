"""Dense format 3's arithmetic, restated once in Python (Feature 026, ADR-0015).

The one place the eight-bit scheme lives on the reference side: `gen_004_fixtures.py` and
`gen_005_fixtures.py` import it to mint their goldens, and `gen_026_fixtures.py` imports it to
check those goldens and to write the scripted oracle's expectations. Every fixture manifest pins
this file's hash beside its generator's, and the fixture-validity tests assert both, so the
scheme cannot change without the goldens being regenerated.

Every step is the engine's step (`crates/xtriever-dense/src/quantise.rs`, `index/search.rs`):

- the scale is `max|component| / 127` as an `f32`, floored at the smallest normal `f32`; a zero
  vector stores a scale of one;
- a code is the `f32` quotient `component / scale`, rounded **half away from zero** (Python's
  `round` goes to even and would not do), clamped to ±127;
- the stored norm is `sqrt(Σ code²) × scale`, the sum exact in integers, rounded to `f32` on
  the way to disk;
- a dot product is the exact integer sum of code products times the two scales; cosine divides
  it by the two quantised norms (the row's as stored, in `f32`); Euclidean is the float query
  against the recovered row.

Everything the engine does in `f32` is done here in `f64` and rounded to `f32`, which gives the
same result for one division, one multiplication or one square root (double rounding is
innocuous at 53 bits). Standard library only, so CI can run the checks.
"""

from __future__ import annotations

import math
import struct

F32_MIN_POSITIVE = 2.0 ** -126


def f32(x: float) -> float:
    """Round to `f32`, the width the stage stores and returns."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


def round_half_away(q: float) -> int:
    """`f32::round`: half-way cases go away from zero. Python's `round` goes to even."""
    magnitude = math.floor(abs(q) + 0.5)
    return int(magnitude if q >= 0.0 else -magnitude)


def quantise(vector: list[float]) -> tuple[list[int], float]:
    """Symmetric, one scale per vector, never code −128; every step the engine's `f32` step."""
    peak = max((abs(x) for x in vector), default=0.0)
    scale = max(f32(peak / 127.0), F32_MIN_POSITIVE) if peak > 0.0 else 1.0
    codes = []
    for x in vector:
        code = round_half_away(f32(x / scale))
        codes.append(max(-127, min(127, code)))
    return codes, scale


def recovered(vector: list[float]) -> list[float]:
    codes, scale = quantise(vector)
    return [f32(c * scale) for c in codes]


def recovered_norm(codes: list[int], scale: float) -> float:
    """The norm a row stores (as `f64`; the engine rounds it to `f32` on the way to disk)."""
    return math.sqrt(sum(c * c for c in codes)) * scale


class Prepared:
    """A vector as the stage scores it: its floats, its codes, its scale and its stored norm."""

    def __init__(self, vector) -> None:
        self.vector = [f32(float(x)) for x in vector]
        self.codes, self.scale = quantise(self.vector)
        self.norm = recovered_norm(self.codes, self.scale)      # f64, the query's form
        self.norm_f32 = f32(self.norm)                          # the row's form


def score(metric: str, q: Prepared, row: Prepared) -> float:
    """One row's score, over what the stage stores rather than what was added, as `f32`."""
    if metric == "euclidean":
        acc = 0.0
        for a, c in zip(q.vector, row.codes):
            d = a - f32(c * row.scale)
            acc += d * d
        return f32(-math.sqrt(acc))
    accumulator = sum(a * b for a, b in zip(q.codes, row.codes))   # exact, in integers
    dot = accumulator * q.scale * row.scale
    if metric == "dot":
        return f32(dot)
    if metric == "cosine":
        return f32(dot / (q.norm * row.norm_f32))
    raise ValueError(metric)


def ranked(scores: dict, allowed: set | None) -> list[tuple]:
    """`(score DESC, id ASC)`, the stage's total order."""
    items = [(i, s) for i, s in scores.items() if allowed is None or i in allowed]
    items.sort(key=lambda pair: (-pair[1], pair[0]))
    return items
