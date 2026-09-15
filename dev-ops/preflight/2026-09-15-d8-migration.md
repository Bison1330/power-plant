# D8 migration dry run — runtime 220 against the live dev chain

Runtime under test: `dev-ops/runtimes/vitreus_power_plant_testnet_runtime-220*.compact.compressed.wasm`
built from `44a9fb0` (`--features testnet-runtime` for the upgrade artefact,
`testnet-runtime,try-runtime` for the dry runs). Live chain: spec 219, DEX
storage version 1, four pools.

## Two tools, because one of them does not run the hooks

`try-runtime-cli 0.10.1 on-runtime-upgrade --checks all|pre-and-post` never
executed `pre_upgrade` / `post_upgrade` for `MigrateToV2`: its first pass runs
with `checks: None`, the pass that carries the checks is the idempotency pass
on the *post-migration* state (storage version already 2, so
`VersionedMigration` skips the hooks with "can be removed"), and the
multi-block pass executes blocks, which run `on_runtime_upgrade` without the
try hooks. Log: `2026-09-15-d8-try-runtime-cli.log` — useful for idempotency,
weight (0.00515 s ref_time, 13.1 KiB PoV) and the full-state decode, not for
the hooks.

`chopsticks try-runtime --checks All` calls `TryRuntime_on_runtime_upgrade`
once on the live state with the checks selected, so the hooks fire. Log:
`2026-09-15-d8-chopsticks-try-runtime.log`.

## What the hooks said (chopsticks)

```
D8 pre_upgrade: 4 pools, all pre-D8 accounts distinct
🚚 Pallet "VitreusDex" VersionedMigration migrating storage version from 1 to 2.
D8 migration: 4 pools moved to hash-derived accounts
D8 post_upgrade: 4 pools at hash-derived accounts holding their reserves; pre-D8 accounts empty
✅ Entire runtime state decodes without error.
🩺 try-state checks ran for all 76 pallets.
```

## The storage diff, decoded

| pool | old account (tail) | new account (tail) | VTRS moved | token moved |
|---|---|---|---|---|
| VTRS/USDC | `…0400440103000000` | `…c43af0c5a10681c6` | 508,766.0248 | 491,415.263007 USDC |
| VTRS/BOARD | `…0400440102000000` | `…dbb6d4ca4640c277` | 3,132.5177 | 192,499,112.82 BOARD |
| VTRS/DLNCH | `…0400440100000000` | `…c7b2e4626f2884ab` | 3,011.0000 | 199,271,530.37 DLNCH |
| VTRS/GLASS | `…0400440101000000` | `…a24d1807c189ceeb` | 3,004.9209 | 199,676,443.96 GLASS |

- The four `Pools` records point at the new accounts; `reserve_a` /
  `reserve_b` unchanged. VTRS moved exceeds the recorded reserve by the
  uncounted fee (D6): USDC pool +0.025 VTRS, BOARD +0.075, DLNCH +0.03 — the
  same figures the indexer's `kept` fee events show.
- Old `System::Account` rows: free 0, providers 0 (reaped). Old
  `Assets::Account` rows removed. New rows hold the amounts above.
- `VitreusDex` storage version `0x0200` (2). `System::LastRuntimeUpgrade` set.
- Side effects of four new accounts existing, as when the pools were first
  funded: `Reputation` (8 keys), `NacManaging` (4) and its `Nfts` mint (13),
  `AssetsFreezer` (8). Nothing else in the diff.

## What to check after the real `setCode`

1. Node log: `D8 migration: 4 pools moved to hash-derived accounts`. No
   `could not move a pool's reserves` line.
2. `vitreusDex.pools(pair).poolAccount` = the new column; storage version 2;
   `system.lastRuntimeUpgrade` 220.
3. The old addresses hold nothing; the new ones hold the figures above.
4. `liquidityPositions`, `totalLiquidity`, launch escrows and the intent
   escrow (`…696e74656e747300`, 1,210 USDC) unchanged.
