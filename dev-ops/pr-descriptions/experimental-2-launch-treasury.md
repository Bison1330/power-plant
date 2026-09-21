# pallet-launch-treasury

One commit on top of `pallets/dex-launchpad` (the DEX + launchpad PR); open this against that
branch, or against `main` after it merges — the diff is the same. Branch `pallets/launch-treasury`
on `Bison1330/power-plant-experimental`.

## What it is

A slice of every launch-token trade — the curve's `treasury_share_bps` of the fee, the pool's
`treasury_bps` of the swap — is pushed by the launchpad and the DEX into one pallet-owned vault and
staked as a single cooperator of `energy-generation`. The LNRG the stake earns is sold to the energy
broker for VTRS and burns the token back on its own venue in impact-capped slices; a launch whose
venue has gone quiet is retired permissionlessly and its principal burned the same way. No call moves
VTRS or LNRG to a chosen account; governance sets terms and validator targets and has no withdraw,
redirect or unbond path. `pallets/LAUNCH_TREASURY_SPEC.md` is the design; its §10.12 is where pallet
code lives and how a consumer pins it.

## How it reaches the runtime

Two pallet-local traits, both with a runtime adapter of a few lines in the consumer and a mock here:

- `TreasuryStaking` — `energy-generation` as the vault sees it: bond / bond_extra / cooperate / chill /
  unbond / withdraw_unbonded dispatched as the vault, plus the reads `cooperate` checks of a target.
- `TreasuryExchange` — the energy broker as the vault sees it: `quote(lnrg)`, `depth()`, `sell(who,
  lnrg, min_native)`. Added for this move: the crate previously took `vitreus-runtime-common`'s
  `QuotePrice`/`Swap`, which this workspace must not depend on. Same calls, same arguments,
  `keep_alive` included.

The DEX and launchpad push fees through `TreasurySink`, bound in the runtime, so neither depends on
this crate. power-plant's adapters are `EnergyGenerationStaking` and `EnergyBrokerExchange` in its
runtime (`feature/solver-marketplace`).

## What travels with it

Tests (37, each finding in `REVIEW_2026-09-17.md` with its red test), the proptest harness
(`src/fuzz.rs`: arbitrary call sequences over the real DEX and launchpad with every invariant checked
after every step; 32 cases in `cargo test`, `PROPTEST_CASES` for more), measured `weights.rs`,
benchmarks (validators and broker depth through `BenchmarkHelper`), and `migrations::v1` — the R1
recount, a `VersionedMigration` — so a chain that ran v0 and a fresh chain both get the right state.

`Cargo.lock` gains `proptest = 1.5.0` (the last release for rustc 1.83) and its tree; every version
shared with power-plant's lock is the same.

## Verified locally (rust 1.83)

```
cargo metadata --locked
cargo test --workspace --locked                      # 107 dex, 44 launchpad, 38 treasury (incl. fuzz)
cargo check --workspace --locked --features runtime-benchmarks
cargo check --workspace --locked --features try-runtime
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- --deny warnings
```

🤖 Generated with [Claude Code](https://claude.com/claude-code)
