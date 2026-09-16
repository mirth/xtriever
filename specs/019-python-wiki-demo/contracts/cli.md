# Contract: the `wikidemo` command line

`wikidemo <subcommand> [flags]`, also `python -m wikidemo …`. Output to stdout; diagnostics
and progress to stderr. Exit codes: `0` success (an empty result is success), `1` a missing
input, an engine error, a parity `FAIL` or a refused build, `2` a usage error (argparse).

## Common flags (every subcommand)

| Flag | Environment | Default | Meaning |
|---|---|---|---|
| `--artefact DIR` | `XTRIEVER_WIKI_ARTEFACT` | `target/xt-wiki` | the 008-shaped artefact: `DIR/index`, `DIR/ATTRIBUTION.txt` |
| `--embedder DIR` | `XTRIEVER_MODEL_DIR` | `reference/models/all-MiniLM-L6-v2` | the pinned embedder |
| `--reranker DIR` | `XTRIEVER_RERANK_MODEL_DIR` | `reference/models/ms-marco-MiniLM-L-6-v2` | the pinned re-ranker |

Relative defaults resolve against the repository root (found from the demo's own location);
an absolute path is used as given.

**Missing-input message** (stderr, exit 1, before any model loads):

```
wikidemo: missing <what>: <path>
  produce it with: <command>
```

## `search`

```
wikidemo search [-k N] [--depth {0,5,10,20}] [--budget-ms MS] [--strict]
                [--mode {interpolate,replace}] [--explain] [--snippet CHARS] QUERY
```

| Flag | Default | Engine option |
|---|---|---|
| `-k` | 10 | `SearchOptions.k` |
| `--depth` | **10** (the demo's default; the engine's is 20 — help text states the 018 trade-off) | `SearchOptions.rerank_depth` of the second call |
| `--budget-ms` | none | `SearchOptions.max_time_ms`; a negative value is a usage error (exit 2) |
| `--strict` | off | `SearchOptions.strict` |
| `--mode` | interpolate (the engine's default rule, α 0.5) | `SearchOptions.rerank_mode`, explicit either way: `RerankMode.INTERPOLATE(alpha=0.5)` / `RerankMode.REPLACE()` — never `None`, which would mean "whatever the index recorded" (review round 1) |
| `--explain` | off | prints the eight features per hit (both calls run with `explain=True` regardless — the marks and the parity need the ids only, but the explanation is free) |
| `--snippet CHARS` | none (whole passages) | passages longer than CHARS are cut with "…" and the list header says "passages cut to N characters" |

Output, in order (research D16):

1. `opened <artefact> (<documents> passages, format <v>) · embedder <ms> ms · re-ranker <ms> ms · open <ms> ms · mmap`
2. `warm-up: the first search of a process pages the vectors in` (always — every process's first search is its warm-up; a second query in the same process is not possible from the CLI)
3. `fused (lexical + dense), <n> hits, <wall> ms` then the hits, each: `<rank>. <title>  <external_id>  passage <ordinal+1>`, the URL line (omitted when the text has no title line), the passage (indented), and with `--explain` eight lines `<name>: <value | not seen by this stage>`
4. unless `--depth 0`: `re-ranked (<mode>, depth <d>), <n> hits, <wall> ms` then the hits with a mark column (`↑n` / `↓n` / `=` / `new`), then `dropped from the head: <id> (was <rank>)…` when any
5. `stages: lexical <n> · dense <n | skipped (<reason>)> · re-rank <c> candidates, <s> scored [· re-rank skipped (<reason>)] [· degraded: <stage> (<reason>)] · time limit ignored: <yes|no> · engine <ms> ms` (for the last call; the fused call's stage line is printed after its list when `--depth 0`)
6. `wall: fused <ms> ms · re-ranked <ms> ms · total <ms> ms · peak resident <MB> MB`
7. no hits: `no passages found for "<query>"` in place of the list, exit 0

Engine error (`XtrieverError`): `wikidemo: <ErrorKind>: <message>` on stderr, exit 1; under
`--strict` a budget exhaustion is such an error, without it a degradation line.

## `about`

```
wikidemo about
```

Prints, labelled, one per line: corpus (edition, snapshot date), articles / selected /
passages (and `partial: first N articles` when set), corpus identity, embedder fingerprint,
re-ranker id, format version, candidate depth, rrf k, `re-rank depth (engine default): 20`,
`re-rank depth (demo default): 10`, `re-rank mode (recorded): …` (the index's), this session's open / embedder load /
re-ranker load ms, then a blank line and `ATTRIBUTION.txt` verbatim, then the licence URL.

## `build` — lands with PR B; not offered by PR A's parser (`invalid choice`, exit 2)

```
wikidemo build --out DIR [--limit N] [--snapshot FILE] [--manifest FILE]
```

- `--out DIR` required; `DIR` must not exist (refused with exit 1); the build writes
  `DIR.partial/` and renames it to `DIR/` when complete.
- `--limit N` (N ≥ 1): the first N articles of the snapshot (read order); `0` is a usage
  error. Without `--limit` the command first prints
  `full build: 427,947 passages ≈ 11 h on a laptop (Feature 008 measured 93 ms per passage); Ctrl-C now if that is not what you want`
  and pauses 5 s before starting.
- `--snapshot` default `reference/datasets/wiki/simple.jsonl`; `--manifest` default
  `reference/datasets/wiki-manifest.json`. The snapshot is verified (bytes, sha256)
  against the manifest before reading; a mismatch prints both hashes and
  `scripts/fetch-wiki.sh`, exit 1.

Progress (stderr), one line per 4,096-passage batch:
`batch <i>: articles <read> (excluded <n>), passages <total>, <elapsed> s`.

Completion (stdout): the counts (`articles`, per-rule `excluded`, `selected`, `passages`,
`url_mismatches`), the phase timings, the corpus identity, and `wrote DIR`.

Result layout: `DIR/index/` (the engine's index with `corpus.json` inside),
`DIR/ATTRIBUTION.txt`, `DIR/wiki-build.json` — searchable with `--artefact DIR`.

## `measure`

```
wikidemo measure [--expected FILE | --against DIR] [--queries FILE] [--out FILE]
```

- Default: `--expected <artefact>/expected.json` (the host goldens). `--against DIR`
  opens a second artefact and compares live responses instead (slice parity, research
  D14); the two are mutually exclusive.
- `--queries` default `reference/fixtures/008/queries.json`.
- `--out` default `specs/019-python-wiki-demo/runs/<machine>-<YYYYMMDDTHHMMSS>Z-mmap-threads<n|default>.json`
  (`slice-` prefix in `--against` mode).
- Runs one warm-up search (the first query at depth 0, not counted), then every query at
  depths 0 / 5 / 10 / 20 with `k=10, explain=True`; prints per-depth medians and maxima,
  the footprint and the parity verdict; writes the record (contracts/records.md); exit 1
  on `FAIL` after writing.
- With `--against`, the order of ids is checked at **every** depth (two builds on one host
  have no drift to tolerate — spec FR-014), and the two `corpus.json`s' identity and counts
  and the two indexes' document counts must be equal: any inequality is a `FAIL` of the
  `against` block and exit 1, whatever the score parity says.
- On a `partial` artefact without `--against`: refused —
  `the host goldens describe the full corpus; compare a slice with --against`.
