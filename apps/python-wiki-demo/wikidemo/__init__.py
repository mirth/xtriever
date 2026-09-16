"""Xtriever's Python Wikipedia demo (Feature 019, ``specs/019-python-wiki-demo``).

A command line over the ``xtriever`` package: ``wikidemo search`` prints the fused list,
then the re-ranked list with what moved, each hit's explanation on request and the engine's
stage report; ``wikidemo about`` the corpus identity, counts, model identities and the
attribution; ``wikidemo build`` an index from the raw Simple English Wikipedia snapshot with
the Feature 008 recipe; ``wikidemo measure`` the shipped index against the host goldens the
phone is checked against (or a demo-built slice against the Rust build of the same slice).

The demo owns no retrieval logic: every number it prints is the engine's or a wall clock
around one engine call.
"""

__version__ = "0.1.0"

#: The demo's re-rank depth: Feature 014 measured depth 10 at −0.3 mean nDCG@10 on the BEIR
#: sets for half the cross-encoder calls of the engine's default 20; Feature 018 made it
#: the iOS demo's default too.
DEFAULT_DEPTH = 10
#: The depths the demos offer (the ones measured on the device, Feature 017).
DEPTHS = (0, 5, 10, 20)
DEFAULT_K = 10
