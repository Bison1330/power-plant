# runtime-benchmarks: make the benchmark build compile again; weight template and runbook

Three commits, no pallet code. Base: `develop` at `1aabbd2`. Split out of #100 at the Foundation's request; #100 will merge `develop` after this lands.

## Why

`cargo check --features runtime-benchmarks` has not compiled since the stable2407 upgrade, and `vitreus-cli benchmark` could never be built. Measured weights for any new pallet need both. The first two commits fix that and nothing else — `git diff 1aabbd2..3d2dfe7` shows no pallet logic — and the third adds the weight template the repairs assume and the runbook that produced #100's weights.

## What changed

**`ba90797` repair: the runtime-benchmarks build**
- `AssetId` is `u128` unconditionally. The `cfg` switch to `u32` under `runtime-benchmarks` measured a narrower type than production (smaller keys and values → lower weights) and broke `xcm_config`'s `GeneralIndex` arguments. `pallet_assets`' benchmarks get a helper building `Compact<u128>` from the `u32` seed.
- `pallet_nfts`: a `BenchmarkHelper` for Ethereum keys (`()` covers sr25519 only).
- The benchmark runtime API rewritten to the current `define_benchmarks!` / `add_benchmarks!` form with `whitelisted_storage_keys()`.
- Feature forwarding for `pallet-asset-rate`, `pallet-dynamic-energy` (its own feature was empty), `polkadot-runtime-common`, `polkadot-runtime-parachains`; `pallet-energy-fee`'s `frame-benchmarking` as an optional normal dependency; `pallet-faucet`'s benchmark updated to the current `request_funds` signature.

**`3d2dfe7` repair: cli** — `frame-benchmarking-cli` was never a dependency, so the declared `benchmark` subcommand could not be built.

**`fac1497` bench: `.maintain/frame-weight-template.hbs` and `pallets/BENCHMARKING.md`** — the standard template, and the runbook (hardware, `benchmark machine`, the commands, what to record). Written for the pallets in #100; the Foundation may relocate or generalise it.

## Mainnet effect: none

Every change is `#[cfg(feature = "runtime-benchmarks")]` or feature plumbing. `subwasm diff` of `mainnet-runtime` built from `1aabbd2` and from this branch: `No change detected`. `spec_version` untouched.

## Verified

```
cargo check -p vitreus-power-plant-runtime --features mainnet-runtime,runtime-benchmarks
cargo check -p vitreus-power-plant-runtime --features testnet-runtime,runtime-benchmarks
cargo check -p vitreus-cli --features runtime-benchmarks
cargo metadata --locked          # the lock carries the cli's new dependency
cargo +nightly fmt -- --check    # what Code Hygiene runs
```

The branch was rewritten once before opening (2026-09-17, `bench/repairs-before-rewrite` keeps the
old SHAs): the cli commit had added `frame-benchmarking-cli` to `cli/Cargo.toml` without the
`Cargo.lock` line, which fails `cargo build --locked` — Code Hygiene's first step — and both repair
commits carried a line `cargo +nightly fmt` reflows. Folded into the commits themselves; still three.

## One thing worth knowing

Every weight in this runtime generated under the previous `runtime-benchmarks` configuration — `pallet_assets` above all — was measured with `AssetId = u32`, narrower than the `u128` the chain runs, so it understates key and value sizes. With this build measuring the production types they can be regenerated; that is a finding about the runtime as it stands, not part of this PR.
