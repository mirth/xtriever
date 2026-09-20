# Contract: the Android measurement record (Feature 025)

One JSON file per measured run, under `specs/025-android-kotlin-demo/runs/`, in the shape the
iPhone's and the laptop's records use so the three can be read side by side — with the fields
this platform needs and one it must not omit.

## Naming

`<emulated-device>-<UTC stamp>-<load path>-threads<n>.json`, mirroring Feature 019's
convention. No device identifier, serial number or account identifier appears in the name or
the body.

## Required fields

| Field | Type | Notes |
|---|---|---|
| `emulated` | boolean | **MUST** be `true` for every record this feature produces |
| `claims` | string | states what the figures do and do not support — correctness yes, physical-device latency no |
| `device` | object | the emulated profile, the system image, the API level |
| `host` | object | the machine and operating system running the emulator |
| `threads` | integer | the engine's effective pool size |
| `threadsSource` | string | where that number came from |
| `loadPath` | string | how the weights and vectors were brought into memory |
| `index` | object | documents, format version, corpus identity, both model fingerprints |
| `queries` | integer | how many measurement queries ran |
| `perDepth` | object | per re-rank depth: median and maximum milliseconds |
| `peakResidentBytes` | integer | measured peak memory |
| `ceilingBytes` | integer | the project's 600 MB phone ceiling, quoted for comparison only |
| `parity` | object | verdict, queries compared, hits compared, bits identical, maximum absolute difference per stage |

## Rules

- A record with `emulated: true` **MUST NOT** be compared with the iPhone's record in any
  document, and its `claims` string says so (spec FR-017).
- The parity verdict is computed by the same rule the iOS and Python records use: lexical bits
  exact, fused order identical, dense and re-rank scores within the stated tolerance per
  document, every hit present.
- A failing parity verdict is a stop-and-report. It is never resolved by widening the
  tolerance (Rule 6).
- Latency figures are medians over the measurement queries, never a single run, and the
  first search of a process is its warm-up and is excluded, as on the other platforms.
