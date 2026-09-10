# ADR-0004: Ignore RUSTSEC-2024-0436 (`paste` unmaintained)

- **Status**: Accepted — 2026-09-11
- **Date**: 2026-09-11
- **Deciders**: mirth (repository owner), 2026-09-11
- **Spec**: [001-ios-build-spike](../../specs/001-ios-build-spike/spec.md)
- **Relates to**: [ADR-0001](./0001-pin-candle-0-9-2.md) (the candle pin that brings `gemm` in)

## Context

Principle VII requires `cargo deny check` to pass. Adding Feature 001's dependencies broke it.
Measured before and after, so the cause is unambiguous:

| | advisories | bans | licenses | sources |
|---|---|---|---|---|
| before the spike deps | ok | ok | ok | ok |
| after | **FAILED** | ok | ok | ok |

Exactly one finding:

```
error[unmaintained]: paste - no longer maintained
  ID: RUSTSEC-2024-0436
  https://rustsec.org/advisories/RUSTSEC-2024-0436
```

Reached only through candle's CPU matmul backend:

```
paste 1.0.15
└── gemm 0.19.0 (also gemm-c32, gemm-c64, pulp)
    └── candle-core 0.9.2 → candle-nn / candle-transformers → xtriever-ffi
```

Three facts that shape the decision:

1. **It is an *unmaintained* advisory, not a vulnerability.** `paste` is a proc-macro that expands
   identifiers at compile time. It has no CVE, no runtime presence in the shipped binary, and no
   known defect — the advisory says only that the author has archived it.
2. **It is unavoidable while using candle.** `gemm` is candle's CPU matmul backend, not an optional
   extra, and `paste` is `gemm`'s own transitive dependency. No candle feature flag removes it, and
   `bans` confirms this is a genuinely different problem from the `onig_sys` one ADR-0001 solved.
3. **The rest of `cargo deny` still passes.** `bans ok` in particular means ADR-0001's pin is doing
   its job — no `onig_sys` — so this decision must not disturb that.

## Decision

Add a single-entry ignore to `deny.toml`:

```toml
[advisories]
yanked = "deny"
ignore = ["RUSTSEC-2024-0436"]
```

Two things this deliberately is **not**:

- **Not a category.** No `unmaintained = "allow"`, no `severity-threshold`. One advisory id, so the
  next unmaintained crate to appear still fails the gate loudly.
- **Not a change to `[bans]`.** `onig_sys` stays denied outright. That ban is the tripwire that fires
  if anyone upgrades candle past 0.9.x, and it must keep working (ADR-0001).

`yanked = "deny"` is untouched, so yanked crates still fail.

## Consequences

**Positive**

- `cargo deny check` returns to all-green, unblocking task T006 and the blocking CI gate.
- The narrow scope means the gate keeps its value: a future advisory — including a real
  vulnerability anywhere in the tree, or a second unmaintained crate — still fails the build.
- The rationale lives in `deny.toml` next to the entry as well as here, so it is visible at the point
  of use rather than only in a document nobody re-reads.

**Negative**

- An ignored advisory that is never revisited is how a suppression list rots. That is the main risk,
  and the review triggers below exist to counter it.
- If `paste` is ever found to have a real defect, the advisory id stays the same and this ignore
  would silence it. The trigger list covers that case explicitly.
- It is a relaxation of a Principle VII gate, made to accommodate a dependency rather than to fix
  one. Recorded honestly as such.

**Review triggers** — revisit when any becomes true:

1. **RUSTSEC-2024-0436 is reclassified** from `unmaintained` to a vulnerability. This is the one that
   matters; treat it as a hard stop rather than a review.
2. `gemm` drops its `paste` dependency, or candle switches matmul backends.
3. Feature 001 concludes and candle is either kept or replaced — ADR-0001's own review triggers fire
   at roughly the same time, so check both together.
4. `paste` gains a maintained fork that candle adopts.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Leave `cargo deny check` red | It is a blocking CI gate under Principle VII, so every subsequent PR would ship with a known-red gate. That trains people to ignore it, which costs far more than this advisory does. |
| `unmaintained = "allow"` globally | Silences the entire category, including crates with genuinely abandoned unsafe code. One id keeps the signal. |
| Drop candle | Guts the spike — proving candle runs on-device is the feature's central question, and Principle I names candle as the default inference engine. |
| Switch the embedder to ONNX Runtime | Trades a documentation-only advisory for a C++ dependency (`ort-sys`) and a spec change. Disproportionate. |
| Vendor or fork `paste` | Forbidden by FR-005, and would make us the maintainer of a proc-macro we do not otherwise care about. |
