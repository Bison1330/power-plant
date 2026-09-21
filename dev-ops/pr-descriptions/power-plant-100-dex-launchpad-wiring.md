# pallet-vitreus-dex and pallet-launchpad (testnet-runtime), with runtime-benchmarks repairs

## Mainnet effect: none

Both new pallets are wired under `testnet-runtime` only. The mainnet runtime built from this branch differs from the one built from `develop` (`1aabbd2`) in nothing but the build repairs, which are all `#[cfg(feature = "runtime-benchmarks")]` or feature plumbing. `subwasm diff` of the two `mainnet-runtime` wasms:

```
$ subwasm diff mainnet-BASE-1aabbd2.compact.compressed.wasm mainnet-PR.compact.compressed.wasm
No change detected
SUMMARY:
- Compatible.......................: true
- Require transaction_version bump.: false
```

`spec_version` is untouched (213); the Council bumps it in the release PR. The runtime carries no storage migration from this PR: the pallets are new, and frame-executive initialises a new pallet's on-chain storage version from the in-code one.

The testnet runtime gains exactly two pallets — the same diff, `testnet-runtime` builds from `develop` and from this branch:

```
$ subwasm diff testnet-BASE-1aabbd2.compact.compressed.wasm testnet-PR.compact.compressed.wasm
[+] id: 43 - new pallet: VitreusDex
[+] id: 57 - new pallet: Launchpad
SUMMARY:
- Compatible.......................: true
- Require transaction_version bump.: false
```

Both indices are free on the live testnet at spec 213. Enabling the DEX on mainnet is deliberately **not** part of this PR: the pallet has an internal security audit and a 96-test suite but no third-party audit, and that decision should be its own PR with that evidence attached.

## What the pallets do

**pallet-vitreus-dex** — a native DEX quoted in VTRS. Constant-product pools over `NativeOrWithId<u128>` pairs with 0.1 / 0.3 / 1.0 % fee tiers; `create_pool` is ManageOrigin, adding and removing liquidity is open to any account. LP positions carry per-position locks that can only be extended. Every swap's fee is split between the pool's reserves and VTRS-denominated protocol and creator slices, accrued in an escrow and pulled by their recipients. A bonded solver marketplace lets solvers fill user intents (exact input, minimum output, deadline; funds escrowed) with slashing for missed settlement and permissionless refunds after expiry. All pool arithmetic runs in U256 (u128 products overflow at 18-decimal scale). An in-runtime `PoolManager` / `ReservedPoolSeeder` interface lets another pallet create-or-adopt, seed and permanently lock a pool atomically over an asset range no extrinsic can create pools for.

**pallet-launchpad** (testnet only) — bonding-curve token launches. `create_launch` mints a fixed 10⁹ supply into a pallet-owned escrow and sells 80 % of it along a constant-product curve quoted in VTRS; `buy` / `sell` trade on it with exact pallet math and slippage minimums. The buy that sells the last token seeds a VTRS pool with the raised VTRS and the reserved 20 % through the DEX's seeder, locked forever, so trading continues in the pool at the curve's end price. Curve fees split between protocol and creator; creators claim theirs. Terms are governance-set and snapshotted per launch. Assets live at `2^64 + launch id`, a range the DEX reserves. It needs permissionless asset creation, which mainnet forbids by decision (`CreateOrigin = EnsureNever`) — the second reason it is testnet-only. Design document: `pallets/LAUNCHPAD_SPEC.md`.

## Why there are repairs in this PR

`cargo check --features runtime-benchmarks` had not compiled since the stable2407 upgrade, and shipping measured weights for the new pallets required it. The first two commits fix that and nothing else; a reviewer can `git diff 1aabbd2..<repair commit 2>` and see no pallet code.

- `AssetId` is `u128` unconditionally. The `cfg` switch to `u32` under `runtime-benchmarks` measured a narrower type than production (smaller keys and values, so lower weights) and broke `xcm_config`'s `GeneralIndex` arguments. `pallet_assets` benchmarks get a helper that builds `Compact<u128>` from the `u32` seed.
- `pallet_nfts`: a benchmark helper for Ethereum keys (`()` only covers sr25519).
- `pallet_treasury`, both instances: an `ArgumentsFactory`, since `AccountId20` and `NativeOrAssetId` don't implement `FromEntropy`.
- `xcm_config`: `ReachableDest` removed (gone from `pallet_xcm::Config`).
- The benchmark runtime API rewritten to the current `define_benchmarks!` / `add_benchmarks!` form with `whitelisted_storage_keys()`.
- Feature forwarding for `pallet-asset-rate`, `pallet-dynamic-energy` (its own feature was empty), `polkadot-runtime-common`, `polkadot-runtime-parachains`; `pallet-energy-fee`'s `frame-benchmarking` as an optional normal dependency; `pallet-faucet`'s benchmark updated to the current `request_funds` signature.
- `cli`: `frame-benchmarking-cli` was never a dependency, so the declared `benchmark` subcommand could not be built.

## Evidence

The tests and the four `cargo check` feature sets were run on this branch. The live-chain runs below used the fork's `testnet-runtime` build at spec 219 — this branch plus the spec bump and the DEX v0→v1 migration that is not submitted (the migration is a no-op on a chain with no DEX state, which is what both runs started from). Records: https://github.com/Bison1330/vitreusdex-pallet/tree/feature/solver-marketplace/dev-ops/preflight

