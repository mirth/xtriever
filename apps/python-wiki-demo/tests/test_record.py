"""The record helpers reproduce the Rust build's (research D9, D12): canonical JSON, the
corpus identity (the shipped index's hash, a constant), the attribution text, and a machine
name that is a hardware model — never a hostname."""

import platform

from wikidemo.record import (
    CHUNKER,
    attribution_text,
    canonical_json,
    corpus_identity,
    machine_name,
    now_rfc3339,
    peak_resident_bytes,
)

FINGERPRINT = (
    "sentence-transformers/all-MiniLM-L6-v2@1110a243fdf4706b3f48f1d95db1a4f5529b4d41;"
    "weights=sha256:53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db;"
    "dim=384;pool=mean-mask;norm=l2;max_tokens=256;dtype=f32;prefix=none;engine=candle-0.9.2"
)
SNAPSHOT = {
    "edition": "simple",
    "snapshot_date": "2023-11-01",
    "parquet_sha256": "31bded16768a47c286becd292079122f5d7d4397a17b87d4250a00ccd581e6f0",
    "jsonl_sha256": "bf69f110c5a7cf5adfbb607c4f65bd81839e27a05e6a603c1e3cbc4736db295f",
}
EXCLUSIONS = [
    {"rule": "title_suffix", "value": " (disambiguation)"},
    {"rule": "lead_contains", "value": "may refer to", "within_chars": 300},
    {"rule": "lead_contains", "value": "may mean", "within_chars": 300},
]
SHIPPED_IDENTITY = "ea0fc78c4dce30cebf066033327cc7a442385c605be8c67eadb772329eab2027"


def test_canonical_json_matches_the_rust_rule():
    assert canonical_json({"b": 1, "a": "é", "c": [True, None, 1.5]}) == '{"a":"é","b":1,"c":[true,null,1.5]}'


SHIPPED_CHUNKER = {
    "version": 1,
    "budget": "256 - token_count(title)",
    "cost": "MiniLmEmbedder::token_count(unit) - 2",
}


def test_identity_of_the_shipped_corpus():
    # The identity function is the Rust build's; with the shipped artefact's chunker block
    # it still reproduces the shipped identity.
    assert corpus_identity(SNAPSHOT, EXCLUSIONS, SHIPPED_CHUNKER, FINGERPRINT) == SHIPPED_IDENTITY
    partial = corpus_identity(SNAPSHOT, EXCLUSIONS, SHIPPED_CHUNKER, FINGERPRINT, partial=2000)
    assert partial != SHIPPED_IDENTITY and len(partial) == 64


def test_chunker_block_is_chonky():
    assert CHUNKER == {
        "name": "chonky",
        "model": "mirth/chonky_distilbert_base_uncased_1",
        "revision": "01d8aae08726368a1b1645de2a7086610f2e86a5",
    }
    # Different passages, different identity — a chonky-built index is never the shipped one.
    assert corpus_identity(SNAPSHOT, EXCLUSIONS, CHUNKER, FINGERPRINT) != SHIPPED_IDENTITY


def test_attribution_text_shape():
    manifest = {
        "snapshot_date": "2023-11-01",
        "source": "https://huggingface.co/datasets/wikimedia/wikipedia",
        "licence": {"name": "CC BY-SA 4.0", "url": "https://creativecommons.org/licenses/by-sa/4.0/"},
    }
    text = attribution_text(manifest, SHIPPED_IDENTITY, "2026-09-14T04:11:54Z")
    assert text == (
        "Text from Simple English Wikipedia, snapshot 2023-11-01 (https://huggingface.co/datasets/wikimedia/wikipedia).\n"
        "Licensed under the Creative Commons Attribution-ShareAlike 4.0 licence (https://creativecommons.org/licenses/by-sa/4.0/).\n"
        "Each passage links to its source article; the title line of every passage is the article's.\n"
        f"Corpus identity {SHIPPED_IDENTITY}, built 2026-09-14T04:11:54Z.\n"
    )
    stamp = now_rfc3339()
    assert len(stamp) == 20 and stamp.endswith("Z") and stamp[10] == "T"


def test_machine_name_is_a_hardware_model_not_a_hostname():
    name = machine_name()
    assert name
    assert "/" not in name and " " not in name
    node = platform.node()
    assert not node or node not in name
    assert node.split(".")[0] not in name if node else True


def test_peak_resident_bytes_is_positive_and_in_bytes():
    peak = peak_resident_bytes()
    assert peak > 10 * 1024 * 1024  # a Python process with pytest loaded is well over 10 MB
