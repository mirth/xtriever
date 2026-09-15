"""Feature 012 checks: the 003 scorer as the oracle, the model-card recipe, the spike's BM25."""

import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "reference"))
