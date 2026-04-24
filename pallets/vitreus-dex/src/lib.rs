//! # Vitreus DEX pallet
//!
//! A native AMM DEX pallet for the Vitreus blockchain. Scaffolded to mirror the
//! structure of `pallet-energy-broker`. Pallet index: 43. PalletId: `vtrs/dex`.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![allow(clippy::result_unit_err, clippy::too_many_arguments)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;

use frame_support::{
    traits::{
        fungibles::{Balanced, Inspect, Mutate},
        tokens::{Balance, Preservation::Expendable},
    },
    PalletId,
};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{
    traits::{
        AccountIdConversion, CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, IntegerSquareRoot,
        Zero,
    },
    RuntimeDebug,
};
use vitreus_runtime_common::{OnEnergyBurn, OnEnergySell};

/// The PalletId used to derive the DEX sovereign account.
pub const PALLET_ID: PalletId = PalletId(*b"vtrs/dex");

/// Denominator for the fee tier, expressed in 10ths of a percent.
pub const FEE_DENOMINATOR: u32 = 1_000;

/// Minimum liquidity permanently locked on first deposit to prevent first-depositor attacks.
pub const MINIMUM_LIQUIDITY: u32 = 1_000;

/// On-chain record of a trading pair's reserves, fee tier and dedicated sub-account.
#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PoolInfo<Balance, AccountId> {
    /// Current reserve of `asset_a` held by the pool.
    pub reserve_a: Balance,
    /// Current reserve of `asset_b` held by the pool.
    pub reserve_b: Balance,
    /// Swap fee tier for this pool, expressed in 10ths of a percent.
    pub fee_tier: u32,
    /// Cumulative fees collected over the pool's lifetime.
    pub total_fees_collected: Balance,
    /// Sub-account that physically holds the pool's reserves.
    pub pool_account: AccountId,
}

