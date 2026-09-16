"""Constants shared by the Feature 014 tests — a plain module, so the suite can be collected
together with the other reference suites (`conftest` imported by name shadows across suites)."""

from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
K = 100
