# pallet-vitreus-dex and pallet-launchpad, with CI

One commit on `main` at `1a575a8`. Branch `pallets/dex-launchpad` on `Bison1330/power-plant-experimental`.
Split out of power-plant #100 at the Foundation's request: the pallets live here, the runtime wiring
stays in #100 (which will pin a SHA of this repository once this lands).

## What's in it

- `pallets/vitreus-dex` — constant-product pools, LP positions, per-pool fee routing (protocol / pool
  creator / launch treasury slices, snapshotted per pool), the solver marketplace. `SECURITY_AUDIT.md`
  beside it.
- `pallets/launchpad` — bonding-curve launches that graduate into a locked DEX pool. `pallets/LAUNCHPAD_SPEC.md`.
- `pallets/REVIEW_2026-09-17.md` — the adversarial review the pallets were fixed against (R1–R11), with
  the red test for each finding, and the proptest fuzz harness's method.
- Each pallet carries its tests, measured `weights.rs`, benchmarks and its storage migrations as
  `VersionedMigration`s (in-crate), so any pinned commit is right for a fresh chain and for one that ran
  an earlier version. The code is what runs on the dev chain today (power-plant
  `feature/solver-marketplace` `3c699c7`).
- `.github/workflows/ci.yml` — build, test, `runtime-benchmarks` check, `try-runtime` check,
  `fmt --check`, clippy with warnings denied; every step `--locked`. A consumer pinning a SHA wants a green
  one to pin, and there was no workflow here.

## Dependency alignment — the one thing to keep

`[workspace.dependencies]` names polkadot-sdk as `git = "https://github.com/paritytech/polkadot-sdk",
branch = "stable2407"` — the same strings as power-plant's `Cargo.toml` — and the committed `Cargo.lock`
resolves it to the same commit (`7642d6b5`). Cargo unifies a git dependency between this workspace and a
consumer only when the source strings match exactly; a `rev` or `tag` here against a `branch` there gives
the consumer two `frame-support`s and a type error at every `Config` boundary. When power-plant moves its
lock, this one moves to the same commit. The README says the same.

## Consuming

```toml
pallet-vitreus-dex = { git = "https://github.com/Vitreus-Foundation/power-plant-experimental", rev = "<sha>", default-features = false }
pallet-launchpad   = { git = "https://github.com/Vitreus-Foundation/power-plant-experimental", rev = "<sha>", default-features = false }
```

Pin a commit, never a branch; a fix is a commit here and a pin bump in the consumer. Benchmarks run
inside a consuming runtime — power-plant's `pallets/BENCHMARKING.md` (in the bench/repairs PR) is the
runbook.

## Verified locally (rust 1.83, the pinned toolchain)

```
cargo metadata --locked
cargo test --workspace --locked                      # 107 dex, 44 launchpad
cargo check --workspace --locked --features runtime-benchmarks
cargo check --workspace --locked --features try-runtime
cargo fmt --all -- --check                           # rustfmt.toml is power-plant's; stable and nightly agree on every file
cargo clippy --workspace --locked --all-targets -- --deny warnings
```

## Next

`pallet-launch-treasury` follows in its own PR here once its exchange trait is pallet-local (it
currently takes `vitreus-runtime-common`'s `QuotePrice`/`Swap`, which this workspace should not depend on).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
