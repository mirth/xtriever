"""Which weights file a pinned model directory holds (Feature 026).

The engine loads a directory holding exactly one weights file — the float ``model.safetensors``
or the eight-bit ``*.gguf`` — and refuses one holding both; the model-backed tests skip by the
same rule, so a skip and a load never disagree. Shared by ``python/tests/conftest.py`` and,
loaded by path, ``apps/python-minimal-demo/tests``; ``apps/python-wiki-demo/wikidemo/inputs.py``
carries the demo's own copy of this rule, which its command line needs at run time.
"""

from pathlib import Path


def weights_files(model_dir: Path) -> list[Path]:
    """Every file in `model_dir` the engine would take for weights: the float
    `model.safetensors` and each `*.gguf`, in that order."""
    found = []
    if (model_dir / "model.safetensors").exists():
        found.append(model_dir / "model.safetensors")
    if model_dir.is_dir():
        found.extend(sorted(model_dir.glob("*.gguf")))
    return found


def weights(model_dir: Path) -> Path | None:
    """The one weights file `model_dir` holds, or None when it holds none or several.

    The engine loads a directory holding exactly one — the float `model.safetensors` or the
    eight-bit GGUF (Feature 026) — and refuses one holding both, so neither a preference nor
    the first of several would be what it loads. It checks in addition that the file is the
    pinned artefact, by name and by hash, which nothing here can do.
    """
    found = weights_files(model_dir)
    return found[0] if len(found) == 1 else None


def unusable_weights(model_dir: Path) -> str | None:
    """Why a directory holding weights could not be loaded, or None. Holding none is not
    reported here: that is a missing input, named with the command that produces it."""
    found = weights_files(model_dir)
    if len(found) > 1:
        names = ", ".join(p.name for p in found)
        return f"{model_dir} holds {names}; a model directory holds exactly one weights file"
    return None
