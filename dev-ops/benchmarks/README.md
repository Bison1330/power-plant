# Benchmark provenance

Raw output of the `benchmark pallet` runs whose numbers are checked in as
`pallets/*/src/weights.rs`, plus the `benchmark machine` score of the box
they ran on. See `pallets/BENCHMARKING.md` for the procedure.

| Date | Commit | Box | Machine score | Files |
|---|---|---|---|---|
| 2026-09-14 | `426699a` (+ `ec6062a`, `7158265`) | DigitalOcean c-16, 16 dedicated vCPU, 32 GB, Regular Intel (Xeon Platinum 8280 @ 2.70 GHz), NYC1 | **4/5** — CPU and disk pass (BLAKE2-256 143 %, SR25519 113 %, seq write 105 %, rnd write 108 %); **Memory Copy 39.8 %** (4.58 GiB/s vs 11.49 required) | `2026-09-14-{machine.txt,vitreus-dex.json,launchpad.json}` |

The memory-bandwidth miss means these weights are conservative on
storage-heavy calls (they err safe) and must be re-measured on hardware
that passes `benchmark machine` cleanly before mainnet — LAUNCHPAD_SPEC
§8.2.
