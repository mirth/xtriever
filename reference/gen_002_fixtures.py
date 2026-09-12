#!/usr/bin/env python3
"""Generate the golden fixtures for Feature 002 (the lexical stage).

Principle II: behaviour with a reference implementation is verified against golden fixtures
produced by scripts in ``reference/``. This script produces every 002 fixture and, with
``--verify-ranking``, independently re-derives the rankings the Rust minter wrote into
``queries.json`` (research D17 — the same two-oracle split Feature 001 used, report.md D-001).

Emits (into ``--out``):

* ``schema.json``     the 11-field fixture schema, in ``xtriever_core::Schema``'s serde form
* ``corpus.json``     1,000 documents with planted structure (data-model ``FixtureCorpus``)
* ``queries.json``    one entry per query shape; ``expected`` is ``null`` until the Rust minter runs
* ``filters.json``    exact expected id sets, computed here
* ``stats.json``      exact term and corpus statistics, computed here
* ``mutations.json``  replace / delete / history-pair recipes and their expectations
* ``manifest.json``   sha256 per file; ``tests/fixtures_valid.rs`` asserts every one

Run via the pinned virtualenv:

    scripts/setup-reference-venv.sh 002
    reference/.venv-002/bin/python reference/gen_002_fixtures.py --seed 2
    reference/.venv-002/bin/python reference/gen_002_fixtures.py --verify-ranking reference/fixtures/002
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))  # for `xtref` (reference/xtref/)

_REQUIRED_PY = (3, 12)
if sys.version_info[:2] != _REQUIRED_PY or sys.prefix == sys.base_prefix:
    sys.exit(
        f"gen_002_fixtures.py requires the pinned venv on Python "
        f"{_REQUIRED_PY[0]}.{_REQUIRED_PY[1]} (running "
        f"{sys.version_info[0]}.{sys.version_info[1]}, "
        f"{'venv' if sys.prefix != sys.base_prefix else 'system interpreter'}).\n"
        "  scripts/setup-reference-venv.sh 002\n"
        "  reference/.venv-002/bin/python reference/gen_002_fixtures.py --seed 2"
    )

for _var in ("OMP_NUM_THREADS", "MKL_NUM_THREADS", "OPENBLAS_NUM_THREADS", "NUMEXPR_NUM_THREADS"):
    os.environ[_var] = "1"

from xtref.bm25 import (  # noqa: E402
    ANALYZERS,
    bm25_term_score,
    bm25_weight,
    fieldnorm_to_id,
    tf_component,
)

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUT = REPO_ROOT / "reference" / "fixtures" / "002"

N_DOCS = 1000
SCORE_REL_TOL = 1e-5  # spec.md Assumptions: scores vs the independent reference
TS_BASE = 1_700_000_000_000  # ms; 2023-11-14T22:13:20Z

# --------------------------------------------------------------------------------------------
# Schema (data-model.md `FixtureSchema`), in xtriever_core::Schema's serde form.
# --------------------------------------------------------------------------------------------
SCHEMA_FIELDS = [
    ("title", {"Text": "standard"}, True, True, 2.0),
    ("body", {"Text": "standard"}, True, False, 1.0),
    ("summary", {"Text": "standard_en"}, True, False, 1.0),
    ("source", "Keyword", True, True, 1.0),
    ("tags", "Keyword", True, False, 1.0),
    ("views", "U64", True, False, 1.0),
    ("rank", "I64", True, False, 1.0),
    ("quality", "F64", True, False, 1.0),
    ("published", "Bool", True, False, 1.0),
    ("ts", "DateMillis", True, False, 1.0),
    ("note", {"Text": "standard"}, False, True, 1.0),
]
TEXT_FIELDS = {  # indexed text fields -> analyzer id
    "title": "standard",
    "body": "standard",
    "summary": "standard_en",
}
KEYWORD_FIELDS = ("source", "tags")
FIELD_BOOST = {name: boost for name, _, _, _, boost in SCHEMA_FIELDS}


def schema_json() -> dict:
    return {
        "fields": [
            {"name": n, "kind": k, "indexed": i, "stored": s, "boost": b}
            for n, k, i, s, b in SCHEMA_FIELDS
        ]
    }


# --------------------------------------------------------------------------------------------
# Corpus. Vocabulary is shared between title and body so the title boost is observable
# (spec Story 1 scenario 8); summary uses inflected words so `standard_en` stemming is observable.
# --------------------------------------------------------------------------------------------
VOCAB = (
    "quantum lattice kernel photon signal vector index graph tensor matrix cache thread memory "
    "search ranking engine retrieval corpus document query segment merge commit filter score "
    "field token analyzer postings bitmap heap stack queue buffer socket packet router switch "
    "orbit plasma fusion reactor turbine piston valve gasket bearing gear axle chassis brake "
    "river mountain forest desert island glacier canyon meadow valley harbor lagoon delta "
    "violin cello trumpet drum flute piano guitar harp organ banjo bugle chime"
).split()
SUMMARY_VOCAB = (
    "running runs ran dogs dog quickly searches searched indexed indexing retrieval retrieved "
    "documents documented queries queried engines engineered fast faster fastest ranking ranked"
).split()
SOURCES = ("web", "book", "paper", "forum")
TAGS = ("alpha", "beta", "gamma", "delta", "epsilon")

# Planted structure (data-model `FixtureCorpus`); every entry is asserted after generation.
PHRASE_ONCE = (5, 17, 42, 100, 250, 600, 777)   # "quantum lattice" exactly once
PHRASE_TWICE = 300                               # "quantum lattice" twice
PHRASE_SLOP1 = (8, 64, 512)                      # "quantum blue lattice" (matches at slop >= 1)
PHRASE_REVERSED = 999                            # "lattice quantum" (matches at slop >= 2)
FUZZY_PLANTS = {                                 # word -> doc; strict Levenshtein distance to "lattice"
    "lattise": 33,   # d1 substitution
    "latice": 44,    # d1 deletion
    "lattices": 55,  # d1 insertion
    "lettuce": 66,   # d2 two substitutions
    "latitce": 77,   # transposition: d2 under strict Levenshtein (transposition_cost_one = false)
}
TS_ONE_MS_DOC = 10                               # doc 11's ts is exactly doc 10's + 1 ms
PHRASE_WORDS = {"quantum", "lattice", "blue"}


def gen_corpus(seed: int) -> list[dict]:
    rng = random.Random(seed)
    docs = []
    for i in range(N_DOCS):
        # Keep the phrase words out of random text so every "quantum lattice" is a plant.
        vocab = [w for w in VOCAB if w not in PHRASE_WORDS] if i in (
            *PHRASE_ONCE, PHRASE_TWICE, *PHRASE_SLOP1, PHRASE_REVERSED, *FUZZY_PLANTS.values()
        ) else VOCAB
        title = " ".join(rng.choice(vocab) for _ in range(rng.randint(3, 7)))
        n_body = rng.randint(12, 40)
        body_words = [rng.choice(vocab) for _ in range(n_body)]
        # A planted phrase is inserted at a random interior position.
        if i in PHRASE_ONCE:
            at = rng.randint(1, len(body_words) - 1)
            body_words[at:at] = ["quantum", "lattice"]
        elif i == PHRASE_TWICE:
            at = rng.randint(1, len(body_words) // 2)
            body_words[at:at] = ["quantum", "lattice"]
            at2 = rng.randint(at + 4, len(body_words) - 1)
            body_words[at2:at2] = ["quantum", "lattice"]
        elif i in PHRASE_SLOP1:
            at = rng.randint(1, len(body_words) - 1)
            body_words[at:at] = ["quantum", "blue", "lattice"]
        elif i == PHRASE_REVERSED:
            at = rng.randint(1, len(body_words) - 1)
            body_words[at:at] = ["lattice", "quantum"]
        for word, doc in FUZZY_PLANTS.items():
            if i == doc:
                at = rng.randint(1, len(body_words) - 1)
                body_words[at:at] = [word]
        # Capitalise the first word and add sentence punctuation so the analyzer has work to do.
        body_words[0] = body_words[0].capitalize()
        body = " ".join(body_words) + "."
        fields = {
            "title": {"Text": title.capitalize()},
            "body": {"Text": body},
            "source": {"Keyword": rng.choice(SOURCES)},
            "views": {"U64": rng.randint(0, 100_000)},
            "rank": {"I64": rng.randint(-50, 50)},
            "quality": {"F64": round(rng.random(), 4)},
            "published": {"Bool": rng.random() < 0.6},
            "ts": {"DateMillis": TS_BASE + i * 60_000 + rng.randint(0, 999)},
        }
        if i == TS_ONE_MS_DOC + 1:
            fields["ts"] = {"DateMillis": docs[TS_ONE_MS_DOC]["fields"]["ts"]["DateMillis"] + 1}
        if i % 2 == 0:
            fields["summary"] = {"Text": " ".join(rng.choice(SUMMARY_VOCAB) for _ in range(rng.randint(6, 14)))}
        if i % 2 == 1:
            fields["tags"] = {"Keyword": rng.choice(TAGS)}
        if i % 3 == 0:
            fields["note"] = {"Text": f"note for document {i}: stored, never indexed"}
        docs.append({"id": i, "fields": fields, "chunk": None})
    return docs


def gen_extra_docs(seed: int, start: int, count: int) -> list[dict]:
    """Documents 1000..1099 for the history-pair recipe: added then deleted (spec FR-015/SC-011)."""
    rng = random.Random(seed ^ 0xBEEF)
    out = []
    for i in range(start, start + count):
        fields = {
            "title": {"Text": " ".join(rng.choice(VOCAB) for _ in range(4)).capitalize()},
            "body": {"Text": ("quantum lattice " + " ".join(rng.choice(VOCAB) for _ in range(20))).capitalize() + "."},
            "source": {"Keyword": rng.choice(SOURCES)},
            "views": {"U64": rng.randint(0, 100_000)},
            "rank": {"I64": rng.randint(-50, 50)},
            "quality": {"F64": round(rng.random(), 4)},
            "published": {"Bool": True},
            "ts": {"DateMillis": TS_BASE + i * 60_000},
            "summary": {"Text": "running dogs " + " ".join(rng.choice(SUMMARY_VOCAB) for _ in range(6))},
            "tags": {"Keyword": "alpha"},
        }
        out.append({"id": i, "fields": fields, "chunk": None})
    return out


# --------------------------------------------------------------------------------------------
# Corpus model: what the index will hold, in a form the scorers can consume.
# --------------------------------------------------------------------------------------------
class Model:
    """Per-field token lists and statistics over a list of documents (live docs only)."""

    def __init__(self, docs: list[dict], max_doc: int | None = None):
        self.docs = {d["id"]: d for d in docs}
        self.ids = sorted(self.docs)
        # tantivy's Bm25StatisticsProvider::total_num_docs sums max_doc, not num_docs (research D8).
        self.max_doc = max_doc if max_doc is not None else len(docs)
        self.tokens: dict[str, dict[int, list[str]]] = {}
        for field, analyzer in TEXT_FIELDS.items():
            an = ANALYZERS[analyzer]
            self.tokens[field] = {
                i: an(d["fields"][field]["Text"]) for i, d in self.docs.items() if field in d["fields"]
            }
        for field in KEYWORD_FIELDS:
            self.tokens[field] = {
                i: [d["fields"][field]["Keyword"]] for i, d in self.docs.items() if field in d["fields"]
            }
        self.avg_fieldnorm = {
            f: sum(len(t) for t in toks.values()) / self.max_doc for f, toks in self.tokens.items()
        }
        self.fieldnorm_id = {
            f: {i: fieldnorm_to_id(len(t)) for i, t in toks.items()} for f, toks in self.tokens.items()
        }

    def analyze(self, field: str, text: str) -> list[str]:
        if field in TEXT_FIELDS:
            return ANALYZERS[TEXT_FIELDS[field]](text)
        return [text]

    def doc_freq(self, field: str, term: str) -> int:
        return sum(1 for t in self.tokens[field].values() if term in t)

    def total_term_freq(self, field: str, term: str) -> int:
        return sum(t.count(term) for t in self.tokens[field].values())

    # -- scorers: each returns {doc_id: score} over matching docs ------------------------------

    def score_term(self, field: str, term: str) -> dict[int, float]:
        df = self.doc_freq(field, term)
        if df == 0:
            return {}
        w = bm25_weight(df, self.max_doc)
        out = {}
        for i, toks in self.tokens[field].items():
            tf = toks.count(term)
            if tf:
                out[i] = bm25_term_score(tf, w, tf_component(self.fieldnorm_id[field][i], self.avg_fieldnorm[field]))
        return out

    def score_match(self, field: str | None, text: str) -> dict[int, float]:
        fields = [field] if field else list(TEXT_FIELDS)
        out: dict[int, float] = {}
        for f in fields:
            per_field: dict[int, float] = {}
            for term in self.analyze(f, text):  # duplicates count twice, as two Should clauses would
                for i, s in self.score_term(f, term).items():
                    per_field[i] = per_field.get(i, 0.0) + s
            for i, s in per_field.items():
                out[i] = out.get(i, 0.0) + s * FIELD_BOOST[f]
        return out

    def score_phrase(self, field: str, text: str, slop: int) -> dict[int, float]:
        terms = self.analyze(field, text)
        if not terms:
            return {}
        if len(terms) == 1:
            return {i: s * FIELD_BOOST[field] for i, s in self.score_term(field, terms[0]).items()}
        assert len(terms) == 2, "the transcription covers two-term phrases only (research D17)"
        for t in terms:
            if self.doc_freq(field, t) == 0:
                return {}
        # tantivy scores a phrase as BM25 with tf = phrase count, using the sum of the terms' idfs
        # (bm25.rs:120-127) -- Bm25Weight::for_terms with several terms.
        idf_sum_weight = sum(bm25_weight(self.doc_freq(field, t), self.max_doc) for t in terms)
        out = {}
        for i, toks in self.tokens[field].items():
            count = phrase_count(toks, terms[0], terms[1], slop)
            if count:
                out[i] = bm25_term_score(count, idf_sum_weight, tf_component(self.fieldnorm_id[field][i], self.avg_fieldnorm[field])) * FIELD_BOOST[field]
        return out

    def score_fuzzy(self, field: str, term: str, distance: int) -> dict[int, float]:
        vocab = {t for toks in self.tokens[field].values() for t in toks}
        matching = {w for w in vocab if levenshtein(term, w) <= distance}
        # AutomatonWeight scores every matching doc with a ConstScorer at `boost` (research D9).
        return {i: 1.0 * FIELD_BOOST[field] for i, toks in self.tokens[field].items() if any(t in matching for t in toks)}

    def score_query(self, q: dict) -> dict[int, float]:
        (kind, payload), = q.items()
        if kind == "Match":
            return self.score_match(payload[0], payload[1])
        if kind == "Term":
            return {i: s * FIELD_BOOST[payload[0]] for i, s in self.score_term(payload[0], payload[1]).items()}
        if kind == "Phrase":
            return self.score_phrase(payload[0], payload[1], payload[2])
        if kind == "Fuzzy":
            return self.score_fuzzy(payload[0], payload[1], payload[2])
        if kind == "Boost":
            return {i: s * payload[1] for i, s in self.score_query(payload[0]).items()}
        if kind == "Bool":
            musts = [self.score_query(c) for c in payload["must"]]
            shoulds = [self.score_query(c) for c in payload["should"]]
            nots = [self.score_query(c) for c in payload["must_not"]]
            if not musts and not shoulds:
                return {}
            if musts:
                candidates = set(musts[0])
                for m in musts[1:]:
                    candidates &= set(m)
            else:
                candidates = set().union(*(set(s) for s in shoulds))
            for n in nots:
                candidates -= set(n)
            out = {}
            for i in candidates:
                out[i] = sum(m.get(i, 0.0) for m in musts) + sum(s.get(i, 0.0) for s in shoulds)
            return out
        raise ValueError(kind)

    def resolve_filter(self, f: dict) -> set[int]:
        (kind, payload), = f.items()
        alive = set(self.ids)
        if kind == "Eq":
            field, value = payload
            return {i for i in alive if self.docs[i]["fields"].get(field) == value}
        if kind == "In":
            field, values = payload
            return {i for i in alive if self.docs[i]["fields"].get(field) in values}
        if kind == "Range":
            field, lo, hi = payload
            if lo is None and hi is None:
                return {i for i in alive if field in self.docs[i]["fields"]}
            lo_v = scalar(lo) if lo is not None else None
            hi_v = scalar(hi) if hi is not None else None
            out = set()
            for i in alive:
                v = self.docs[i]["fields"].get(field)
                if v is None:
                    continue
                x = scalar(v)
                if (lo_v is None or x >= lo_v) and (hi_v is None or x <= hi_v):
                    out.add(i)
            return out
        if kind == "Exists":
            return {i for i in alive if payload in self.docs[i]["fields"]}
        if kind == "And":
            s = alive
            for sub in payload:
                s = s & self.resolve_filter(sub)
            return s
        if kind == "Or":
            s: set[int] = set()
            for sub in payload:
                s = s | self.resolve_filter(sub)
            return s
        if kind == "Not":
            return alive - self.resolve_filter(payload)
        if kind == "Ids":
            return alive & set(payload)
        raise ValueError(kind)

    def rank(self, q: dict, flt: dict | None, k: int) -> list[dict]:
        scores = self.score_query(q)
        if flt is not None:
            allowed = self.resolve_filter(flt)
            scores = {i: s for i, s in scores.items() if i in allowed}
        ranked = sorted(scores.items(), key=lambda kv: (-kv[1], kv[0]))
        return [{"id": i, "score": s} for i, s in ranked[:k]]


def scalar(v: dict):
    (kind, x), = v.items()
    if kind == "Bool":
        return int(x)
    return x


def phrase_count(tokens: list[str], a: str, b: str, slop: int) -> int:
    """tantivy PhraseScorer for two terms: positions of term 0 are shifted by +1 (max_offset -
    offset, phrase_scorer.rs:379-383), then ``intersection_count`` (slop 0) or the greedy
    ``intersection_count_with_slop`` (phrase_scorer.rs:145-190) counts matches with
    ``abs_diff <= slop``."""
    left = [p + 1 for p, t in enumerate(tokens) if t == a]
    right = [p for p, t in enumerate(tokens) if t == b]
    if slop == 0:
        return len(set(left) & set(right))
    li = ri = count = 0
    while li < len(left) and ri < len(right):
        lv, rv = left[li], right[ri]
        if abs(lv - rv) <= slop:
            while li + 1 < len(left):
                if left[li + 1] > rv:
                    break
                li += 1
            count += 1
            li += 1
            ri += 1
        elif lv < rv:
            li += 1
        else:
            ri += 1
    return count


def levenshtein(a: str, b: str) -> int:
    """Strict Levenshtein: insert/delete/substitute cost 1, a transposition costs 2."""
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]


# --------------------------------------------------------------------------------------------
# Goldens.
# --------------------------------------------------------------------------------------------
def q_match(field, text):
    return {"Match": [field, text]}


def q_term(field, text):
    return {"Term": [field, text]}


def q_phrase(field, text, slop):
    return {"Phrase": [field, text, slop]}


def q_fuzzy(field, text, d):
    return {"Fuzzy": [field, text, d]}


def q_bool(must=(), should=(), must_not=()):
    return {"Bool": {"must": list(must), "should": list(should), "must_not": list(must_not)}}


def q_boost(q, b):
    return {"Boost": [q, b]}


def gen_queries() -> list[dict]:
    """``expected`` stays null here; the Rust example mints it and ``--verify-ranking`` checks it.
    ``tie_ok`` marks entries where a score tie at the k-boundary is planted on purpose."""
    return [
        {"name": "match_title", "query": q_match("title", "quantum kernel"), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "match_body", "query": q_match("body", "lattice signal vector"), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "match_all_boosted", "query": q_match(None, "quantum kernel"), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "match_summary_stem", "query": q_match("summary", "running dogs"), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "match_empty", "query": q_match("body", "zzzzunknownword"), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "phrase_exact", "query": q_phrase("body", "quantum lattice", 0), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "phrase_slop1", "query": q_phrase("body", "quantum lattice", 1), "filter": None, "k": 20, "oracle": "python", "expected": None},
        {"name": "phrase_slop2", "query": q_phrase("body", "quantum lattice", 2), "filter": None, "k": 20, "oracle": "python", "expected": None},
        {"name": "term_tag_tie", "query": q_term("tags", "alpha"), "filter": None, "k": 10, "oracle": "python", "tie_ok": True, "expected": None},
        {"name": "term_body", "query": q_term("body", "quantum"), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "term_all_matches", "query": q_term("body", "photon"), "filter": None, "k": 1000, "oracle": "python", "expected": None},
        {"name": "fuzzy_d1", "query": q_fuzzy("body", "lattice", 1), "filter": None, "k": 1000, "oracle": "python", "tie_ok": True, "expected": None},
        {"name": "fuzzy_d2", "query": q_fuzzy("body", "lattice", 2), "filter": None, "k": 1000, "oracle": "python", "tie_ok": True, "expected": None},
        {"name": "bool_mix", "query": q_bool(must=[q_match("body", "quantum")], should=[q_match("title", "kernel")], must_not=[q_term("source", "forum")]), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "bool_should_only", "query": q_bool(should=[q_term("body", "photon"), q_term("title", "photon")]), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "bool_mustnot_only", "query": q_bool(must_not=[q_term("source", "web")]), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "boost_query", "query": q_boost(q_match("body", "lattice"), 3.0), "filter": None, "k": 10, "oracle": "python", "expected": None},
        {"name": "match_filtered", "query": q_match("body", "quantum"), "filter": {"And": [{"Eq": ["source", {"Keyword": "web"}]}, {"Range": ["views", {"U64": 1000}, None]}]}, "k": 10, "oracle": "python", "expected": None},
    ]


def gen_filters(model: Model) -> list[dict]:
    t10 = model.docs[TS_ONE_MS_DOC]["fields"]["ts"]["DateMillis"]
    some_views = model.docs[123]["fields"]["views"]["U64"]
    specs = [
        ("eq_source_web", {"Eq": ["source", {"Keyword": "web"}]}),
        ("eq_published_true", {"Eq": ["published", {"Bool": True}]}),
        ("eq_views_exact", {"Eq": ["views", {"U64": some_views}]}),
        ("eq_tags_alpha", {"Eq": ["tags", {"Keyword": "alpha"}]}),
        ("in_source", {"In": ["source", [{"Keyword": "web"}, {"Keyword": "paper"}]]}),
        ("in_tags", {"In": ["tags", [{"Keyword": "alpha"}, {"Keyword": "gamma"}]]}),
        ("range_views_closed", {"Range": ["views", {"U64": 1000}, {"U64": 5000}]}),
        ("range_rank_open_hi", {"Range": ["rank", {"I64": 10}, None]}),
        ("range_quality_open_lo", {"Range": ["quality", None, {"F64": 0.25}]}),
        ("range_ts_one_ms", {"Range": ["ts", {"DateMillis": t10}, {"DateMillis": t10}]}),
        ("range_ts_two_ms", {"Range": ["ts", {"DateMillis": t10}, {"DateMillis": t10 + 1}]}),
        ("range_none_none", {"Range": ["views", None, None]}),
        ("exists_tags", {"Exists": "tags"}),
        ("exists_summary", {"Exists": "summary"}),
        ("exists_views", {"Exists": "views"}),
        ("and_web_published", {"And": [{"Eq": ["source", {"Keyword": "web"}]}, {"Eq": ["published", {"Bool": True}]}]}),
        ("or_forum_or_negative_rank", {"Or": [{"Eq": ["source", {"Keyword": "forum"}]}, {"Range": ["rank", None, {"I64": -1}]}]}),
        ("not_exists_tags", {"Not": {"Exists": "tags"}}),
        ("ids_explicit", {"Ids": [3, 1, 4, 1, 5, 9, 999, 1000, 2000]}),
        ("nested", {"And": [{"Or": [{"Eq": ["source", {"Keyword": "book"}]}, {"In": ["tags", [{"Keyword": "beta"}]]}]}, {"Not": {"Range": ["views", None, {"U64": 50_000}]}}]}),
        ("and_empty", {"And": []}),
        ("or_empty", {"Or": []}),
    ]
    out = []
    for name, f in specs:
        ids = sorted(model.resolve_filter(f))
        out.append({"name": name, "filter": f, "expected_ids": ids})
    by = {e["name"]: e["expected_ids"] for e in out}
    assert by["range_ts_one_ms"] == [TS_ONE_MS_DOC], by["range_ts_one_ms"]
    assert by["range_ts_two_ms"] == [TS_ONE_MS_DOC, TS_ONE_MS_DOC + 1], by["range_ts_two_ms"]
    assert by["range_none_none"] == list(range(N_DOCS))
    assert by["exists_views"] == list(range(N_DOCS))
    assert by["and_empty"] == list(range(N_DOCS)) and by["or_empty"] == []
    assert by["ids_explicit"] == [1, 3, 4, 5, 9, 999]
    assert 0 < len(by["exists_summary"]) < N_DOCS and 0 < len(by["exists_tags"]) < N_DOCS
    return out


def gen_stats(model: Model) -> dict:
    terms = [
        ("body", "quantum"), ("body", "lattice"), ("body", "photon"), ("title", "kernel"),
        ("title", "quantum"), ("summary", "run"), ("summary", "dog"), ("summary", "running"),
        ("tags", "alpha"), ("source", "web"), ("body", "zzzzunknownword"), ("body", "lettuce"),
    ]
    out_terms = []
    for field, term in terms:
        df = model.doc_freq(field, term)
        out_terms.append({
            "field": field, "term": term,
            "doc_freq": df if df else None,
            "total_term_freq": model.total_term_freq(field, term) if df else None,
        })
    return {
        "num_docs": N_DOCS,
        "avg_field_len": {
            f: sum(len(t) for t in model.tokens[f].values()) / len(model.tokens[f]) for f in TEXT_FIELDS
        },
        "terms": out_terms,
    }


def gen_mutations(model: Model, seed: int) -> dict:
    replaced = 5
    assert replaced in PHRASE_ONCE
    new_doc = {
        "id": replaced,
        "fields": {
            "title": {"Text": "Replaced document five"},
            "body": {"Text": "Nothing about crystals here, only replacement text."},
            "source": {"Keyword": "web"},
            "views": {"U64": 1},
            "rank": {"I64": 0},
            "quality": {"F64": 0.5},
            "published": {"Bool": False},
            "ts": {"DateMillis": TS_BASE},
        },
        "chunk": None,
    }
    deleted = [17, 42, 300]
    remaining = [d for d in model.docs.values() if d["id"] not in deleted]
    live = Model(remaining)
    extra = gen_extra_docs(seed, N_DOCS, 100)
    return {
        "replace": {
            "id": replaced,
            "new_document": new_doc,
            "absent_from": "phrase_exact",
            "present_in": {"query": q_match("title", "replaced"), "expected_ids": [replaced]},
        },
        "delete": {
            "ids": deleted,
            "expected_num_docs": N_DOCS - len(deleted),
            "expected_term_stats": [
                {"field": "body", "term": "quantum", "doc_freq": live.doc_freq("body", "quantum"), "total_term_freq": live.total_term_freq("body", "quantum")},
                {"field": "body", "term": "lattice", "doc_freq": live.doc_freq("body", "lattice"), "total_term_freq": live.total_term_freq("body", "lattice")},
            ],
            "expected_avg_field_len": {
                f: sum(len(t) for t in live.tokens[f].values()) / len(live.tokens[f]) for f in TEXT_FIELDS
            },
            "phrase_exact_after_delete_ids": "golden ids minus deleted, same order",
        },
        "history_pair": {
            "extra_documents": extra,
            "delete_ids": [d["id"] for d in extra],
            "divergence_query": "phrase_exact",
            "note": "live-only stats after add+delete must equal stats.json exactly (SC-011); "
                    "scores before merge are expected to diverge (research D8), recorded in report.md",
        },
    }


# --------------------------------------------------------------------------------------------
# Planted-structure assertions (the generator refuses to emit otherwise).
# --------------------------------------------------------------------------------------------
def assert_plants(model: Model) -> None:
    body = model.tokens["body"]
    # Random text may also produce the phrase; that is fine (the oracle scores real content).
    # What must hold: every plant is present, the planted counts are exact, and the slop sets nest
    # strictly so each slop golden has documents the previous one lacks.
    exact = {i for i, t in body.items() if phrase_count(t, "quantum", "lattice", 0)}
    assert set(PHRASE_ONCE) | {PHRASE_TWICE} <= exact, exact
    for i in PHRASE_ONCE:
        assert phrase_count(body[i], "quantum", "lattice", 0) == 1
    assert phrase_count(body[PHRASE_TWICE], "quantum", "lattice", 0) == 2
    slop1 = {i for i, t in body.items() if phrase_count(t, "quantum", "lattice", 1)}
    assert set(PHRASE_SLOP1) <= slop1 - exact, slop1
    slop2 = {i for i, t in body.items() if phrase_count(t, "quantum", "lattice", 2)}
    assert PHRASE_REVERSED in slop2 - slop1, slop2
    assert exact < slop1 < slop2
    d0 = set(model.score_fuzzy("body", "lattice", 0))
    d1 = set(model.score_fuzzy("body", "lattice", 1))
    d2 = set(model.score_fuzzy("body", "lattice", 2))
    assert d0 < d1 < d2, (len(d0), len(d1), len(d2))
    assert FUZZY_PLANTS["latitce"] in d2 and FUZZY_PLANTS["latitce"] not in d1, "transposition must cost 2"
    for w in ("lattise", "latice", "lattices"):
        assert FUZZY_PLANTS[w] in d1
    assert FUZZY_PLANTS["lettuce"] in d2 and FUZZY_PLANTS["lettuce"] not in d1
    alpha = model.doc_freq("tags", "alpha")
    assert alpha > 10, f"need > 10 alpha-tagged docs for the k-boundary tie, got {alpha}"
    # title and body must share query vocabulary so the boost is observable
    assert model.doc_freq("title", "quantum") > 0 and model.doc_freq("body", "quantum") > 0
    assert model.doc_freq("title", "kernel") > 0 and model.doc_freq("body", "kernel") > 0
    # stemming observable: "running"/"runs" collapse to "run" in summary
    assert model.doc_freq("summary", "run") > model.doc_freq("summary", "running")
    # lossy fieldnorms exercised (body lengths exceed 40 tokens for some docs)
    assert any(len(t) > 40 for t in body.values())


# --------------------------------------------------------------------------------------------
# Verify (after the Rust minter): recompute every covered ranking and compare.
# --------------------------------------------------------------------------------------------
def verify_ranking(fixtures: Path) -> int:
    corpus = json.loads((fixtures / "corpus.json").read_text())
    queries = json.loads((fixtures / "queries.json").read_text())
    model = Model(corpus["documents"])
    failures = 0
    for entry in queries["queries"]:
        name = entry["name"]
        if entry["expected"] is None:
            print(f"  {name}: NOT MINTED")
            failures += 1
            continue
        if entry["oracle"] == "rust-only":
            print(f"  {name}: rust-only (skipped)")
            continue
        py = model.rank(entry["query"], entry["filter"], entry["k"])
        rust = entry["expected"]
        if entry["oracle"] == "python-membership":
            ok = {h["id"] for h in py} == {h["id"] for h in rust}
            print(f"  {name}: {'ok' if ok else 'MISMATCH'} (membership, {len(rust)} hits)")
            failures += 0 if ok else 1
            continue
        ok = [h["id"] for h in py] == [h["id"] for h in rust]
        if ok:
            for a, b in zip(py, rust):
                if abs(a["score"] - b["score"]) > SCORE_REL_TOL * max(abs(a["score"]), 1e-30):
                    ok = False
                    print(f"    id {a['id']}: python {a['score']!r} vs rust {b['score']!r}")
        else:
            print(f"    python ids {[h['id'] for h in py]}\n    rust   ids {[h['id'] for h in rust]}")
        # The k-boundary must be defensible: a gap inside the tolerance band but not zero is float
        # noise and the order is not meaningfully determined. An EXACT tie (gap == 0.0) is not
        # noise -- it is the case spec FR-014 defines (membership backend-chosen, deterministic),
        # so it is reported, not failed.
        note = ""
        if ok and len(py) == entry["k"]:
            all_scores = sorted(model.score_query(entry["query"]).values(), reverse=True)
            if len(all_scores) > entry["k"]:
                gap = all_scores[entry["k"] - 1] - all_scores[entry["k"]]
                if gap == 0.0:
                    note = ", exact tie at the k-boundary (FR-014)"
                elif gap <= SCORE_REL_TOL * all_scores[0] * 10:
                    ok = False
                    print(f"    k-boundary gap {gap} is inside the tolerance band; order not defensible")
        print(f"  {name}: {'ok' if ok else 'MISMATCH'} ({len(rust)} hits{note})")
        failures += 0 if ok else 1
    print(f"verify-ranking: {'PASS' if failures == 0 else f'FAIL ({failures})'}")
    return 1 if failures else 0


def refresh_manifest(fixtures: Path) -> int:
    """After ``gen_ranking`` mints ``queries.json``, re-hash it. Refuses if any entry is unminted or
    if ``--verify-ranking`` would fail, so a manifest never blesses an unchecked golden."""
    if verify_ranking(fixtures) != 0:
        print("refresh-manifest: refusing — verify-ranking failed")
        return 1
    manifest = json.loads((fixtures / "manifest.json").read_text())
    manifest["files"] = {name: sha256_file(fixtures / name) for name in manifest["files"]}
    write_json(fixtures / "manifest.json", manifest)
    return 0


# --------------------------------------------------------------------------------------------
def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def write_json(path: Path, payload: object) -> None:
    path.write_text(json.dumps(payload, indent=1, sort_keys=False, ensure_ascii=True) + "\n")
    print(f"  wrote {path.relative_to(REPO_ROOT) if path.is_relative_to(REPO_ROOT) else path}")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=2)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    ap.add_argument("--verify-ranking", type=Path, metavar="DIR")
    ap.add_argument("--refresh-manifest", type=Path, metavar="DIR",
                    help="recompute manifest.json hashes after the Rust minter rewrote queries.json")
    args = ap.parse_args()

    if args.verify_ranking:
        return verify_ranking(args.verify_ranking)
    if args.refresh_manifest:
        return refresh_manifest(args.refresh_manifest)

    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)
    print(f"gen_002_fixtures: seed={args.seed} out={out}")

    docs = gen_corpus(args.seed)
    model = Model(docs)
    assert_plants(model)

    write_json(out / "schema.json", schema_json())
    write_json(out / "corpus.json", {"seed": args.seed, "documents": docs})
    queries = gen_queries()
    # Re-running the generator must not silently un-mint goldens: keep an existing `expected` when
    # the entry's (query, filter, k) is unchanged. Changed entries go back to null and must be
    # re-minted and re-verified.
    existing_path = out / "queries.json"
    if existing_path.exists():
        prior = {e["name"]: e for e in json.loads(existing_path.read_text())["queries"]}
        for e in queries:
            old = prior.get(e["name"])
            if old and (old["query"], old["filter"], old["k"]) == (e["query"], e["filter"], e["k"]):
                e["expected"] = old.get("expected")
    write_json(out / "queries.json", {"score_rel_tol": SCORE_REL_TOL, "queries": queries})
    write_json(out / "filters.json", {"filters": gen_filters(model)})
    write_json(out / "stats.json", gen_stats(model))
    write_json(out / "mutations.json", gen_mutations(model, args.seed))
    names = ["schema.json", "corpus.json", "queries.json", "filters.json", "stats.json", "mutations.json"]
    write_json(out / "manifest.json", {
        "generator": "gen_002_fixtures.py", "seed": args.seed,
        "files": {n: sha256_file(out / n) for n in names},
    })
    print("gen_002_fixtures: done")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
