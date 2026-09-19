"""Puts `reference/` on `sys.path` so the tests import the scripts beside them."""

import sys
from pathlib import Path

REFERENCE = Path(__file__).resolve().parent.parent
if str(REFERENCE) not in sys.path:
    sys.path.insert(0, str(REFERENCE))
