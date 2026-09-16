"""Record helpers reproducing the Rust build's (`crates/xtriever-cli/src/wiki/record.rs`):
canonical JSON, the corpus identity, the attribution text; plus the machine facts the run
records carry (research D9, D12, D17). No hostname, user name or device identifier ever
goes into a record — the machine is named by its hardware model.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import resource
import subprocess
import sys
import time
from pathlib import Path

#: The chunker the identity names — the contract, not the implementation (the shipped
#: corpus.json's literal strings).
CHUNKER = {
    "version": 1,
    "budget": "256 - token_count(title)",
    "cost": "MiniLmEmbedder::token_count(unit) - 2",
}


def canonical_json(value) -> str:
    """Keys sorted, no whitespace, non-ASCII kept — byte-identical to `record.rs::canonical_json`."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def corpus_identity(snapshot: dict, exclusions: list, chunker: dict, embedder_fingerprint: str, partial: int | None = None) -> str:
    basis = {
        "snapshot": snapshot,
        "exclusions": exclusions,
        "chunker": chunker,
        "embedder_fingerprint": embedder_fingerprint,
    }
    if partial is not None:
        basis["partial"] = partial
    return hashlib.sha256(canonical_json(basis).encode("utf-8")).hexdigest()


def attribution_text(manifest: dict, identity: str, recorded_at: str) -> str:
    """The four lines of `record.rs::attribution`."""
    return (
        f"Text from Simple English Wikipedia, snapshot {manifest['snapshot_date']} ({manifest['source']}).\n"
        f"Licensed under the Creative Commons Attribution-ShareAlike 4.0 licence ({manifest['licence']['url']}).\n"
        "Each passage links to its source article; the title line of every passage is the article's.\n"
        f"Corpus identity {identity}, built {recorded_at}.\n"
    )


def now_rfc3339() -> str:
    """RFC 3339 UTC, seconds."""
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def stamp() -> str:
    """The compact form for record file names: `YYYYMMDDTHHMMSSZ`."""
    return time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())


def machine_name() -> str:
    """The hardware model (`MacBookPro18,3`) — never `platform.node()`."""
    if sys.platform == "darwin":
        try:
            model = subprocess.run(["sysctl", "-n", "hw.model"], capture_output=True, text=True, timeout=5).stdout.strip()
            if model:
                return model
        except (OSError, subprocess.SubprocessError):
            pass
    return f"{platform.system()}-{platform.machine()}"


def os_name() -> str:
    if sys.platform == "darwin":
        return f"macOS {platform.mac_ver()[0]}"
    return f"{platform.system()} {platform.release()}"


def peak_resident_bytes() -> int:
    """The process's peak resident size: `ru_maxrss` is bytes on macOS, KiB on Linux."""
    peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return peak if sys.platform == "darwin" else peak * 1024


def threads() -> tuple[int, str]:
    """(effective threads, how that was decided) — candle's rayon pool follows RAYON_NUM_THREADS."""
    env = os.environ.get("RAYON_NUM_THREADS")
    if env and env.isdigit() and int(env) > 0:
        return int(env), "RAYON_NUM_THREADS"
    return os.cpu_count() or 1, "os.cpu_count (candle's default when RAYON_NUM_THREADS is unset)"


def dir_bytes(path: Path) -> int:
    return sum(p.stat().st_size for p in path.rglob("*") if p.is_file())


def write_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
