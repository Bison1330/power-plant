# pallet-launch-treasury (testnet-runtime): runtime wiring

*Prepared 2026-09-21, rebased onto merged `develop` (`8aba9b6`) on 2026-09-22 as
`6f4d4a0` — one commit, pushed to `Bison1330/power-plant`. #100 was squash-merged,
so this was replayed with `--onto` rather than onto its old base; the 84e69af tidy
(which moved the testnet wiring into a `launchpad` module) and 9beba3d (launchpad
calls on the flat custom fee — no interaction with this PR) are both absorbed.
Evidence below was run against this commit.*

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
by commit, pinned at **`db128ab`** (`main` after experimental #2) — the same rev #100
pins for `pallet-vitreus-dex` and `pallet-launchpad`. It has to be the same rev: the
treasury crate depends on both, and two revs would be two copies of each. `Cargo.lock`
gains that one package; polkadot-sdk stays at `7642d6b`.

## What this changes in #100's wiring, not just adds

Three of #100's values are replaced, and a reviewer should see that here rather than
in the diff:

| | #100 | This PR |
| --- | --- | --- |
| `pallet_vitreus_dex::Config::TreasurySink` | `()` — the treasury slice folds into the protocol share | `LaunchTreasury` |
| `pallet_launchpad::Config::CurveTreasurySink` | `()` | `LaunchTreasury` |
| Curve fee split, `protocol_share_bps` / `treasury_share_bps` (both `LaunchParams` literals) | 5,000 / 0 | 2,500 / 2,500 |

Why 2,500 / 2,500: the curve fee is 100 bps, split creator / protocol / treasury. The
treasury's quarter comes out of the protocol's half; the creator's 50% is untouched, so
no launch creator's terms change. It is the split the dev chain has run since runtime
221 (LAUNCH_TREASURY_SPEC §2.6), and it satisfies the launchpad's own bound
(`protocol + treasury ≥ 50%` of the fee, `validate_params`). The pool-side routing
(`set_default_fee_routing`) is not touched by this PR: it is storage, set by governance
after the upgrade, and `PoolInfo.routing` is snapshotted per pool at seed.

The two placeholders and the 0 were the right values for #100 on its own — with no
treasury pallet there is nowhere for the slice to go — and this PR is the point at
which there is.

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

And in `lib.rs`: the three replacements in the table above, `LaunchTreasury:
pallet_launch_treasury = 212` in `construct_runtime!`, and the benchmark list entry. Index 212 is the next slot in the 210–219 block #100 reserves for these pallets (VitreusDex 210, Launchpad 211).

## What is deliberately not here

- The pallet itself, its tests, spec, review and weights: in experimental.
- `spec_version`: yours.
- Pallet storage migrations (`pallet_vitreus_dex::migrations::v1..v3`,
  `pallet_launchpad::migrations::v1`, `pallet_launch_treasury::migrations::v1`):
  they exist inside the crates for chains that hold old-shape state, and the
  fork's dev chain does; the public testnet has none, and a new pallet needs none.
- `FundDexFeeEscrow`: the DEX-side counterpart of the vault funding (Finding 14).
  It belongs with the DEX wiring; if #100 lands without it, it can ride here.
- A `RuntimeCall::LaunchTreasury(..)` arm in `CustomFee`. 9beba3d puts
  `VitreusDex` and `Launchpad` calls on the flat custom fee (`base_fee ×
  multiplier`, the same as a transfer); the treasury pallet's calls (`stake`,
  `harvest`, `compound`, `retarget`, `retire`, …) stay on the default
  weight-based fee, as every pallet not in that list does. Whether they belong
  on the flat fee is a one-line follow-up for whoever owns the fee policy;
  none of it touches the pallet's own accounting, which is a share of curve
  and pool *trading* fees in VTRS, not the VNRG transaction fee.

## Evidence (run 2026-09-22 against `6f4d4a0`)

| Check | Result |
|---|---|
| `cargo metadata --locked`, `cargo +nightly fmt -- --check` | pass |
| `cargo clippy --locked -p vitreus-power-plant-runtime --features testnet-runtime -- -D warnings` | pass |
| `cargo check --locked -p vitreus-power-plant-runtime` — `testnet-runtime`, `mainnet-runtime` | pass |
| `cargo check --locked -p vitreus-power-plant-node` — `testnet-native`, `mainnet-native` | pass (the check that caught the 9c14dcf genesis break) |
| `runtime-benchmarks` build (testnet) | pass |
| `try-runtime` feature build — testnet, mainnet | pass |
| Pallet suites at the pinned `db128ab` — vitreus-dex / launchpad / launch-treasury | 108 / 44 / 40, all green |
| `subwasm diff` **mainnet** (develop → branch) | **No change detected.** Compatible, no `transaction_version` bump |
| `subwasm diff` **testnet** (develop → branch) | **`[+] id: 212 - new pallet: LaunchTreasury`**, Compatible, no `transaction_version` bump; plus two four-byte changes in pallet 211 `DefaultLaunchParams` and its storage default (offsets 18–21) — `protocol_share_bps` 5000 → 2500 and `treasury_share_bps` 0 → 2500, `0x1388` → `0x09C4` little-endian, i.e. the three replaced values in the table above and nothing else |
| `try-runtime on-runtime-upgrade --checks all` vs **mainnet** (`wss://rpc-mainnet.vtrs.io`, spec 213) | pass — migrations idempotent, every pallet's `try_state` green |
| `try-runtime on-runtime-upgrade --checks all` vs **testnet** (`wss://rpc.testnet.compliq.io:9945`, spec 213) | pass — `🐥 New pallet "LaunchTreasury" detected … initializing the on-chain storage version`, migrations idempotent, `try_state` green for `VitreusDex`, `Launchpad`, `LaunchTreasury` among the rest |

Both `try-runtime` runs use `--disable-spec-version-check`: this PR leaves
`spec_version` to the Foundation, so the branch carries the same 213 both chains
run and the guard refuses before executing anything. Runtime blobs for the diffs
are built without `try-runtime` (what would actually be deployed); the two runs
use separate builds with the feature compiled in.

Release blobs, built under the reproducible recipe (workspace bind-mounted at
`/build`, `RUSTUP_HOME=/rustup`, clean `wbuild`):

| Blob | sha256 (first 16) | bytes |
|---|---|---|
| branch testnet-runtime | `cb3b5a093fd08c7b…` | 2,326,594 |
| branch mainnet-runtime | `e2ec21503d1e2ad1…` | 2,230,851 |
| develop testnet-runtime | `b5820ba86884ab97…` | 2,293,922 |

## Original evidence checklist

- Done on `dbbbd5e`: `cargo check -p vitreus-power-plant-runtime --features
  testnet-runtime --locked` passes against the pinned crates. Still to run: the
  `runtime-benchmarks` and `try-runtime` checks, the node build, workspace tests.
- `subwasm diff` mainnet and testnet, `develop` → branch. Expected: mainnet no
  change; testnet `[+] id: 212 - new pallet: LaunchTreasury`, nothing else.
- `try-runtime on-runtime-upgrade live` against the public testnet: succeeds,
  idempotent, `FundLaunchTreasuryVault` transfers exactly one ED.
- A chopsticks fork of the public testnet with the wasm override: a launch
  trades, the vault receives its slice, `stake` / `harvest` / `compound` run.
- Records: `dev-ops/preflight/` on the fork (2026-09-17 treasury migration,
  2026-09-18 FM-17 + Finding 14 at 225 are the same pallet code on the fork's chain).
