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

#: The two chunker blocks the identity can name (Feature 023: `build --chunker`).
#: `CONTRACT_CHUNKER` is the shipped `corpus.json`'s literal strings — the block the Rust CLI
#: writes for the 008 contract chunker — so a default build of the same articles has the
#: Rust build's identity. `CHONKY_CHUNKER` names the chonky splitter and its pinned revision
#: (`reference/models/manifest-chonky.json`, Feature 021); a chonky-built index is never the
#: shipped one.
CONTRACT_CHUNKER = {
    "version": 1,
    "budget": "256 - token_count(title)",
    "cost": "MiniLmEmbedder::token_count(unit) - 2",
}
CHONKY_CHUNKER = {
    "name": "chonky",
    "model": "mirth/chonky_distilbert_base_uncased_1",
    "revision": "01d8aae08726368a1b1645de2a7086610f2e86a5",
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
    """The process's peak resident size: `ru_maxrss` is bytes on macOS, KiB on Linux. It counts
    clean file-backed pages, so a memory-mapped index inflates it as queries page it in."""
    peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return peak if sys.platform == "darwin" else peak * 1024


def peak_footprint_bytes() -> int | None:
    """The process's lifetime peak `phys_footprint` on macOS, or None elsewhere.

    `phys_footprint` is the counter iOS enforces when it terminates an app, and the one the
    600 MB ceiling is defined on (ADR-0010; the device tests read its kernel ledger): it counts
    the memory the process owns and excludes clean file-backed pages, which the system reclaims
    freely. `ri_lifetime_max_phys_footprint` from `proc_pid_rusage(RUSAGE_INFO_V4)`, the same
    kernel-kept peak the Swift package's `Measure.swift` reads (Feature 026).
    """
    if sys.platform != "darwin":
        return None
    import ctypes

    class RusageV4(ctypes.Structure):
        # <sys/resource.h> rusage_info_v4: a 16-byte uuid, then u64 fields;
        # ri_lifetime_max_phys_footprint is the 29th of them.
        _fields_ = [("uuid", ctypes.c_uint8 * 16), ("fields", ctypes.c_uint64 * 40)]

    try:
        libproc = ctypes.CDLL("/usr/lib/libproc.dylib")
        info = RusageV4()
        if libproc.proc_pid_rusage(os.getpid(), 4, ctypes.byref(info)) != 0:
            return None
        return int(info.fields[28])
    except OSError:
        return None


def megabytes(n: int) -> str:
    """Bytes as decimal megabytes to one place — the unit the 600 MB ceiling is written in
    (600,000,000 bytes); a mebibyte figure beside it misled once (Feature 026)."""
    return f"{n / 1_000_000:,.1f} MB"


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
