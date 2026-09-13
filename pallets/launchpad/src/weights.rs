//! Weights for pallet-launchpad.
//!
//! PLACEHOLDER VALUES — NOT BENCHMARKED. These carry over the constants that
//! were previously hard-coded on each `#[pallet::weight]` so behaviour is
//! unchanged until real weights are generated. Regenerate with:
//!
//! ```text
//! target/release/vitreus-power-plant-node benchmark pallet \
//!   --chain dev --pallet pallet_launchpad --extrinsic '*' \
//!   --steps 50 --repeat 20 --wasm-execution compiled \
//!   --output pallets/launchpad/src/weights.rs \
//!   --template .maintain/frame-weight-template.hbs
//! ```
//!
//! Requires `testnet-runtime`: the pallet is not part of the mainnet runtime.
//!
//! How the functions map onto the calls (§6.3):
//!
//! * `create_launch(n, s)` — creation without an initial buy; `n` and `s` are
//!   the name and symbol lengths (metadata write size and deposit). A call
//!   with `initial_buy > 0` is charged `create_launch(n, s) + buy_crossing()`
//!   up front and refunded to `create_launch(n, s) + buy()` when the buy did
//!   not cross.
//! * `buy()` — a buy that leaves tokens on the curve.
//! * `buy_crossing()` — the max-weight path: partial fill, then pool creation,
//!   liquidity seeding with a pre-seed sweep of both assets, and the permanent
//!   lock, all inside `do_buy`. `buy` is charged this and refunded to `buy()`
//!   when it does not cross.
//! * `graduate()` — the deferred seed on a `Complete` launch, same worst-case
//!   sweep as `buy_crossing`.
//! * `force_seed_into_existing_pool()` — governance rescue into a pool that
//!   already holds liquidity.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for pallet-launchpad.
pub trait WeightInfo {
    fn create_launch(n: u32, s: u32) -> Weight;
    fn buy() -> Weight;
    fn buy_crossing() -> Weight;
    fn sell() -> Weight;
    fn graduate() -> Weight;
    fn claim_creator_fees() -> Weight;
    fn set_creator_fee_recipient() -> Weight;
    fn set_params() -> Weight;
    fn set_creation_paused() -> Weight;
    fn force_seed_into_existing_pool() -> Weight;
}

/// Weights for pallet-launchpad using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn create_launch(_n: u32, _s: u32) -> Weight {
        Weight::from_parts(600_000_000, 40_000)
    }
    fn buy() -> Weight {
        Weight::from_parts(400_000_000, 30_000)
    }
    fn buy_crossing() -> Weight {
        Weight::from_parts(900_000_000, 70_000)
    }
    fn sell() -> Weight {
        Weight::from_parts(300_000_000, 20_000)
    }
    fn graduate() -> Weight {
        Weight::from_parts(500_000_000, 30_000)
    }
    fn claim_creator_fees() -> Weight {
        Weight::from_parts(150_000_000, 10_000)
    }
    fn set_creator_fee_recipient() -> Weight {
        Weight::from_parts(100_000_000, 10_000)
    }
    fn set_params() -> Weight {
        Weight::from_parts(50_000_000, 5_000)
    }
    fn set_creation_paused() -> Weight {
        Weight::from_parts(30_000_000, 3_000)
    }
    fn force_seed_into_existing_pool() -> Weight {
        Weight::from_parts(600_000_000, 40_000)
    }
}

// For backwards compatibility and tests.
impl WeightInfo for () {
    fn create_launch(_n: u32, _s: u32) -> Weight {
        Weight::from_parts(600_000_000, 40_000)
    }
    fn buy() -> Weight {
        Weight::from_parts(400_000_000, 30_000)
    }
    fn buy_crossing() -> Weight {
        Weight::from_parts(900_000_000, 70_000)
    }
    fn sell() -> Weight {
        Weight::from_parts(300_000_000, 20_000)
    }
    fn graduate() -> Weight {
        Weight::from_parts(500_000_000, 30_000)
    }
    fn claim_creator_fees() -> Weight {
        Weight::from_parts(150_000_000, 10_000)
    }
    fn set_creator_fee_recipient() -> Weight {
        Weight::from_parts(100_000_000, 10_000)
    }
    fn set_params() -> Weight {
        Weight::from_parts(50_000_000, 5_000)
    }
    fn set_creation_paused() -> Weight {
        Weight::from_parts(30_000_000, 3_000)
    }
    fn force_seed_into_existing_pool() -> Weight {
        Weight::from_parts(600_000_000, 40_000)
    }
}
