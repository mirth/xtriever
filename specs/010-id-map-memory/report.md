# Report: Shrink the Id Map's Resident Memory

**Feature**: `010-id-map-memory` | **Date**: 2026-09-15 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Before (red commit)

The final method (`ids::tests::id_map_cost_per_passage`, counting allocator, `--release`) run
against the pre-change `IdMap` at the red commit:

| input | slots | id bytes | parent bytes | parents | **held after read** | per passage | **peak during read** | read |
|---|---|---|---|---|---|---|---|---|
| synthetic Wikipedia shape | 100,000 | 727,930 | 264,219 | 50,045 | 20,913,078 B | **209.1 B** | 32,317,510 B | 57 ms |
| `target/xt-wiki/index/ids.json` (008) | 427,947 | 3,441,429 | 1,421,907 | 239,436 | **88,529,028 B** | **206.9 B** | **138,643,027 B** | 287 ms |

Bounds the tests demand: held ≤ 7,058,405 B (synthetic) / 31,013,092 B (008 file); peak ≤
31,046,830 B / 79,827,657 B. Both fail on both inputs. Two copies are held by `HybridIndex`
(research D1: 174.7 MB of the 178.2 MB the open allocates). Write-back of the 008 file:
byte-identical, sha256 `20028054c295e4afa396654b98c0539a1fb28c799740127efc42fd55b3a39994`.

Red state of the lib test binary: `index::tests::committed_and_pending_share_until_a_change_is_staged`
does not compile (`Arc::ptr_eq` on plain `IdMap` fields) — the one compile error; with it
absent, `id_map_cost_per_passage` and `refuses_chunk_key_without_slot` fail, the rest pass.
