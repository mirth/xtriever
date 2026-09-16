"""Constants shared by the Feature 016 tests (kept out of `conftest.py` so `tests_014` and
`tests_016` can be collected in one pytest run without the two `conftest` modules shadowing
each other)."""

# Ten documents; the internal id (the tie key) is the corpus position. `d10` sorts before `d9`
# as a string but after it by position — the tie tests rely on that disagreement.
CORPUS_IDS = ["d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7", "d8", "d9", "d10"]
