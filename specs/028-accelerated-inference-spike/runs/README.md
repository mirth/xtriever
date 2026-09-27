# Feature 028 run records

One JSON record per measured run, named for people as

```
<device>-<computePath>-<single|batch>-<demo|harness|measure>-<timestamp>.json
```

- `demo`: the iOS demo's measured run (app-level latency and footprint, the Feature 009 shape)
- `harness`: the Swift package's device measurement (depths 0/5/10/20, parity, footprint)
- `measure`: the Python demo's `wikidemo measure` on the host

- `<device>`: `iPhone17,5` (the iPhone 16e) or `MacBookPro18,3` (the 2021 MacBook Pro, M1 Pro)
- `<computePath>`: `cpu`, `accelerate` or `metal`
- `single` / `batch`: the re-ranker scoring one pair at a time, or all pairs of a query at once

The variant is also inside every record (`computePath`, `rerankBatch`), and the record, not the
file name, is authoritative. Other files here:

- `goldens-<path>.txt`: the existing golden tests' outcome under each spike feature
- `build-<path>.json`, `mixed-scifact.json`: the host build measurements (US3)
- `<path>.hybrid-rerank-v3.<dataset>.json`: three-dataset quality for a recommended path (US2)
- `errors.md`: every path that would not build, link or run on a device, with its exact error

**No record may contain the phone's device identifier or the Apple developer team id.** Grep
this directory for both before committing.
