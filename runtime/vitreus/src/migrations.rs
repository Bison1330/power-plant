#![allow(clippy::collapsible_else_if, unused_parens)]

use super::*;

pub type Permanent = (
    pallet_xcm::migration::MigrateToLatestXcmVersion<Runtime>,
    pallet_energy_generation::migrations::FixCooperatorStake<Runtime>,
);

pub type V0213 =
    (InitTechnicalCommitteeTreasury, pallet_privileges::migration::MigrateToV1<Runtime>);

/// D4: `PoolInfo` gains a fee-routing snapshot; existing pools get zero
/// routing (100% of fees stay in the pool), including the launchpad pool that
/// graduated before D4.
// 221: the launch treasury. D9 re-encodes every pool (DEX v2 → v3) and L1
// every launch (launchpad v0 → v1) before the vault is funded, so the sinks
// the treasury reads exist in their new shape first. v1 and v2 are already
// on chain (219, 220) and skip themselves.
// 222: the review's seven fixes (pallets/REVIEW_2026-09-17.md). R1 needs a
// recount of LnrgAccounted on a chain that sold under the old rule; the
// rest change no storage shape.
#[cfg(feature = "testnet-runtime")]
pub type Unreleased = (
    pallet_vitreus_dex::migrations::v1::MigrateToV1<Runtime>,
    pallet_vitreus_dex::migrations::v2::MigrateToV2<Runtime>,
    pallet_vitreus_dex::migrations::v3::MigrateToV3<Runtime>,
    pallet_launchpad::migrations::v1::MigrateToV1<Runtime>,
    crate::launch_treasury::FundLaunchTreasuryVault,
    pallet_launch_treasury::migrations::v1::MigrateToV1<Runtime>,
);
#[cfg(not(feature = "testnet-runtime"))]
pub type Unreleased = (
    pallet_vitreus_dex::migrations::v1::MigrateToV1<Runtime>,
    pallet_vitreus_dex::migrations::v2::MigrateToV2<Runtime>,
    pallet_vitreus_dex::migrations::v3::MigrateToV3<Runtime>,
);

pub struct InitTechnicalCommitteeTreasury;
impl frame_support::traits::OnRuntimeUpgrade for InitTechnicalCommitteeTreasury {
    fn on_runtime_upgrade() -> Weight {
        if !System::account_exists(&TechnicalCommitteeTreasury::account_id()) {
            let amount = <Balances as Currency<_>>::minimum_balance();
            let res = <Balances as Currency<_>>::transfer(
                &Treasury::account_id(),
                &TechnicalCommitteeTreasury::account_id(),
                amount,
                ExistenceRequirement::KeepAlive,
            );

            match res {
                Ok(_) => {
                    log::info!("Transfer {} tokens from Treasury to TechnicalTreasury", amount)
                },
                Err(_) => log::warn!("Failed to initialize TechnicalTreasury"),
            }
        }

        Weight::zero()
    }
}
