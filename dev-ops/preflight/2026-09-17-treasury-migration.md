# Launch treasury — runtime 221 dry run against the live dev chain

Runtime under test: `dev-ops/runtimes/vitreus_power_plant_testnet_runtime-221*.compact.compressed.wasm`
built from `38268a3` on `feature/solver-marketplace` (`--features testnet-runtime`
for the upgrade artefact, sha256 `f603eec167effee6…`; `testnet-runtime,try-runtime` for the dry run).
Live chain: spec 220, DEX storage version 2, launchpad storage version 0,
four pools, five launches. Rollback: `…-220.compact.compressed.wasm`, byte-identical
to the live `:code` (sha256 `aa4c3df3…`), read-only beside its `.sha256`.

What 221 carries (LAUNCH_TREASURY_SPEC §10.12): the treasury pallet at index
59, cherry-picked from `design/launch-treasury`, plus what only a running
chain needs — D9 as the DEX's v2 → v3 and L1 as the launchpad's v0 → v1,
both with migrations because this chain has pools and launches in the old
shapes, and `FundLaunchTreasuryVault`.

## The run

`chopsticks try-runtime --checks All` against `ws://127.0.0.1:9945`, hooks
on (the tool that ran D8's hooks; try-runtime-cli 0.10 does not). Console:
`2026-09-17-treasury-chopsticks-try-runtime.log`; storage diff and runtime
logs: `…-try-runtime.json`. Three runs: the first two found the three
things below, the third is the record.

```
🐥 New pallet "LaunchTreasury" detected … Initializing the on-chain storage version … StorageVersion(0)
🚚 Pallet "VitreusDex" VersionedMigration migration 0->1 can be removed; on-chain is already at StorageVersion(2).
🚚 Pallet "VitreusDex" VersionedMigration migration 1->2 can be removed; on-chain is already at StorageVersion(2).
D9 pre_upgrade: 4 pools to re-encode; default routing set: false
🚚 Pallet "VitreusDex" VersionedMigration migrating storage version from 2 to 3.
D9 migration: 4 pools re-encoded with treasury_bps = 0; default routing not set (zero)
D9 post_upgrade: 4 pools decode with treasury_bps = 0; default routing protocol 0 / creator 0 / treasury 0
L1 pre_upgrade: 5 launches, 5 curves, next id 5, governance params set: false
🚚 Pallet "Launchpad" VersionedMigration migrating storage version from 0 to 1.
L1 migration: 5 launches and 5 curves re-encoded with treasury_share_bps = 0; governance params not set: the runtime default applies, treasury share included
L1 post_upgrade: 5 launches and 5 curves decode; params (runtime default) protocol 2500 / treasury 2500
vault funded: Err(Token(NotExpendable))
✅ Entire runtime state decodes without error. 75919 bytes total.
🩺 try-state checks ran for all 77 pallets, LaunchTreasury included; none failed.
```

## The storage diff, decoded (22 keys)

| key | change |
|---|---|
| `Launchpad::Launches` × 5 | re-encoded: `curve.treasury_share_bps = 0`, everything else byte-equal |
| `Launchpad::Curves` × 5 | re-encoded: `treasury_fees_paid = 0`; `last_trade_block` = `graduated_at` for the three graduated, the migration block for the two trading |
| `Launchpad::__STORAGE_VERSION__` | 0 → 1 |
| `VitreusDex::Pools` × 4 | re-encoded: `routing.treasury_bps = 0`; reserves, fee tier, account byte-equal |
| `VitreusDex::__STORAGE_VERSION__` | 2 → 3 |
| `LaunchTreasury::__STORAGE_VERSION__` | new, 0 |
| `Launchpad::Params`, `VitreusDex::DefaultFeeRouting` | delete-writes of keys that were already absent (`translate` on an unset value); no-ops |
| `System::LastRuntimeUpgrade` | 221 |
| `:ethereum_schema`, `XcmPallet::CurrentMigration` | the permanent migrations' usual writes, as on every upgrade |

Nothing else. No account was touched: the vault was not funded (below).

## What the first two runs found

1. **`Params` and `DefaultFeeRouting` are absent on this chain** — governance
   never set them, so both are the runtime defaults. After 221 the launchpad
   default is `protocol 2,500 / treasury 2,500` (LAUNCH_TREASURY_SPEC §2.6):
   every *new* launch feeds its treasury without a `set_params`. The DEX
   default routing stays zero until `set_default_fee_routing`. Existing
   launches and pools carry treasury 0 — their snapshot. The first run's
   hooks asserted stored values and panicked on the default; fixed.
2. **`Treasury::account_id()` holds exactly one ED here** (fee recycling
   is in VNRG), so `FundLaunchTreasuryVault`'s `Preserve` transfer is
   `NotExpendable`. It logs and moves on; `VaultFunded` stays false, and
   the first fee withholds the ED itself — §9.6 running on the real chain.
   On testnet and mainnet the Treasury holds VTRS and the migration funds
   the vault as designed.
3. **I-T1 assumed the ED before the vault had it.** The second run's
   try-state failed LaunchTreasury's conservation check on the unfunded
   vault: a pallet bug from the §9.6 change, fixed on `design/launch-treasury`
   (`c6615bf`, red→green) and cherry-picked here (`38268a3`).

## What to check after the real `setCode`

1. Node log: the D9 and L1 lines above with the same counts; `vault funded: Err(Token(NotExpendable))`.
2. `system.lastRuntimeUpgrade` 221; storage versions launchpad 1, vitreusDex 3, launchTreasury 0.
3. `launchpad.launches(0..4).curve.treasuryShareBps` = 0; `launchpad.curves(id).lastTradeBlock` as in the table; `vitreusDex.pools(pair).routing.treasuryBps` = 0 for all four; reserves unchanged.
4. `launchpad.params()` (default) shows `treasuryShareBps: 2500`.
5. The site: `/launch/[id]` renders the treasury card ("No fee has reached the vault…"); `/api/index/status` keeper state moves from `unavailable` to `watching`.

## Then, by hand

- `sudo(launchTreasury.setTargets([<validator stash>]))` — `Targets` is empty at genesis of the pallet and `stake` is `NoTargets` until set.
- Optionally `sudo(vitreusDex.setDefaultFeeRouting(5, 5, 10))` so pools seeded from now on route a treasury slice.
- The keeper: `npx tsx indexer/keeper-key.ts /etc/vitreus-indexer/keeper.key`, fund the printed address (20 VTRS, and VNRG since the broker here may have nothing to sell), set `KEEPER_KEY_FILE` in the unit, restart `vitreus-indexer` (the restart also lets the event filter pick up the vault's address).
