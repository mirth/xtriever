"""Feature 012 checks: the 003 scorer as the oracle, the model-card recipe, the spike's BM25."""

import sys
from pathlib import Path

from helpers_012 import REPO  # noqa: E402
sys.path.insert(0, str(REPO / "reference"))
