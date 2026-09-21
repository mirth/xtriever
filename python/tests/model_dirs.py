"""Which weights file a pinned model directory holds (Feature 026).

The engine accepts a directory holding the float ``model.safetensors`` or one eight-bit
``*.gguf``; the model-backed tests skip by the same rule, so a skip and a load never disagree.
Shared by ``python/tests/conftest.py`` and, loaded by path, ``apps/python-minimal-demo/tests``;
``apps/python-wiki-demo/wikidemo/inputs.py`` carries the demo's own copy, which its command
line needs at run time.
"""

from pathlib import Path


def weights(model_dir: Path) -> Path | None:
    """The weights file ``model_dir`` holds — the float ``model.safetensors`` or the eight-bit
    ``*.gguf`` — or None when it holds neither."""
    float_weights = model_dir / "model.safetensors"
    if float_weights.exists():
        return float_weights
    ggufs = sorted(model_dir.glob("*.gguf")) if model_dir.is_dir() else []
    return ggufs[0] if ggufs else None