- Tests: `pallet-vitreus-dex` 96 (116 with `runtime-benchmarks`); `pallet-launchpad` 40 (51 with `runtime-benchmarks`). The launchpad's tests are named for the failure modes in the spec (`fm01`–`fm16`), lifecycle, and a 600-step random buy/sell/claim walk checking every spec invariant after each step.
- `try-runtime on-runtime-upgrade live` against the public testnet at head (spec 213): succeeded; idempotent (identical storage root on the second run); no weight safety issues; `try_state` ran for all 76 pallets.
- A chopsticks fork of the live testnet at block 12,799,856 with this runtime as a wasm override: 32 blocks, zero `ExtrinsicFailed`. The first block ran `on_runtime_upgrade` with no DEX pallet in state. Two launches went create → buys/sells → crossing buy → `PoolCreated` / `LiquidityAdded` / `LiquidityLocked` / `ReservedPoolSeeded` / `Graduated` → pool swap → creator claim, with the opening price within 9.4e-10 of the curve's end price (the spec's I7 bound is 1e-6). A treasury spend period happened to fall inside the run and executed normally under this runtime.
- Pallet-level metadata diff, live testnet → fork: `VitreusDex@43` and `Launchpad@57` added, nothing removed, nothing changed in the other 74 pallets — the same result as the `subwasm diff` above from this branch.

## How to verify

```
cargo test -p pallet-vitreus-dex --lib
cargo test -p pallet-vitreus-dex --lib --features runtime-benchmarks
cargo test -p pallet-launchpad --lib
cargo test -p pallet-launchpad --lib --features runtime-benchmarks
cargo check -p vitreus-power-plant-runtime --features testnet-runtime
cargo check -p vitreus-power-plant-runtime --features mainnet-runtime
cargo check -p vitreus-power-plant-runtime --features testnet-runtime,runtime-benchmarks
cargo check -p vitreus-power-plant-runtime --features mainnet-runtime,runtime-benchmarks
# mainnet wasm from develop and from this branch, then:
subwasm diff <develop mainnet wasm> <branch mainnet wasm>
```
`scripts/devchain-launchpad.mjs` drives a launch end to end on a `--dev` node (well-known dev keys only).

## Known open items

- **The runtime's existing weights.** Every weight in this runtime generated under the previous `runtime-benchmarks` configuration — `pallet_assets` above all — was measured with `AssetId = u32`, a narrower type than the `u128` the chain runs, so it understates key and value sizes. They should be regenerated now that the benchmark build measures the production types. This is a finding about the runtime as it stands, not about the new pallets.
- **The new pallets' weights are conservative, not accurate.** Measured on a 16-vCPU box that scored 4/5 on `benchmark machine` (memory bandwidth 39.8 % of reference), `--steps 50 --repeat 20`. Safe on storage-heavy calls, to be re-measured on a 5/5 box per `pallets/BENCHMARKING.md` before any mainnet enablement. The raw `benchmark pallet` output is large and kept out of this repo: https://github.com/Bison1330/vitreusdex-pallet/tree/feature/solver-marketplace/dev-ops/benchmarks (`2026-09-14-machine.txt` is the `benchmark machine` output).
- **No third-party audit.** `pallets/vitreus-dex/SECURITY_AUDIT.md` is an internal review (12 findings, all resolved with tests). The launchpad's failure-mode analysis is in its spec. Mainnet enablement of the DEX should wait for an external audit.
- **D7.** `PoolInfo.total_fees_collected` adds each swap's fee in that swap's input asset, so a pool's counter mixes denominations and cannot be formatted or compared. Not consensus-relevant; to be split per asset or dropped with the next storage-touching change (`LAUNCHPAD_SPEC` §5.2).
- Still open in the launchpad spec (§8.2): a `try_state` hook mirroring the test-side invariant checks; a runtime API for curve quotes; an EVM precompile once VTRS-as-native lands.
- Operational: `system.events` lives in state, so an indexer for these pallets has to be running before the upgrade that adds them enacts on any chain (spec §8.2 item 4).

## Deliberately not in this PR

- `spec_version` (yours to bump), and any storage migration.
- Mainnet enablement of the DEX (separate decision, separate PR).
- Deployment tooling and pre-flight logs from our fork; the raw benchmark JSON.

## Commits

```
9ab42c3 repair: make the runtime-benchmarks build compile again (pre-existing)
157d14b repair: cli — the benchmark subcommand was never buildable (pre-existing)
e1b5978 bench: weight template and the benchmarking runbook
8cde917 pallet-vitreus-dex: constant-product AMM, LP positions, fee routing, solver marketplace
dfdcd96 runtime: wire pallet-vitreus-dex at index 43, testnet-runtime only
c9d8a12 pallet-launchpad: bonding-curve token launches that graduate into DEX pools
dea9816 runtime: wire pallet-launchpad at index 57, testnet-runtime only
ad3f021 docs: launchpad design spec, DEX internal security review, dev-chain launch script
```
The first two are repairs and carry no pallet code; the third is the weight template and benchmarking runbook; then the DEX pallet and its runtime wiring; then the launchpad pallet and its wiring; then documentation.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_01J5rGNkNthP1BdRnGLTf5Ne
