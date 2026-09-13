//! Weights for pallet-vitreus-dex.
//!
//! PLACEHOLDER VALUES — NOT BENCHMARKED. These carry over the constants that
//! were previously hard-coded on each `#[pallet::weight]` so behaviour is
//! unchanged until real weights are generated. Regenerate with:
//!
//! ```text
//! target/release/vitreus-power-plant-node benchmark pallet \
//!   --chain dev --pallet pallet_vitreus_dex --extrinsic '*' \
//!   --steps 50 --repeat 20 --wasm-execution compiled \
//!   --output pallets/vitreus-dex/src/weights.rs \
//!   --template .maintain/frame-weight-template.hbs
//! ```
//!
//! Every DEX call has a fixed read/write footprint (no length-parametrised
//! arguments), so no function takes a component. Where a call has a cheaper
//! and a dearer branch, `benchmarking.rs` sets up the dearer one:
//! `register_solver` re-registers a previously deregistered account,
//! `commit_fill` displaces a prior commitment, `settle_intent` and
//! `slash_solver` perform every transfer (non-zero profit / bond split), and
//! `add_liquidity` extends an existing position.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for pallet-vitreus-dex.
pub trait WeightInfo {
    fn create_pool() -> Weight;
    fn add_liquidity() -> Weight;
    fn remove_liquidity() -> Weight;
    fn swap_exact_tokens_for_tokens() -> Weight;
    fn lock_liquidity() -> Weight;
    fn register_solver() -> Weight;
    fn deregister_solver() -> Weight;
    fn submit_intent() -> Weight;
    fn cancel_intent() -> Weight;
    fn commit_fill() -> Weight;
    fn settle_intent() -> Weight;
    fn slash_solver() -> Weight;
    fn refund_expired_intent() -> Weight;
    fn set_bid_window() -> Weight;
    fn set_settlement_window() -> Weight;
    fn set_solver_bond_amount() -> Weight;
}

/// Weights for pallet-vitreus-dex using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn create_pool() -> Weight {
        Weight::from_parts(100_000_000, 10_000)
    }
    fn add_liquidity() -> Weight {
        Weight::from_parts(200_000_000, 20_000)
    }
    fn remove_liquidity() -> Weight {
        Weight::from_parts(200_000_000, 20_000)
    }
    fn swap_exact_tokens_for_tokens() -> Weight {
        Weight::from_parts(200_000_000, 20_000)
    }
    fn lock_liquidity() -> Weight {
        Weight::from_parts(50_000_000, 5_000)
    }
    fn register_solver() -> Weight {
        Weight::from_parts(150_000_000, 15_000)
    }
    fn deregister_solver() -> Weight {
        Weight::from_parts(100_000_000, 10_000)
    }
    fn submit_intent() -> Weight {
        Weight::from_parts(200_000_000, 20_000)
    }
    fn cancel_intent() -> Weight {
        Weight::from_parts(150_000_000, 15_000)
    }
    fn commit_fill() -> Weight {
        Weight::from_parts(200_000_000, 20_000)
    }
    fn settle_intent() -> Weight {
        Weight::from_parts(300_000_000, 30_000)
    }
    fn slash_solver() -> Weight {
        Weight::from_parts(250_000_000, 25_000)
    }
    fn refund_expired_intent() -> Weight {
        Weight::from_parts(150_000_000, 15_000)
    }
    fn set_bid_window() -> Weight {
        Weight::from_parts(50_000_000, 5_000)
    }
    fn set_settlement_window() -> Weight {
        Weight::from_parts(50_000_000, 5_000)
    }
    fn set_solver_bond_amount() -> Weight {
        Weight::from_parts(50_000_000, 5_000)
    }
}

// For backwards compatibility and tests.
impl WeightInfo for () {
    fn create_pool() -> Weight {
        Weight::from_parts(100_000_000, 10_000)
    }
    fn add_liquidity() -> Weight {
        Weight::from_parts(200_000_000, 20_000)
    }
    fn remove_liquidity() -> Weight {
        Weight::from_parts(200_000_000, 20_000)
    }
    fn swap_exact_tokens_for_tokens() -> Weight {
        Weight::from_parts(200_000_000, 20_000)
    }
    fn lock_liquidity() -> Weight {
        Weight::from_parts(50_000_000, 5_000)
    }
    fn register_solver() -> Weight {
        Weight::from_parts(150_000_000, 15_000)
    }
    fn deregister_solver() -> Weight {
        Weight::from_parts(100_000_000, 10_000)
    }
    fn submit_intent() -> Weight {
        Weight::from_parts(200_000_000, 20_000)
    }
    fn cancel_intent() -> Weight {
        Weight::from_parts(150_000_000, 15_000)
    }
    fn commit_fill() -> Weight {
        Weight::from_parts(200_000_000, 20_000)
    }
    fn settle_intent() -> Weight {
        Weight::from_parts(300_000_000, 30_000)
    }
    fn slash_solver() -> Weight {
        Weight::from_parts(250_000_000, 25_000)
    }
    fn refund_expired_intent() -> Weight {
        Weight::from_parts(150_000_000, 15_000)
    }
    fn set_bid_window() -> Weight {
        Weight::from_parts(50_000_000, 5_000)
    }
    fn set_settlement_window() -> Weight {
        Weight::from_parts(50_000_000, 5_000)
    }
    fn set_solver_bond_amount() -> Weight {
        Weight::from_parts(50_000_000, 5_000)
    }
}
