# Pre-flight against live testnet state — 2026-09-14

Runtime under test: `runtime-219.wasm` (spec_version 219 on our numbering:
D6 + measured weights + hook drop + pallets 43/57), built from
`feature/solver-marketplace` at `f3e55b2`. Live testnet at the time:
spec 213, head 12,799,822, `LastRuntimeUpgrade {213}`, sudo key
`0x2F8CF06C0c21CA40eC4006d35C01B92a63d15d66`. Nothing was sent to testnet.

## try-runtime `on-runtime-upgrade live` (`2026-09-14-try-runtime.log`)

try-runtime-cli 0.10.1 against `wss://rpc.testnet.compliq.io:9945`, 10,959
keys at head, wasm built with `--features testnet-runtime,try-runtime`.

- `TryRuntime_on_runtime_upgrade` succeeded. frame-executive detected
  `VitreusDex` and `Launchpad` as new pallets and initialised their on-chain
  storage versions to the in-code values (1 and 0) before the `Unreleased`
  tuple ran, so `MigrateToV1` saw version 1 and was a no-op — the
  "VersionedMigration 0->1 can be removed" warning is that, not a fault.
  It stays for the dev chain's history.
- Migrations idempotent (identical storage root on the second run).
- No weight safety issues.
- `try_state` ran for all 76 pallets, `VitreusDex` and `Launchpad` included.
- Pre-existing, not ours: `Privileges` 0->1 "can be removed" and the
  `Ethereum` internal-migrations note are present at 213 already.

## chopsticks fork (`chopsticks-testnet.yml`, `2026-09-14-fork-launch.log`)

Forked at 12,799,856 with the wasm override; dev keys funded via storage.

Block-by-block, 12,799,857 → 12,799,888, **zero `ExtrinsicFailed`**:

- **12,799,857** — first block under 219 with `LastRuntimeUpgrade` 213 and no
  DEX pallet in state: `on_runtime_upgrade` ran; the only event is
  `xcmPallet.VersionMigrationFinished` (pallet_xcm's own lazy migration).
  After it: `LastRuntimeUpgrade {219}`, VitreusDex storage version 1,
  Launchpad 0, `launchpad.params` = the runtime default.
- 858–859 — inherents only.
- 860–864 — `create_launch` ×5 (repeated runs): `assets.ForceCreated`,
  `MetadataSet`, `Issued` (10^27 to the escrow), `launchpad.LaunchCreated`.
  Testnet-specific and harmless: `nacManaging.NftMinted` / `nfts.Issued`
  for each new escrow account (the NAC pallet greets any new account).
- 865–868 — `buy` 10 / 100 / 500 VTRS, `sell` 47.1M DLNCH: `Bought`/`Sold`
  with fees; curve state after each matches the pallet math.
- **869** — the crossing buy (2,527.56 VTRS taken of 500,000 offered):
  `CurveCompleted` (raised 2,999.999996 VTRS), `vitreusDex.PoolCreated`,
  `LiquidityAdded` (2,999.999996 VTRS + 200,000,000 DLNCH from the escrow),
  `LiquidityLocked` to 4,294,967,295, `ReservedPoolSeeded`,
  `launchpad.Graduated` (774,596,668,757,360,459,213,113 shares). Opening
  price vs curve end: relative diff 9.37e-10 (I7 bound 1e-6).
- 870 — `swap` 1 VTRS → 66,444.585 DLNCH through the seeded pool,
  `FeesCollected` 0.003 VTRS.
- 871 — `claim_creator_fees`: 16.2187 VTRS to the creator.
- 872–879 — the same sequence for launch #5 (872 also shows
  `paraInclusion.CandidateTimedOut`: the fork receives no parachain
  candidates; Foundation machinery, benign).
- 880 — a treasury spend period fell in this block: `treasury.Spending`,
  `Rollover`, `treasuryExtension.Recycled`, `technicalCommitteeTreasury.*`
  ran under our runtime alongside the creator claim.
- 881–882 — swaps from the site (`e2e-swap.mjs` through `/swap`): quote =
  chain formula, on-chain delta exact, `assetOut WithId(2^64+5)`.
- 883–888 — `add_liquidity` / `remove_liquidity` from the site's pool page
  (`e2e-liquidity.mjs`): positions open unlocked beside the escrow's
  permanent lock; D6 holds (recorded == held after each add).

Pallet-level diff live → fork: added `VitreusDex@43`, `Launchpad@57`;
removed none; **changed none** across the other 74 pallets' calls and
storage.

Fork artefacts, not runtime findings: the site's network-fee row reads
`--` on the fork (`energyFee_estimateCallFee` is a custom RPC chopsticks
does not serve); blocks take 10–80 s each (state is fetched lazily).
