# pallet-launch-treasury (testnet-runtime): runtime wiring

*Draft, prepared 2026-09-21 on `pr/launch-treasury-wiring` (`dfcd4c2`, local). Opens
against `develop` once #100 merges; rebase and re-run the evidence first. Not
posted.*

## Mainnet effect: none

The pallet is wired under `testnet-runtime` only, like the launchpad it serves. On
mainnet the DEX's `TreasurySink` and the launchpad's `CurveTreasurySink` are `()`,
which folds the treasury slice into the protocol share, and the pallet is not in
`construct_runtime!`. `spec_version` is untouched; the Council bumps it in the
release PR. No pallet storage migration is listed: the pallet is new here, and
frame-executive initialises a new pallet's on-chain storage version from the
in-code one.

*(subwasm diff mainnet develop → this branch: to be pasted after rebase; expected
"No change detected".)*

## What it depends on

`pallet-launch-treasury` is consumed from `Vitreus-Foundation/power-plant-experimental`
by commit, pinned at **`db128ab`** (`main` after experimental #2). It depends on
`pallet-vitreus-dex` and `pallet-launchpad` from the same repository, so the three
pins must be one rev: two revs would be two copies of each crate and the runtime
would not type-check. #100 pins the DEX and the launchpad; if it pins `254ca48`
(`main` after #1), this PR moves the pin to `db128ab`. The DEX and launchpad
sources are identical at the two commits (#2 added only the treasury crate, its
spec, the runbook and the workspace entries), so the move changes nothing in them.

## What the runtime provides

All in `runtime/vitreus/src/launch_treasury.rs` (292 lines, one module so the
consumer's side is reviewable in one place):

- `EnergyGenerationStaking` — the pallet's `TreasuryStaking` adapter over
  `energy-generation`: bond, cooperate, unbond, withdraw, payout, dispatched as
  `Signed(vault)`; the pallet has no in-runtime staking trait.
- `EnergyBrokerExchange` — its `TreasuryExchange` adapter over the energy broker's
  fixed-rate LNRG → VTRS path: `quote`, `depth`, `sell`.
- Terms: `PalletId` `vtrs/lpt`, `MaxTargets` 16, `MaxUnlockingChunks` shared with
  `energy-generation`, default `TreasuryTerms`.
- `LaunchTreasuryBenchmarkHelper` under `runtime-benchmarks`.
- `FundLaunchTreasuryVault` — an `OnRuntimeUpgrade` in `Unreleased` that gives the
  vault its existential deposit once, from the Treasury, so the first routed fee is
  not withheld (spec §9.6). Idempotent (`providers > 0` short-circuits); it is a
  funding step, not a storage migration.

And in `lib.rs`, the connections: `type TreasurySink = LaunchTreasury` on the DEX
(testnet) and `type CurveTreasurySink = LaunchTreasury` on the launchpad;
`LaunchTreasury: pallet_launch_treasury = 59` in `construct_runtime!`; the
benchmark list entry; and the curve fee split creator 50 / protocol 25 /
treasury 25 (`LAUNCH_TREASURY_SPEC §2.6`). Index 59 is free on the live testnet.

## What is deliberately not here

- The pallet itself, its tests, spec, review and weights: in experimental.
- `spec_version`: yours.
- Pallet storage migrations (`pallet_vitreus_dex::migrations::v1..v3`,
  `pallet_launchpad::migrations::v1`, `pallet_launch_treasury::migrations::v1`):
  they exist inside the crates for chains that hold old-shape state, and the
  fork's dev chain does; the public testnet has none, and a new pallet needs none.
- `FundDexFeeEscrow`: the DEX-side counterpart of the vault funding (Finding 14).
  It belongs with the DEX wiring; if #100 lands without it, it can ride here.

## Evidence (to run after rebase, before opening)

- `cargo check -p vitreus-power-plant-runtime --features testnet-runtime`, and
  with `runtime-benchmarks` and `try-runtime`; the node build and workspace tests.
- `subwasm diff` mainnet and testnet, `develop` → branch. Expected: mainnet no
  change; testnet `[+] id: 59 - new pallet: LaunchTreasury`, nothing else.
- `try-runtime on-runtime-upgrade live` against the public testnet: succeeds,
  idempotent, `FundLaunchTreasuryVault` transfers exactly one ED.
- A chopsticks fork of the public testnet with the wasm override: a launch
  trades, the vault receives its slice, `stake` / `harvest` / `compound` run.
- Records: `dev-ops/preflight/` on the fork (2026-09-17 treasury migration,
  2026-09-18 FM-17 + Finding 14 at 225 are the same pallet code on the fork's chain).