/// On-chain record of a single liquidity provider's position in a pool.
#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct LiquidityPosition<Balance, BlockNumber> {
    /// LP shares owned by the provider.
    pub shares: Balance,
    /// Block at which the position was opened.
    pub entry_block: BlockNumber,
    /// Block until which the position is locked, if any.
    pub locked_until: Option<BlockNumber>,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The origin which can manage parameters of this pallet.
        type ManageOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// The type in which the assets for swapping are measured.
        type Balance: Balance;

        /// Type of asset class used to provide liquidity.
        type AssetKind: Parameter + MaxEncodedLen;

        /// Registry of assets utilized for providing liquidity.
        type Assets: Inspect<Self::AccountId, AssetId = Self::AssetKind, Balance = Self::Balance>
            + Mutate<Self::AccountId>
            + Balanced<Self::AccountId>;

        /// Identifier of native asset.
        #[pallet::constant]
        type NativeAsset: Get<Self::AssetKind>;

        /// Identifier of energy asset.
        #[pallet::constant]
        type EnergyAsset: Get<Self::AssetKind>;
    }

    /// All known pools keyed by their canonical ordered asset pair.
    #[pallet::storage]
    pub type Pools<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        (T::AssetKind, T::AssetKind),
        PoolInfo<T::Balance, T::AccountId>,
    >;

    /// Per-provider liquidity positions keyed by account and pool.
    #[pallet::storage]
    pub type LiquidityPositions<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        (T::AssetKind, T::AssetKind),
        LiquidityPosition<T::Balance, BlockNumberFor<T>>,
    >;

    /// Total LP shares outstanding for each pool.
    #[pallet::storage]
    pub type TotalLiquidity<T: Config> =
        StorageMap<_, Blake2_128Concat, (T::AssetKind, T::AssetKind), T::Balance>;

    /// Cumulative energy sold through the on-chain hook.
    #[pallet::storage]
    pub type TotalEnergySold<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

    /// Cumulative energy burned through the on-chain hook.
    #[pallet::storage]
    pub type TotalEnergyBurned<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A new pool has been created.
        PoolCreated {
            /// First asset in the pair.
            asset_a: T::AssetKind,
            /// Second asset in the pair.
            asset_b: T::AssetKind,
            /// Fee tier for the new pool (10ths of a percent).
            fee_tier: u32,
        },
        /// Liquidity has been added to a pool.
        LiquidityAdded {
            /// Account providing the liquidity.
            provider: T::AccountId,
            /// First asset in the pair.
            asset_a: T::AssetKind,
            /// Second asset in the pair.
            asset_b: T::AssetKind,
            /// Amount of `asset_a` deposited.
            amount_a: T::Balance,
            /// Amount of `asset_b` deposited.
            amount_b: T::Balance,
            /// LP shares minted to the provider.
            shares_minted: T::Balance,
        },
        /// Liquidity has been removed from a pool.
        LiquidityRemoved {
            /// Account withdrawing the liquidity.
            provider: T::AccountId,
            /// First asset in the pair.
            asset_a: T::AssetKind,
            /// Second asset in the pair.
            asset_b: T::AssetKind,
            /// Amount of `asset_a` returned.
            amount_a: T::Balance,
            /// Amount of `asset_b` returned.
            amount_b: T::Balance,
            /// LP shares burned from the provider.
            shares_burned: T::Balance,
        },
        /// A swap has been executed against a pool.
        SwapExecuted {
            /// Originator of the swap.
            who: T::AccountId,
            /// Input asset.
            asset_in: T::AssetKind,
            /// Output asset.
            asset_out: T::AssetKind,
            /// Amount of `asset_in` consumed.
            amount_in: T::Balance,
            /// Amount of `asset_out` produced.
            amount_out: T::Balance,
            /// Fee charged on this swap.
            fee: T::Balance,
        },
        /// Accumulated fees have been collected from a pool.
        FeesCollected {
            /// Pool whose fees were collected.
            pool: (T::AssetKind, T::AssetKind),
            /// Total fee amount paid out.
            amount: T::Balance,
            /// Beneficiary of the collected fees.
            recipient: T::AccountId,
        },
        /// A liquidity position has been locked until a given block.
        LiquidityLocked {
            /// The account that locked the position.
            who: T::AccountId,
            /// The pool pair.
            pool: (T::AssetKind, T::AssetKind),
            /// The block until which the position is locked.
            locked_until: BlockNumberFor<T>,
        },
        /// The `OnEnergySell` hook was invoked against this pallet.
        EnergySold {
            /// The amount reported by the hook.
            amount: T::Balance,
        },
        /// The `OnEnergyBurn` hook was invoked against this pallet.
        EnergyBurned {
            /// The amount reported by the hook.
            amount: T::Balance,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// A pool already exists for this asset pair.
        PoolAlreadyExists,
        /// No pool exists for this asset pair.
        PoolNotFound,
        /// The pool does not hold enough liquidity to satisfy the operation.
        InsufficientLiquidity,
        /// The caller does not own enough LP shares.
        InsufficientShares,
        /// Calculated output falls outside the caller's slippage bounds.
        SlippageExceeded,
        /// The position or pool is currently locked.
        PoolLocked,
        /// The provided fee tier is not accepted.
        InvalidFeeTier,
        /// Amount can't be zero.
        ZeroAmount,
        /// An overflow happened.
        Overflow,
        /// Initial liquidity deposit is too small to exceed MINIMUM_LIQUIDITY.
        InsufficientInitialLiquidity,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Create a new AMM pool for the given asset pair and fee tier.
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(100_000_000, 10_000))]
        pub fn create_pool(
            origin: OriginFor<T>,
            asset_a: T::AssetKind,
            asset_b: T::AssetKind,
            fee_tier: u32,
        ) -> DispatchResult {
            T::ManageOrigin::ensure_origin(origin)?;

            // Finding 4: whitelist allowed fee tiers (0.1%, 0.3%, 1.0%).
            ensure!(
                fee_tier == 1 || fee_tier == 3 || fee_tier == 10,
                Error::<T>::InvalidFeeTier
            );

            // Finding 5: canonicalize pair to prevent duplicate pools.
            let pair = Self::canonical_pair(asset_a, asset_b);
            ensure!(!Pools::<T>::contains_key(&pair), Error::<T>::PoolAlreadyExists);

            // Finding 6: length-prefix each asset encoding to avoid truncation collisions.
            let pair_key = (pair.0.encode(), pair.1.encode());
            let pool_account: T::AccountId = PALLET_ID.into_sub_account_truncating(&pair_key);

            let pool = PoolInfo {
                reserve_a: Zero::zero(),
                reserve_b: Zero::zero(),
                fee_tier,
                total_fees_collected: Zero::zero(),
                pool_account,
            };

            Pools::<T>::insert(&pair, pool);
            TotalLiquidity::<T>::insert(&pair, T::Balance::zero());

            Self::deposit_event(Event::PoolCreated {
                asset_a: pair.0,
                asset_b: pair.1,
                fee_tier,
            });
            Ok(())
        }

        /// Add liquidity to an existing pool.
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(200_000_000, 20_000))]
        pub fn add_liquidity(
            origin: OriginFor<T>,
            asset_a: T::AssetKind,
            asset_b: T::AssetKind,
            amount_a: T::Balance,
            amount_b: T::Balance,
            amount_a_min: T::Balance,
            amount_b_min: T::Balance,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(
                amount_a > Zero::zero() && amount_b > Zero::zero(),
                Error::<T>::ZeroAmount
            );

            // Finding 5: canonicalize pair.
            let pair = Self::canonical_pair(asset_a, asset_b);
            let mut pool = Pools::<T>::get(&pair).ok_or(Error::<T>::PoolNotFound)?;
            let total_shares =
                TotalLiquidity::<T>::get(&pair).unwrap_or_else(T::Balance::zero);

            // Determine actual deposit amounts and shares to mint.
            let (actual_a, actual_b, total_new_shares, shares_to_mint) = if total_shares.is_zero()
            {
                // Finding 1: first deposit — burn MINIMUM_LIQUIDITY shares permanently.
                let raw_shares = amount_a
                    .checked_mul(&amount_b)
                    .ok_or(Error::<T>::Overflow)?
                    .integer_sqrt();
                let min_liq: T::Balance = MINIMUM_LIQUIDITY.into();
                ensure!(raw_shares > min_liq, Error::<T>::InsufficientInitialLiquidity);
                let shares_to_mint = raw_shares
                    .checked_sub(&min_liq)
                    .ok_or(Error::<T>::Overflow)?;
                // total includes the locked minimum; user only receives the remainder.
                (amount_a, amount_b, raw_shares, shares_to_mint)
            } else {
                // Finding 8: calculate optimal amounts — don't donate excess tokens.
                let optimal_b = amount_a
                    .checked_mul(&pool.reserve_b)
                    .ok_or(Error::<T>::Overflow)?
                    .checked_div(&pool.reserve_a)
                    .ok_or(Error::<T>::InsufficientLiquidity)?;

                let (actual_a, actual_b) = if optimal_b <= amount_b {
                    (amount_a, optimal_b)
                } else {
                    let optimal_a = amount_b
                        .checked_mul(&pool.reserve_a)
                        .ok_or(Error::<T>::Overflow)?
                        .checked_div(&pool.reserve_b)
                        .ok_or(Error::<T>::InsufficientLiquidity)?;
                    (optimal_a, amount_b)
                };

                let share_a = actual_a
                    .checked_mul(&total_shares)
                    .ok_or(Error::<T>::Overflow)?
                    .checked_div(&pool.reserve_a)
                    .ok_or(Error::<T>::InsufficientLiquidity)?;
                let share_b = actual_b
                    .checked_mul(&total_shares)
                    .ok_or(Error::<T>::Overflow)?
                    .checked_div(&pool.reserve_b)
                    .ok_or(Error::<T>::InsufficientLiquidity)?;
                let shares = if share_a < share_b { share_a } else { share_b };

                (actual_a, actual_b, shares, shares)
            };

            ensure!(shares_to_mint > Zero::zero(), Error::<T>::ZeroAmount);

            // Finding 7: enforce slippage on the actual (possibly adjusted) amounts.
            ensure!(actual_a >= amount_a_min, Error::<T>::SlippageExceeded);
            ensure!(actual_b >= amount_b_min, Error::<T>::SlippageExceeded);

            T::Assets::transfer(
                pair.0.clone(),
                &who,
                &pool.pool_account,
                actual_a,
                Expendable,
            )?;
            T::Assets::transfer(
                pair.1.clone(),
                &who,
                &pool.pool_account,
                actual_b,
                Expendable,
            )?;

            pool.reserve_a =
                pool.reserve_a.checked_add(&actual_a).ok_or(Error::<T>::Overflow)?;
            pool.reserve_b =
                pool.reserve_b.checked_add(&actual_b).ok_or(Error::<T>::Overflow)?;
            Pools::<T>::insert(&pair, &pool);

            let new_total =
                total_shares.checked_add(&total_new_shares).ok_or(Error::<T>::Overflow)?;
            TotalLiquidity::<T>::insert(&pair, new_total);

            let current_block = frame_system::Pallet::<T>::block_number();
            LiquidityPositions::<T>::try_mutate(
                &who,
                &pair,
                |maybe_pos| -> DispatchResult {
                    match maybe_pos {
                        Some(pos) => {
                            // Finding 9: keep original entry_block on top-up.
                            pos.shares = pos
                                .shares
                                .checked_add(&shares_to_mint)
                                .ok_or(Error::<T>::Overflow)?;
                        },
                        None => {
                            *maybe_pos = Some(LiquidityPosition {
                                shares: shares_to_mint,
                                entry_block: current_block,
                                locked_until: None,
                            });
                        },
                    }
                    Ok(())
                },
            )?;

            Self::deposit_event(Event::LiquidityAdded {
                provider: who,
                asset_a: pair.0,
                asset_b: pair.1,
                amount_a: actual_a,
                amount_b: actual_b,
                shares_minted: shares_to_mint,
            });
            Ok(())
        }

        /// Remove liquidity from an existing pool.
        #[pallet::call_index(2)]
        #[pallet::weight(Weight::from_parts(200_000_000, 20_000))]
        pub fn remove_liquidity(
            origin: OriginFor<T>,
            asset_a: T::AssetKind,
            asset_b: T::AssetKind,
            shares: T::Balance,
            amount_a_min: T::Balance,
            amount_b_min: T::Balance,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(shares > Zero::zero(), Error::<T>::ZeroAmount);

            // Finding 5: canonicalize pair.
            let pair = Self::canonical_pair(asset_a, asset_b);
            let mut pool = Pools::<T>::get(&pair).ok_or(Error::<T>::PoolNotFound)?;

            // Finding 2: sync reserves from actual balances before computing withdrawal.
            Self::sync_reserves(&pair, &mut pool);

            let total_shares =
                TotalLiquidity::<T>::get(&pair).ok_or(Error::<T>::PoolNotFound)?;
            ensure!(!total_shares.is_zero(), Error::<T>::InsufficientLiquidity);

            let mut position = LiquidityPositions::<T>::get(&who, &pair)
                .ok_or(Error::<T>::InsufficientShares)?;
            ensure!(position.shares >= shares, Error::<T>::InsufficientShares);

            // Finding 10: enforce lock check.
            if let Some(until) = position.locked_until {
                let current = frame_system::Pallet::<T>::block_number();
                ensure!(current >= until, Error::<T>::PoolLocked);
            }

            let amount_a = shares
                .checked_mul(&pool.reserve_a)
                .ok_or(Error::<T>::Overflow)?
                .checked_div(&total_shares)
                .ok_or(Error::<T>::InsufficientLiquidity)?;
            let amount_b = shares
                .checked_mul(&pool.reserve_b)
                .ok_or(Error::<T>::Overflow)?
                .checked_div(&total_shares)
                .ok_or(Error::<T>::InsufficientLiquidity)?;

            ensure!(amount_a >= amount_a_min, Error::<T>::SlippageExceeded);
            ensure!(amount_b >= amount_b_min, Error::<T>::SlippageExceeded);

            T::Assets::transfer(
                pair.0.clone(),
                &pool.pool_account,
                &who,
                amount_a,
                Expendable,
            )?;
            T::Assets::transfer(
                pair.1.clone(),
                &pool.pool_account,
                &who,
                amount_b,
                Expendable,
            )?;

            pool.reserve_a = pool
                .reserve_a
                .checked_sub(&amount_a)
                .ok_or(Error::<T>::InsufficientLiquidity)?;
            pool.reserve_b = pool
                .reserve_b
                .checked_sub(&amount_b)
                .ok_or(Error::<T>::InsufficientLiquidity)?;
            Pools::<T>::insert(&pair, pool);

            let new_total =
                total_shares.checked_sub(&shares).ok_or(Error::<T>::Overflow)?;
            TotalLiquidity::<T>::insert(&pair, new_total);

            position.shares =
                position.shares.checked_sub(&shares).ok_or(Error::<T>::Overflow)?;
            if position.shares.is_zero() {
                LiquidityPositions::<T>::remove(&who, &pair);
            } else {
                LiquidityPositions::<T>::insert(&who, &pair, position);
            }

            Self::deposit_event(Event::LiquidityRemoved {
                provider: who,
                asset_a: pair.0,
                asset_b: pair.1,
                amount_a,
                amount_b,
                shares_burned: shares,
            });
            Ok(())
        }

        /// Swap an exact amount of `asset_in` for as much `asset_out` as the pool yields.
        #[pallet::call_index(3)]
        #[pallet::weight(Weight::from_parts(200_000_000, 20_000))]
        pub fn swap_exact_tokens_for_tokens(
            origin: OriginFor<T>,
            asset_in: T::AssetKind,
            asset_out: T::AssetKind,
            amount_in: T::Balance,
            amount_out_min: T::Balance,
            recipient: T::AccountId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(amount_in > Zero::zero(), Error::<T>::ZeroAmount);

            // Finding 5: canonicalize pair for lookup.
            let pair = Self::canonical_pair(asset_in.clone(), asset_out.clone());
            let mut pool = Pools::<T>::get(&pair).ok_or(Error::<T>::PoolNotFound)?;

            // Finding 2: sync reserves from actual on-chain balances.
            Self::sync_reserves(&pair, &mut pool);

            let flipped = pair.0.encode() != asset_in.encode();
            let (reserve_in, reserve_out) = if flipped {
                (pool.reserve_b, pool.reserve_a)
            } else {
                (pool.reserve_a, pool.reserve_b)
            };

            ensure!(
                !reserve_in.is_zero() && !reserve_out.is_zero(),
                Error::<T>::InsufficientLiquidity
            );

            let fee_tier_bal: T::Balance = pool.fee_tier.into();
            let denominator_bal: T::Balance = FEE_DENOMINATOR.into();

            let fee = amount_in
                .checked_mul(&fee_tier_bal)
                .ok_or(Error::<T>::Overflow)?
                .checked_div(&denominator_bal)
                .ok_or(Error::<T>::Overflow)?;
            let amount_in_after_fee =
                amount_in.checked_sub(&fee).ok_or(Error::<T>::Overflow)?;

            let numerator = reserve_out
                .checked_mul(&amount_in_after_fee)
                .ok_or(Error::<T>::Overflow)?;
            let denom = reserve_in
                .checked_add(&amount_in_after_fee)
                .ok_or(Error::<T>::Overflow)?;
            let amount_out = numerator
                .checked_div(&denom)
                .ok_or(Error::<T>::InsufficientLiquidity)?;

            ensure!(amount_out >= amount_out_min, Error::<T>::SlippageExceeded);
            ensure!(amount_out < reserve_out, Error::<T>::InsufficientLiquidity);

            T::Assets::transfer(
                asset_in.clone(),
                &who,
                &pool.pool_account,
                amount_in,
                Expendable,
            )?;
            T::Assets::transfer(
                asset_out.clone(),
                &pool.pool_account,
                &recipient,
                amount_out,
                Expendable,
            )?;

            // Finding 3: only add amount_in_after_fee to reserves; the fee stays in
            // the pool account but is not counted in reserves until the next sync.
            if flipped {
                pool.reserve_b = pool
                    .reserve_b
                    .checked_add(&amount_in_after_fee)
                    .ok_or(Error::<T>::Overflow)?;
                pool.reserve_a = pool
                    .reserve_a
                    .checked_sub(&amount_out)
                    .ok_or(Error::<T>::InsufficientLiquidity)?;
            } else {
                pool.reserve_a = pool
                    .reserve_a
                    .checked_add(&amount_in_after_fee)
                    .ok_or(Error::<T>::Overflow)?;
                pool.reserve_b = pool
                    .reserve_b
                    .checked_sub(&amount_out)
                    .ok_or(Error::<T>::InsufficientLiquidity)?;
            }
            pool.total_fees_collected = pool
                .total_fees_collected
                .checked_add(&fee)
                .ok_or(Error::<T>::Overflow)?;

            let pool_account_for_event = pool.pool_account.clone();
            Pools::<T>::insert(&pair, pool);

            Self::deposit_event(Event::SwapExecuted {
                who,
                asset_in,
                asset_out,
                amount_in,
                amount_out,
                fee,
            });

            // Finding 11: emit FeesCollected event.
            Self::deposit_event(Event::FeesCollected {
                pool: pair,
                amount: fee,
                recipient: pool_account_for_event,
            });

            Ok(())
        }

        /// Lock a liquidity position until a given block.
        ///
        /// Finding 10: activates the previously dead `locked_until` field.
        #[pallet::call_index(4)]
        #[pallet::weight(Weight::from_parts(50_000_000, 5_000))]
        pub fn lock_liquidity(
            origin: OriginFor<T>,
            asset_a: T::AssetKind,
            asset_b: T::AssetKind,
            lock_until: BlockNumberFor<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let pair = Self::canonical_pair(asset_a, asset_b);

            LiquidityPositions::<T>::try_mutate(
                &who,
                &pair,
                |maybe_pos| -> DispatchResult {
                    let pos = maybe_pos.as_mut().ok_or(Error::<T>::InsufficientShares)?;
                    pos.locked_until = Some(lock_until);
                    Ok(())
                },
            )?;

            Self::deposit_event(Event::LiquidityLocked {
                who,
                pool: pair,
                locked_until: lock_until,
            });

            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// The account ID of the Vitreus DEX.
        pub fn account_id() -> T::AccountId {
            AccountIdConversion::<T::AccountId>::into_account_truncating(&PALLET_ID)
        }

        /// Finding 5: canonicalize a pair so that the lexicographically smaller
        /// encoded asset comes first. Prevents duplicate pools for (A,B) vs (B,A).
        pub fn canonical_pair(
            a: T::AssetKind,
            b: T::AssetKind,
        ) -> (T::AssetKind, T::AssetKind) {
            if a.encode() <= b.encode() {
                (a, b)
            } else {
                (b, a)
            }
        }

        /// Finding 2: sync pool reserves from actual on-chain asset balances.
        /// Absorbs any direct transfers or previously uncounted fees into the
        /// reserve tracking so the AMM math operates on accurate figures.
        fn sync_reserves(
            pair: &(T::AssetKind, T::AssetKind),
            pool: &mut PoolInfo<T::Balance, T::AccountId>,
        ) {
            pool.reserve_a = T::Assets::balance(pair.0.clone(), &pool.pool_account);
            pool.reserve_b = T::Assets::balance(pair.1.clone(), &pool.pool_account);
        }
    }
}

/// Finding 12: energy hooks now persist cumulative counters.
impl<T: Config> OnEnergySell<T::Balance> for Pallet<T> {
    fn on_energy_sell(amount: T::Balance) {
        TotalEnergySold::<T>::mutate(|total| {
            *total = total.checked_add(&amount).unwrap_or(*total);
        });
        Self::deposit_event(Event::EnergySold { amount });
    }
}

impl<T: Config> OnEnergyBurn<T::Balance> for Pallet<T> {
    fn on_energy_burn(amount: T::Balance) {
        TotalEnergyBurned::<T>::mutate(|total| {
            *total = total.checked_add(&amount).unwrap_or(*total);
        });
        Self::deposit_event(Event::EnergyBurned { amount });
    }
}
