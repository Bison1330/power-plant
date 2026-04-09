//! # HiveIQ Settlement pallet
//!
//! On-chain settlement layer for the HiveIQ multi-provider LLM orchestration
//! service. The off-chain orchestration oracle submits a signed receipt for
//! every completed inference task; this pallet escrows the VTRS payment,
//! releases it to the provider on `settle_task`, and tracks per-provider
//! reputation. Disputed receipts can be slashed by governance, burning a
//! configurable share of the escrow.
//!
//! ## Overview
//!
//! Lifecycle of a single task:
//!
//! 1. Off-chain: a HiveIQ client submits an inference task; the orchestration
//!    layer routes it to a provider and runs it.
//! 2. On-chain: the orchestration oracle calls [`Pallet::submit_receipt`],
//!    transferring `cost_vtrs` from the oracle account into a pallet-owned
//!    escrow account and recording a [`TaskReceipt`].
//! 3. On-chain: anyone (typically the provider or the oracle) calls
//!    [`Pallet::settle_task`], which releases the escrowed VTRS to the
//!    provider and bumps the provider's [`ProviderStats`].
//! 4. Optional: if the client disputes the receipt, governance calls
//!    [`Pallet::slash_provider`], which burns a percentage of the escrow,
//!    decrements the provider's reputation, and increments the slash count.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![allow(clippy::too_many_arguments)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;

use frame_support::{
    traits::{Currency, ExistenceRequirement, WithdrawReasons},
    PalletId,
};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
    traits::{AccountIdConversion, SaturatedConversion, Saturating, Zero},
    Percent, RuntimeDebug,
};

/// Privacy tier requested for a task. Mirrors the off-chain `privacy_tier`
/// field on the orchestration layer's `TaskRequest`.
#[derive(Clone, Copy, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, PartialEq, Eq)]
pub enum PrivacyTier {
    /// Standard hosted inference.
    Public,
    /// Decentralized compute with stake-based verification.
    SemiPrivate,
    /// Trusted Execution Environment-backed inference.
    Tee,
}

impl PrivacyTier {
    /// Wire-format conversion from the `u8` used at the extrinsic boundary.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Public),
            1 => Some(Self::SemiPrivate),
            2 => Some(Self::Tee),
            _ => None,
        }
    }
}

/// Lifecycle state of a task receipt.
#[derive(Clone, Copy, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, PartialEq, Eq)]
pub enum ReceiptStatus {
    /// Receipt has been submitted; funds are sitting in escrow.
    Pending,
    /// Funds have been released to the provider.
    Settled,
    /// Receipt was disputed; escrow has been (partially) burned.
    Slashed,
}

/// Per-task on-chain receipt.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, PartialEq, Eq)]
pub struct TaskReceipt<AccountId, Balance> {
    /// Provider account that fulfilled the task.
    pub provider_id: AccountId,
    /// VTRS owed to the provider on settlement (before any slashing).
    pub cost_vtrs: Balance,
    /// Privacy tier the task was processed under.
    pub privacy_tier: PrivacyTier,
    /// Block number at which the receipt was recorded.
    pub timestamp: u64,
    /// Current state of the receipt.
    pub status: ReceiptStatus,
}

/// Running totals and reputation for a single provider.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, PartialEq, Eq, Default)]
pub struct ProviderStats<Balance: Default> {
    /// Total successfully settled tasks.
    pub total_tasks: u64,
    /// Total VTRS earned across all settled tasks.
    pub total_earned: Balance,
    /// Reputation score; starts at [`INITIAL_REPUTATION`], rises on settle,
    /// falls on slash.
    pub reputation_score: u32,
    /// Number of times this provider has been slashed.
    pub slash_count: u32,
}

/// Convenience alias for the runtime's balance type.
pub type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// Reputation score awarded the first time a provider appears.
pub const INITIAL_REPUTATION: u32 = 1_000;
/// Reputation reward per successful settle.
pub const REPUTATION_REWARD: u32 = 1;
/// Reputation penalty per slash.
pub const REPUTATION_PENALTY: u32 = 100;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Overarching event type.
        type RuntimeEvent: From<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// VTRS balance handler.
        type Currency: Currency<Self::AccountId>;

        /// Origin allowed to submit receipts (the off-chain orchestration oracle).
        type OracleOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

        /// Origin allowed to slash providers and force-update stats. Typically
        /// `EnsureRoot` or a council collective.
        type ManageOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Percentage of escrowed funds that is burned on slash. The remainder
        /// stays in the pallet account and can be swept to a treasury via a
        /// separate runtime extrinsic.
        #[pallet::constant]
        type SlashBurnPercent: Get<Percent>;

        /// Pallet account that holds escrow funds. Derived from this `PalletId`.
        #[pallet::constant]
        type PalletId: Get<PalletId>;
    }

    /// All recorded task receipts, keyed by their off-chain task ID.
    #[pallet::storage]
    pub type TaskReceipts<T: Config> =
        StorageMap<_, Blake2_128Concat, H256, TaskReceipt<T::AccountId, BalanceOf<T>>>;

    /// Reputation and lifetime totals per provider.
    #[pallet::storage]
    pub type ProviderStatsMap<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        ProviderStats<BalanceOf<T>>,
        ValueQuery,
    >;

    /// Escrowed VTRS held against a not-yet-settled task.
    #[pallet::storage]
    pub type EscrowBalance<T: Config> = StorageMap<_, Blake2_128Concat, H256, BalanceOf<T>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A receipt was submitted by the oracle and funds were escrowed.
        TaskReceiptSubmitted {
            /// The task identifier.
            task_id: H256,
            /// The provider that fulfilled the task.
            provider_id: T::AccountId,
            /// Escrowed amount.
            cost_vtrs: BalanceOf<T>,
        },
        /// A task was settled and funds were released to the provider.
        TaskSettled {
            /// The task identifier.
            task_id: H256,
            /// The provider that received payment.
            provider_id: T::AccountId,
            /// Amount actually paid out.
            amount_paid: BalanceOf<T>,
        },
        /// A provider was slashed for a disputed task.
        ProviderSlashed {
            /// The provider that was slashed.
            provider_id: T::AccountId,
            /// Amount removed from escrow.
            amount: BalanceOf<T>,
            /// Reason supplied by the slashing origin.
            reason: BoundedVec<u8, ConstU32<256>>,
        },
        /// Funds were placed in escrow for a task. Emitted alongside
        /// [`Event::TaskReceiptSubmitted`] so subscribers can track escrow
        /// flow independently of receipt metadata.
        EscrowHeld {
            /// The task identifier.
            task_id: H256,
            /// Escrowed amount.
            amount: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// No receipt exists for the given task ID.
        TaskNotFound,
        /// The receipt has already been settled or slashed.
        AlreadySettled,
        /// The pallet escrow account does not have enough funds to release.
        InsufficientEscrow,
        /// The provider account in the receipt doesn't match the slash target.
        ProviderNotRegistered,
        /// The supplied privacy tier byte is not in `{0, 1, 2}`.
        InvalidPrivacyTier,
        /// A receipt with this task ID already exists.
        DuplicateReceipt,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Submit a task receipt and escrow `cost_vtrs` from the oracle account.
        ///
        /// The oracle is the off-chain orchestration layer; it attests that a
        /// task was completed and pays into escrow on the provider's behalf.
        /// Settlement is a separate extrinsic so that disputes can occur
        /// between submission and payout.
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(40_000_000, 4_000))]
        pub fn submit_receipt(
            origin: OriginFor<T>,
            task_id: H256,
            provider_id: T::AccountId,
            cost_vtrs: BalanceOf<T>,
            privacy_tier: u8,
        ) -> DispatchResult {
            let oracle = T::OracleOrigin::ensure_origin(origin)?;

            ensure!(
                !TaskReceipts::<T>::contains_key(task_id),
                Error::<T>::DuplicateReceipt
            );
            let tier =
                PrivacyTier::from_u8(privacy_tier).ok_or(Error::<T>::InvalidPrivacyTier)?;

            // Move funds from the oracle into escrow held by the pallet account.
            T::Currency::transfer(
                &oracle,
                &Self::account_id(),
                cost_vtrs,
                ExistenceRequirement::AllowDeath,
            )?;

            let timestamp: u64 =
                <frame_system::Pallet<T>>::block_number().saturated_into::<u64>();

            let receipt = TaskReceipt {
                provider_id: provider_id.clone(),
                cost_vtrs,
                privacy_tier: tier,
                timestamp,
                status: ReceiptStatus::Pending,
            };

            TaskReceipts::<T>::insert(task_id, receipt);
            EscrowBalance::<T>::insert(task_id, cost_vtrs);

            Self::deposit_event(Event::TaskReceiptSubmitted {
                task_id,
                provider_id,
                cost_vtrs,
            });
            Self::deposit_event(Event::EscrowHeld {
                task_id,
                amount: cost_vtrs,
            });

            Ok(())
        }

        /// Release escrowed funds for a completed task to the provider and
        /// bump the provider's reputation. Callable by any signed origin —
        /// the receipt is the source of truth for who gets paid.
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(40_000_000, 4_000))]
        pub fn settle_task(origin: OriginFor<T>, task_id: H256) -> DispatchResult {
            let _who = ensure_signed(origin)?;

            let mut receipt =
                TaskReceipts::<T>::get(task_id).ok_or(Error::<T>::TaskNotFound)?;
            ensure!(
                receipt.status == ReceiptStatus::Pending,
                Error::<T>::AlreadySettled
            );

            let escrow =
                EscrowBalance::<T>::get(task_id).ok_or(Error::<T>::InsufficientEscrow)?;

            T::Currency::transfer(
                &Self::account_id(),
                &receipt.provider_id,
                escrow,
                ExistenceRequirement::AllowDeath,
            )?;

            receipt.status = ReceiptStatus::Settled;
            let provider = receipt.provider_id.clone();
            TaskReceipts::<T>::insert(task_id, receipt);
            EscrowBalance::<T>::remove(task_id);

            Self::do_update_provider_stats(&provider, true, escrow);

            Self::deposit_event(Event::TaskSettled {
                task_id,
                provider_id: provider,
                amount_paid: escrow,
            });

            Ok(())
        }

        /// Slash a provider for a disputed task. Burns the configured share of
        /// escrow, leaves the rest in the pallet account, marks the receipt
        /// `Slashed`, and decrements the provider's reputation.
        #[pallet::call_index(10)]
        #[pallet::weight(Weight::from_parts(50_000_000, 5_000))]
        pub fn slash_provider(
            origin: OriginFor<T>,
            provider_id: T::AccountId,
            task_id: H256,
            reason: BoundedVec<u8, ConstU32<256>>,
        ) -> DispatchResult {
            T::ManageOrigin::ensure_origin(origin)?;

            let mut receipt =
                TaskReceipts::<T>::get(task_id).ok_or(Error::<T>::TaskNotFound)?;
            ensure!(
                receipt.provider_id == provider_id,
                Error::<T>::ProviderNotRegistered
            );
            ensure!(
                receipt.status == ReceiptStatus::Pending,
                Error::<T>::AlreadySettled
            );

            let escrow =
                EscrowBalance::<T>::get(task_id).ok_or(Error::<T>::InsufficientEscrow)?;

            let burn_pct = T::SlashBurnPercent::get();
            let burn_amount = burn_pct.mul_floor(escrow);

            if !burn_amount.is_zero() {
                let neg = T::Currency::withdraw(
                    &Self::account_id(),
                    burn_amount,
                    WithdrawReasons::all(),
                    ExistenceRequirement::AllowDeath,
                )?;
                // Dropping a NegativeImbalance reduces total issuance — i.e. burns the funds.
                drop(neg);
            }

            receipt.status = ReceiptStatus::Slashed;
            TaskReceipts::<T>::insert(task_id, receipt);
            EscrowBalance::<T>::remove(task_id);

            ProviderStatsMap::<T>::mutate(&provider_id, |stats| {
                Self::ensure_initialized(stats);
                stats.slash_count = stats.slash_count.saturating_add(1);
                stats.reputation_score =
                    stats.reputation_score.saturating_sub(REPUTATION_PENALTY);
            });

            Self::deposit_event(Event::ProviderSlashed {
                provider_id,
                amount: escrow,
                reason,
            });

            Ok(())
        }

        /// Manually update a provider's running totals. Governance/admin only —
        /// used to correct desync between off-chain analytics and on-chain
        /// state, or to register a fresh provider.
        #[pallet::call_index(11)]
        #[pallet::weight(Weight::from_parts(20_000_000, 2_000))]
        pub fn update_provider_stats(
            origin: OriginFor<T>,
            provider_id: T::AccountId,
            task_completed: bool,
            cost: BalanceOf<T>,
        ) -> DispatchResult {
            T::ManageOrigin::ensure_origin(origin)?;
            Self::do_update_provider_stats(&provider_id, task_completed, cost);
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// Pallet escrow account derived from `T::PalletId`.
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        /// Initialize a freshly-defaulted `ProviderStats` with the starting
        /// reputation. Idempotent — safe to call on already-seeded entries
        /// because we only set the score when every counter is still zero.
        fn ensure_initialized(stats: &mut ProviderStats<BalanceOf<T>>) {
            if stats.reputation_score == 0
                && stats.total_tasks == 0
                && stats.slash_count == 0
            {
                stats.reputation_score = INITIAL_REPUTATION;
            }
        }

        /// Internal stats updater shared by `settle_task` and the admin
        /// `update_provider_stats` extrinsic.
        fn do_update_provider_stats(
            provider: &T::AccountId,
            task_completed: bool,
            cost: BalanceOf<T>,
        ) {
            ProviderStatsMap::<T>::mutate(provider, |stats| {
                Self::ensure_initialized(stats);
                if task_completed {
                    stats.total_tasks = stats.total_tasks.saturating_add(1);
                    stats.total_earned = stats.total_earned.saturating_add(cost);
                    stats.reputation_score =
                        stats.reputation_score.saturating_add(REPUTATION_REWARD);
                }
            });
        }
    }
}
